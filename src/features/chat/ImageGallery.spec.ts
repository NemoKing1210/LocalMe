// @vitest-environment happy-dom
import { createPinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { VueWrapper } from '@vue/test-utils';

import type { Attachment } from '@/ipc';
import { makeAttachment } from '@/test/factories';
import { flushPromises, mountView } from '@/test/mount';

vi.mock('@/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  __v_isRef: false,
  fileSrc: vi.fn(),
  openAttachment: vi.fn(),
  revealAttachment: vi.fn(),
  saveAttachment: vi.fn(),
  retryAttachment: vi.fn(),
}));

import * as ipc from '@/ipc';

import ImageGallery from './ImageGallery.vue';

function image(overrides: Partial<Attachment> = {}): Attachment {
  return makeAttachment({
    kind: 'image',
    name: 'one.png',
    state: 'complete',
    path: '/tmp/one.png',
    ...overrides,
  });
}

async function mountGallery(attachments: readonly Attachment[]): Promise<VueWrapper> {
  const wrapper = await mountView(ImageGallery, {
    pinia: createPinia(),
    props: { attachments },
  });
  await flushPromises();
  return wrapper;
}

beforeEach(() => {
  vi.mocked(ipc.fileSrc).mockImplementation((path: string) => `file://${path}`);
  vi.mocked(ipc.openAttachment).mockResolvedValue(undefined);
  vi.mocked(ipc.revealAttachment).mockResolvedValue(undefined);
  vi.mocked(ipc.saveAttachment).mockResolvedValue(null);
  vi.mocked(ipc.retryAttachment).mockResolvedValue(undefined);
});

describe('the image gallery', () => {
  it('lays out one, two, three, four and many pictures differently', async () => {
    const cases: ReadonlyArray<readonly [number, string]> = [
      [1, 'gallery--single'],
      [2, 'gallery--pair'],
      [3, 'gallery--trio'],
      [4, 'gallery--grid'],
      [5, 'gallery--many'],
    ];

    for (const [count, className] of cases) {
      const attachments = Array.from({ length: count }, (_, index) =>
        image({ id: `image-${index}`, name: `${index}.png`, path: `/tmp/${index}.png` }),
      );
      const wrapper = await mountGallery(attachments);
      expect(wrapper.find('.gallery').classes(), String(count)).toContain(className);
      expect(wrapper.findAll('.gallery__tile')).toHaveLength(count);
    }
  });

  it('draws only images and leaves the other kinds to their own components', async () => {
    const wrapper = await mountGallery([
      image({ id: 'image-1' }),
      makeAttachment({ id: 'file-1', kind: 'file', name: 'notes.txt' }),
      makeAttachment({ id: 'audio-1', kind: 'audio', name: 'song.mp3' }),
    ]);

    expect(wrapper.findAll('.gallery__tile')).toHaveLength(1);
  });

  it('opens a completed picture in the viewer, not in another application', async () => {
    const wrapper = await mountGallery([image({ name: 'cat.png', path: '/pics/cat.png' })]);

    await wrapper.get('button[aria-label="Open cat.png"]').trigger('click');

    expect(wrapper.get<HTMLDialogElement>('dialog.viewer').element.open).toBe(true);
    expect(wrapper.get('.viewer__image').attributes('src')).toBe('file:///pics/cat.png');
    expect(ipc.openAttachment).not.toHaveBeenCalled();
  });

  it('keeps open, save and reveal on a completed tile', async () => {
    const wrapper = await mountGallery([image({ id: 'a-1' })]);

    for (const label of ['Open', 'Save as…', 'Show in folder']) {
      expect(wrapper.find(`button[aria-label="${label}"]`).exists()).toBe(true);
    }

    await wrapper.get('button[aria-label="Save as…"]').trigger('click');
    await flushPromises();
    expect(ipc.saveAttachment).toHaveBeenCalledWith('a-1');
    expect(ipc.openAttachment).not.toHaveBeenCalled();
  });

  it('shows progress instead of a picture while the image is still arriving', async () => {
    const wrapper = await mountGallery([
      image({ state: 'receiving', path: null, size: 1000, transferred: 250 }),
    ]);

    expect(wrapper.find('img').exists()).toBe(false);
    expect(wrapper.find('.gallery__progress').exists()).toBe(true);
    expect(wrapper.find('.gallery__bar').attributes('style')).toContain('inline-size: 25%');
  });

  it('offers a retry for an image that failed', async () => {
    const wrapper = await mountGallery([
      image({ id: 'a-2', state: 'failed', path: null, name: 'sad.png' }),
    ]);

    const retry = wrapper.get('.gallery__retry');
    expect(retry.text()).toContain('Send it again');
    expect(wrapper.find('.gallery__name').text()).toBe('sad.png');

    await retry.trigger('click');
    await flushPromises();
    expect(ipc.retryAttachment).toHaveBeenCalledWith('a-2');
  });

  it('falls back to the glyph and the name when the picture cannot be decoded', async () => {
    const wrapper = await mountGallery([image({ name: 'odd.png' })]);
    expect(wrapper.find('img').exists()).toBe(true);

    await wrapper.get('img').trigger('error');

    expect(wrapper.find('img').exists()).toBe(false);
    expect(wrapper.find('.gallery__glyph').exists()).toBe(true);
    expect(wrapper.find('.gallery__name').text()).toBe('odd.png');
  });
});
