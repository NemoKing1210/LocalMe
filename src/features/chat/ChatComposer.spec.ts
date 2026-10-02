// @vitest-environment happy-dom
import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Pinia } from 'pinia';
import type { VueWrapper } from '@vue/test-utils';

import type { Peer } from '@/ipc';
import { makeFilePick, makePeer } from '@/test/factories';
import { flushPromises, mountView } from '@/test/mount';
import { useUiStore } from '@/stores/ui';
import type { UiStore } from '@/stores/ui';

vi.mock('@/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  // The template exposes the `ipc` namespace to Vue's setup state, which probes refs by reading
  // `__v_isRef`; the mock proxy throws for keys it does not know, so satisfy that probe here.
  __v_isRef: false,
  pickFiles: vi.fn(),
  inspectFiles: vi.fn(),
  fileSrc: vi.fn(),
}));

/**
 * The web view hands the drop event to the component through this callback; capturing it lets a
 * test act as the host instead of needing a real Tauri window.
 */
const tauri = vi.hoisted(() => ({
  handlers: [] as ((event: { payload: { type: string; paths?: string[] } }) => void)[],
  onDragDropEvent: vi.fn(),
}));

vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({ onDragDropEvent: tauri.onDragDropEvent }),
}));

import * as ipc from '@/ipc';

import ChatComposer from './ChatComposer.vue';

const SEND = 'button[aria-label="Send"]';
const ATTACH = 'button[aria-label="Attach files"]';

interface MountOverrides {
  readonly peer?: Peer;
  readonly sending?: boolean;
}

type MountedComposer = {
  wrapper: VueWrapper;
  pinia: Pinia;
  ui: UiStore;
};

async function mountComposer(overrides: MountOverrides = {}): Promise<MountedComposer> {
  const pinia = createPinia();
  const wrapper = await mountView(ChatComposer, {
    pinia,
    props: { peer: makePeer(), sending: false, ...overrides },
  });
  await flushPromises();
  return { wrapper, pinia, ui: useUiStore(pinia) };
}

beforeEach(() => {
  tauri.handlers = [];
  tauri.onDragDropEvent.mockImplementation(
    (handler: (event: { payload: { type: string; paths?: string[] } }) => void) => {
      tauri.handlers.push(handler);
      return Promise.resolve(() => {});
    },
  );
  vi.mocked(ipc.pickFiles).mockResolvedValue([]);
  vi.mocked(ipc.inspectFiles).mockResolvedValue([]);
  vi.mocked(ipc.fileSrc).mockImplementation((path: string) => `file://${path}`);
  setActivePinia(createPinia());
});

