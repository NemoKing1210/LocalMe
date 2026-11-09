// @vitest-environment happy-dom
import { createPinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { VueWrapper } from '@vue/test-utils';

import type { Attachment, AttachmentState } from '@/ipc';
import { makeAttachment } from '@/test/factories';
import { flushPromises, mountView } from '@/test/mount';
import { useUiStore } from '@/stores/ui';
import type { UiStore } from '@/stores/ui';

vi.mock('@/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  // See ChatComposer.spec: the template exposes `ipc` to Vue's setup state, which reads
  // `__v_isRef`; the mock proxy must not throw for that key.
  __v_isRef: false,
  openAttachment: vi.fn(),
  revealAttachment: vi.fn(),
  saveAttachment: vi.fn(),
  downloadAttachment: vi.fn(),
  cancelAttachment: vi.fn(),
  retryAttachment: vi.fn(),
}));

import * as ipc from '@/ipc';

import AttachmentCard from './AttachmentCard.vue';

type MountedCard = {
  wrapper: VueWrapper;
  ui: UiStore;
};

async function mountCard(overrides: Partial<Attachment> = {}): Promise<MountedCard> {
  const pinia = createPinia();
  const wrapper = await mountView(AttachmentCard, {
    pinia,
    props: { attachment: makeAttachment(overrides) },
  });
  await flushPromises();
  return { wrapper, ui: useUiStore(pinia) };
}

beforeEach(() => {
  vi.mocked(ipc.openAttachment).mockResolvedValue(undefined);
  vi.mocked(ipc.revealAttachment).mockResolvedValue(undefined);
  vi.mocked(ipc.saveAttachment).mockResolvedValue(null);
  vi.mocked(ipc.downloadAttachment).mockResolvedValue(null);
  vi.mocked(ipc.cancelAttachment).mockResolvedValue(undefined);
  vi.mocked(ipc.retryAttachment).mockResolvedValue(undefined);
});

describe('an attachment card', () => {
  it('always shows the name and the size', async () => {
    const { wrapper } = await mountCard({ name: 'notes.txt', size: 1024 });

    expect(wrapper.find('.attachment__name').text()).toBe('notes.txt');
    expect(wrapper.find('.attachment__meta').text()).toMatch(/1\s?kB/);
  });

  it('offers open, save and reveal once the transfer is complete', async () => {
    const { wrapper } = await mountCard({ state: 'complete' });
    const labels = ['Open', 'Save as…', 'Show in folder'];

    for (const label of labels) {
      expect(wrapper.find(`button[aria-label="${label}"]`).exists()).toBe(true);
    }
    expect(wrapper.find('button[aria-label="Cancel the transfer"]').exists()).toBe(false);
    expect(wrapper.find('.attachment__retry').exists()).toBe(false);
    expect(wrapper.find('.attachment__progress').exists()).toBe(false);
  });

  it('shows how far a transfer has come and lets it be cancelled', async () => {
    const { wrapper } = await mountCard({ state: 'sending', size: 1000, transferred: 250 });

    expect(wrapper.find('.attachment__meta').text()).toContain('Sending 25%');
    expect(wrapper.find('.attachment__bar').attributes('style')).toContain('inline-size: 25%');
    expect(wrapper.find('button[aria-label="Cancel the transfer"]').exists()).toBe(true);
  });

  it('rounds the progress and reports an empty file as complete', async () => {
    const { wrapper: rounded } = await mountCard({
      state: 'receiving',
      size: 3,
      transferred: 1,
    });
    expect(rounded.find('.attachment__meta').text()).toContain('Receiving 33%');

    const { wrapper: empty } = await mountCard({ state: 'sending', size: 0, transferred: 0 });
    expect(empty.find('.attachment__meta').text()).toContain('Sending 100%');
  });

  it('names every state in the row beside the size', async () => {
    const expected: ReadonlyArray<readonly [AttachmentState, string]> = [
      ['queued', 'Waiting to send'],
      ['sending', 'Sending 50%'],
      ['receiving', 'Receiving 50%'],
      ['cancelled', 'Transfer cancelled'],
      ['failed', 'The file did not arrive'],
    ];

    for (const [state, label] of expected) {
      const { wrapper } = await mountCard({ state, size: 1000, transferred: 500 });
      expect(wrapper.find('.attachment__meta').text(), state).toContain(label);
    }
  });

  it('leaves a completed card with no state label, only the size', async () => {
    const { wrapper } = await mountCard({ state: 'complete', size: 1024, transferred: 1024 });

    expect(wrapper.find('.attachment__meta').text()).toMatch(/1\s?kB/);
    expect(wrapper.find('.attachment__separator').exists()).toBe(false);
  });

  it('offers a retry only for failed or cancelled transfers', async () => {
    const { wrapper: failed } = await mountCard({ state: 'failed' });
    expect(failed.find('.attachment__retry').text()).toContain('Send it again');
    expect(failed.find('.attachment--failed').exists()).toBe(true);

    const { wrapper: cancelled } = await mountCard({ state: 'cancelled' });
    expect(cancelled.find('.attachment__retry').exists()).toBe(true);
    expect(cancelled.find('.attachment--failed').exists()).toBe(false);

    const { wrapper: queued } = await mountCard({ state: 'queued' });
    expect(queued.find('.attachment__retry').exists()).toBe(false);
    expect(queued.classes()).not.toContain('attachment--failed');
  });

  it('calls each host action with the attachment id', async () => {
    const { wrapper } = await mountCard({ state: 'complete', id: 'a-1' });

    await wrapper.find('button[aria-label="Open"]').trigger('click');
    await wrapper.find('button[aria-label="Show in folder"]').trigger('click');
    await flushPromises();
    expect(ipc.openAttachment).toHaveBeenCalledWith('a-1');
    expect(ipc.revealAttachment).toHaveBeenCalledWith('a-1');

    await wrapper.find('button[aria-label="Save as…"]').trigger('click');
    await flushPromises();
    expect(ipc.saveAttachment).toHaveBeenCalledWith('a-1');

    const { wrapper: sending } = await mountCard({ state: 'sending', id: 'a-2' });
    await sending.find('button[aria-label="Cancel the transfer"]').trigger('click');
    await flushPromises();
    expect(ipc.cancelAttachment).toHaveBeenCalledWith('a-2');

    const { wrapper: failed } = await mountCard({ state: 'failed', id: 'a-3' });
    await failed.find('.attachment__retry').trigger('click');
    await flushPromises();
    expect(ipc.retryAttachment).toHaveBeenCalledWith('a-3');
  });

  it('reports a failed host action instead of swallowing it', async () => {
    const { wrapper, ui } = await mountCard({ state: 'complete' });
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.mocked(ipc.openAttachment).mockRejectedValue(new Error('gone'));

    await wrapper.find('button[aria-label="Open"]').trigger('click');
    await flushPromises();

    expect(errorSpy).toHaveBeenCalled();
    expect(ui.notice?.tone).toBe('error');
  });

  it('says where a file was saved, and says nothing when the dialog is dismissed', async () => {
    const { wrapper, ui } = await mountCard({ state: 'complete' });
    vi.mocked(ipc.saveAttachment).mockResolvedValue('/home/user/notes.txt');

    await wrapper.find('button[aria-label="Save as…"]').trigger('click');
    await flushPromises();
    expect(ui.noticeText).toBe('Saved to /home/user/notes.txt');

    ui.dismiss();
    vi.mocked(ipc.saveAttachment).mockResolvedValue(null);
    await wrapper.find('button[aria-label="Save as…"]').trigger('click');
    await flushPromises();
    expect(ui.notice).toBeNull();
  });

  it('reports a storage failure when the save cannot be written', async () => {
    const { wrapper, ui } = await mountCard({ state: 'complete' });
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.mocked(ipc.saveAttachment).mockRejectedValue(new Error('disk full'));

    await wrapper.find('button[aria-label="Save as…"]').trigger('click');
    await flushPromises();

    expect(errorSpy).toHaveBeenCalled();
    expect(ui.noticeText).toBe('LocalMe could not read its own database.');
  });

  it('never previews a picture: the gallery draws images', async () => {
    const { wrapper } = await mountCard({
      kind: 'image',
      name: 'cat.png',
      path: '/pics/cat.png',
      state: 'complete',
    });

    expect(wrapper.find('img').exists()).toBe(false);
    expect(wrapper.find('.attachment__glyph').exists()).toBe(true);
  });

  it('offers a download for an incoming file that was not fetched yet', async () => {
    const { wrapper } = await mountCard({
      direction: 'incoming',
      state: 'offered',
      path: null,
      id: 'a-9',
    });

    expect(wrapper.find('.attachment__meta').text()).toContain('Not downloaded');
    expect(wrapper.find('button[aria-label="Download…"]').exists()).toBe(true);
    expect(wrapper.find('button[aria-label="Open"]').exists()).toBe(false);
    expect(wrapper.find('button[aria-label="Save as…"]').exists()).toBe(false);
    expect(wrapper.find('button[aria-label="Show in folder"]').exists()).toBe(false);

    await wrapper.find('button[aria-label="Download…"]').trigger('click');
    await flushPromises();
    expect(ipc.downloadAttachment).toHaveBeenCalledWith('a-9');
  });

  it('says nothing when the download dialog is dismissed', async () => {
    const { wrapper, ui } = await mountCard({
      direction: 'incoming',
      state: 'offered',
      path: null,
    });
    vi.mocked(ipc.downloadAttachment).mockResolvedValue(null);

    await wrapper.find('button[aria-label="Download…"]').trigger('click');
    await flushPromises();
    expect(ui.notice).toBeNull();
  });

  it('waits quietly on the sending side of an offered file', async () => {
    const { wrapper } = await mountCard({ direction: 'outgoing', state: 'offered' });

    expect(wrapper.find('.attachment__meta').text()).toContain('Waiting to be downloaded');
    expect(wrapper.find('button[aria-label="Download…"]').exists()).toBe(false);
    expect(wrapper.find('button[aria-label="Cancel the transfer"]').exists()).toBe(false);
    expect(wrapper.find('.attachment__progress').exists()).toBe(false);
  });
});