describe('the composer', () => {
  it('starts unable to send and names the peer in the placeholder', async () => {
    const { wrapper } = await mountComposer();
    const textarea = wrapper.find('textarea');

    expect(wrapper.find(SEND).attributes('disabled')).toBeDefined();
    expect(textarea.attributes('placeholder')).toBe('Message Alice');
    expect(textarea.attributes('aria-label')).toBe('Message Alice');
    expect(wrapper.text()).toContain('Enter sends, Shift+Enter starts a new line');
  });

  it('enables sending once the draft holds non-whitespace and emits on click', async () => {
    const { wrapper } = await mountComposer();
    const textarea = wrapper.find('textarea');

    await textarea.setValue('hello');
    expect(textarea.element.value).toBe('hello');
    expect(wrapper.find(SEND).attributes('disabled')).toBeUndefined();

    await wrapper.find(SEND).trigger('click');
    expect(wrapper.emitted('send')).toEqual([['hello', []]]);
    expect(textarea.element.value).toBe('');
    expect(wrapper.find(SEND).attributes('disabled')).toBeDefined();
  });

  it('sends on Enter but not on Shift+Enter or while an IME is composing', async () => {
    const { wrapper } = await mountComposer();
    const textarea = wrapper.find('textarea');

    await textarea.setValue('line one');
    await textarea.trigger('keydown', { key: 'Enter', shiftKey: true });
    expect(wrapper.emitted('send')).toBeUndefined();
    expect(textarea.element.value).toBe('line one');

    await textarea.trigger('keydown', { key: 'Enter', isComposing: true });
    expect(wrapper.emitted('send')).toBeUndefined();

    await textarea.trigger('keydown', { key: 'Enter' });
    expect(wrapper.emitted('send')).toEqual([['line one', []]]);
  });

  it('clears the draft on Escape', async () => {
    const { wrapper } = await mountComposer();
    const textarea = wrapper.find('textarea');

    await textarea.setValue('unsent');
    await textarea.trigger('keydown', { key: 'Escape' });

    expect(textarea.element.value).toBe('');
    expect(wrapper.emitted('send')).toBeUndefined();
  });

  it('refuses a whitespace-only body with no attachments', async () => {
    const { wrapper } = await mountComposer();
    const textarea = wrapper.find('textarea');

    await textarea.setValue('   ');
    await textarea.trigger('keydown', { key: 'Enter' });

    expect(wrapper.find(SEND).attributes('disabled')).toBeDefined();
    expect(wrapper.emitted('send')).toBeUndefined();
  });

  it('appends picked files, ignores repeats, and sends their paths with the body', async () => {
    const { wrapper } = await mountComposer();
    vi.mocked(ipc.pickFiles).mockResolvedValue([
      makeFilePick({ path: '/tmp/a.txt', name: 'a.txt' }),
    ]);

    await wrapper.find(ATTACH).trigger('click');
    await flushPromises();
    expect(wrapper.findAll('.composer__file')).toHaveLength(1);

    // Picking the same file again must not duplicate it.
    await wrapper.find(ATTACH).trigger('click');
    await flushPromises();
    expect(wrapper.findAll('.composer__file')).toHaveLength(1);

    const textarea = wrapper.find('textarea');
    await textarea.setValue('see attached');
    await textarea.trigger('keydown', { key: 'Enter' });

    expect(wrapper.emitted('send')).toEqual([['see attached', ['/tmp/a.txt']]]);
    expect(wrapper.findAll('.composer__file')).toHaveLength(0);
    expect(textarea.element.value).toBe('');
  });

  it('caps the list at ten files and says so instead of silently dropping them', async () => {
    const { wrapper, ui } = await mountComposer();
    vi.mocked(ipc.pickFiles).mockResolvedValue(
      Array.from({ length: 11 }, (_value, index) =>
        makeFilePick({ path: `/tmp/f${index}.bin`, name: `f${index}.bin` }),
      ),
    );

    await wrapper.find(ATTACH).trigger('click');
    await flushPromises();

    expect(wrapper.findAll('.composer__file')).toHaveLength(10);
    expect(ui.noticeText).toBe('No more than 10 files in one message');
  });

  it('shows why a file cannot be sent and blocks the send while it is held', async () => {
    const { wrapper } = await mountComposer();
    vi.mocked(ipc.pickFiles).mockResolvedValue([
      makeFilePick({ path: '/tmp/huge.bin', name: 'huge.bin', problem: 'tooLarge' }),
      makeFilePick({ path: '/tmp/dir', name: 'dir', problem: 'directory' }),
      makeFilePick({ path: '/tmp/gone', name: 'gone', problem: 'missing' }),
    ]);

    await wrapper.find(ATTACH).trigger('click');
    await flushPromises();

    const text = wrapper.text();
    expect(text).toContain('Too large');
    expect(text).toContain('Folders cannot be sent');
    expect(text).toContain('The file is no longer there');

    const textarea = wrapper.find('textarea');
    await textarea.setValue('with a broken file');
    expect(wrapper.find(SEND).attributes('disabled')).toBeDefined();
    await textarea.trigger('keydown', { key: 'Enter' });
    expect(wrapper.emitted('send')).toBeUndefined();
  });

  it('blocks sending while a broken file is held, and sends once it is removed', async () => {
    const { wrapper } = await mountComposer();
    vi.mocked(ipc.pickFiles).mockResolvedValue([
      makeFilePick({ path: '/tmp/ok.txt', name: 'ok.txt' }),
      makeFilePick({ path: '/tmp/bad.bin', name: 'bad.bin', problem: 'missing' }),
    ]);

    await wrapper.find(ATTACH).trigger('click');
    await flushPromises();

    // One file the host would refuse blocks the message; dropping the broken one does not
    // silently drop a file the user chose.
    expect(wrapper.find(SEND).attributes('disabled')).toBeDefined();

    await wrapper.find('button[aria-label="Remove bad.bin"]').trigger('click');
    expect(wrapper.find(SEND).attributes('disabled')).toBeUndefined();
    await wrapper.find(SEND).trigger('click');

    expect(wrapper.emitted('send')).toEqual([['', ['/tmp/ok.txt']]]);
  });

  it('removes a pending attachment by its labelled button', async () => {
    const { wrapper } = await mountComposer();
    vi.mocked(ipc.pickFiles).mockResolvedValue([
      makeFilePick({ path: '/tmp/a.txt', name: 'a.txt' }),
    ]);

    await wrapper.find(ATTACH).trigger('click');
    await flushPromises();

    await wrapper.find('button[aria-label="Remove a.txt"]').trigger('click');
    expect(wrapper.findAll('.composer__file')).toHaveLength(0);
  });

  it('shows the drop overlay while dragging and inspects dropped paths', async () => {
    const { wrapper } = await mountComposer();
    vi.mocked(ipc.inspectFiles).mockResolvedValue([
      makeFilePick({ path: '/tmp/dropped.txt', name: 'dropped.txt' }),
    ]);
    const handler = tauri.handlers[0];
    expect(handler).toBeDefined();

    handler?.({ payload: { type: 'enter' } });
    await flushPromises();
    expect(wrapper.find('.composer__drop').exists()).toBe(true);

    handler?.({ payload: { type: 'drop', paths: ['/tmp/dropped.txt'] } });
    await flushPromises();

    expect(wrapper.find('.composer__drop').exists()).toBe(false);
    expect(ipc.inspectFiles).toHaveBeenCalledWith(['/tmp/dropped.txt']);
    expect(wrapper.text()).toContain('dropped.txt');
  });

  it('reports a failed picker and a failed drop inspection like any other host failure', async () => {
    const { wrapper, ui } = await mountComposer();
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.mocked(ipc.pickFiles).mockRejectedValue(new Error('no picker'));

    await wrapper.find(ATTACH).trigger('click');
    await flushPromises();
    expect(errorSpy).toHaveBeenCalled();
    expect(ui.notice?.tone).toBe('error');

    const handler = tauri.handlers[0];
    vi.mocked(ipc.inspectFiles).mockRejectedValue(new Error('unreadable'));
    handler?.({ payload: { type: 'drop', paths: ['/tmp/x'] } });
    await flushPromises();
    expect(ui.notice?.tone).toBe('error');
  });

  it('refuses a second send while one is in flight', async () => {
    const { wrapper } = await mountComposer({ sending: true });
    const textarea = wrapper.find('textarea');

    await textarea.setValue('again');
    expect(wrapper.find(SEND).attributes('disabled')).toBeDefined();
    await textarea.trigger('keydown', { key: 'Enter' });
    await wrapper.find(SEND).trigger('click');

    expect(wrapper.emitted('send')).toBeUndefined();
  });

  it('warns with a counter past the threshold and an error past the limit', async () => {
    const { wrapper } = await mountComposer();
    const textarea = wrapper.find('textarea');

    await textarea.setValue('a'.repeat(6401));
    expect(wrapper.find('.composer__counter').text()).toBe('6401 / 8000');

    await textarea.setValue('a'.repeat(8001));
    expect(wrapper.find('.composer__error').text()).toBe(
      'That message is longer than 8000 characters.',
    );
    expect(wrapper.find(SEND).attributes('disabled')).toBeDefined();
  });

  it('previews an image pick from fileSrc and falls back to a glyph when it fails to load', async () => {
    const { wrapper } = await mountComposer();
    vi.mocked(ipc.pickFiles).mockResolvedValue([
      makeFilePick({ path: '/pics/cat.png', name: 'cat.png', kind: 'image' }),
    ]);

    await wrapper.find(ATTACH).trigger('click');
    await flushPromises();

    const thumb = wrapper.find('img.composer__thumb');
    expect(thumb.attributes('src')).toBe('file:///pics/cat.png');

    await thumb.trigger('error');
    expect(wrapper.find('img.composer__thumb').exists()).toBe(false);
    expect(wrapper.find('.composer__thumb--glyph').exists()).toBe(true);
  });

  it('tells an away peer that messages wait until they return', async () => {
    const { wrapper } = await mountComposer({ peer: makePeer({ online: false }) });

    expect(wrapper.find('.composer__offline').exists()).toBe(true);
    expect(wrapper.find('.composer__offline').text()).toContain('Messages will be sent');
  });
});
