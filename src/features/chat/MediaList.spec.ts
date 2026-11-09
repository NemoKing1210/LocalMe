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
  cancelAttachment: vi.fn(),
}));

import * as ipc from '@/ipc';

import MediaList from './MediaList.vue';

function audio(overrides: Partial<Attachment> = {}): Attachment {
  return makeAttachment({
    kind: 'audio',
    name: 'song.mp3',
    state: 'complete',
    path: '/tmp/song.mp3',
    ...overrides,
  });
}

function video(overrides: Partial<Attachment> = {}): Attachment {
  return makeAttachment({
    kind: 'video',
    name: 'clip.mp4',
    state: 'complete',
    path: '/tmp/clip.mp4',
    ...overrides,
  });
}

async function mountList(attachments: readonly Attachment[]): Promise<VueWrapper> {
  const wrapper = await mountView(MediaList, {
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
  vi.mocked(ipc.cancelAttachment).mockResolvedValue(undefined);
});

describe('the media list', () => {
  it('plays a completed song inline', async () => {
    const wrapper = await mountList([audio()]);

    const player = wrapper.get('audio');
    expect(player.attributes('src')).toBe('file:///tmp/song.mp3');
    expect(player.attributes('controls')).toBeDefined();
    expect(player.attributes('preload')).toBe('metadata');
    expect(wrapper.find('video').exists()).toBe(false);
  });

  it('plays a completed clip inline', async () => {
    const wrapper = await mountList([video()]);

    const player = wrapper.get('video');
    expect(player.attributes('src')).toBe('file:///tmp/clip.mp4');
    expect(player.attributes('controls')).toBeDefined();
    expect(wrapper.find('audio').exists()).toBe(false);
  });

  it('shows the name and size whether or not the file has arrived', async () => {
    const wrapper = await mountList([
      audio({ name: 'song.mp3', size: 2048 }),
      audio({ id: 'second', name: 'other.mp3', state: 'offered', path: null }),
    ]);

    const rows = wrapper.findAll('.media');
    expect(rows).toHaveLength(2);
    expect(rows[0]?.find('.media__name').text()).toBe('song.mp3');
    expect(rows[0]?.find('.media__meta').text()).toMatch(/2\s?kB/);
    expect(rows[1]?.find('.media__name').text()).toBe('other.mp3');
  });

  it('shows progress and no player while the file is still arriving', async () => {
    const wrapper = await mountList([
      audio({ state: 'receiving', path: null, size: 1000, transferred: 250 }),
    ]);

    expect(wrapper.find('audio').exists()).toBe(false);
    expect(wrapper.find('.media__progress').exists()).toBe(true);
    expect(wrapper.find('.media__bar').attributes('style')).toContain('inline-size: 25%');
    expect(wrapper.find('.media__meta').text()).toContain('Receiving 25%');
  });

  it('keeps open, save and reveal on a playable row', async () => {
    const wrapper = await mountList([audio({ id: 'a-1' })]);

    for (const label of ['Open', 'Save as…', 'Show in folder']) {
      expect(wrapper.find(`button[aria-label="${label}"]`).exists()).toBe(true);
    }

    await wrapper.get('button[aria-label="Save as…"]').trigger('click');
    await flushPromises();
    expect(ipc.saveAttachment).toHaveBeenCalledWith('a-1');
  });

  it('says so and keeps the actions when a player cannot decode the file', async () => {
    const wrapper = await mountList([audio({ id: 'a-2' })]);

    await wrapper.get('audio').trigger('error');

    expect(wrapper.find('audio').exists()).toBe(false);
    expect(wrapper.text()).toContain('This file cannot be played here');
    // A file the platform cannot play is still a file: every action stays.
    expect(wrapper.find('button[aria-label="Open"]').exists()).toBe(true);
    expect(wrapper.find('button[aria-label="Save as…"]').exists()).toBe(true);
    expect(wrapper.find('button[aria-label="Show in folder"]').exists()).toBe(true);
  });

  it('offers a retry for a song that failed', async () => {
    const wrapper = await mountList([audio({ id: 'a-3', state: 'failed', path: null })]);

    const retry = wrapper.get('.media__retry');
    expect(retry.text()).toContain('Send it again');

    await retry.trigger('click');
    await flushPromises();
    expect(ipc.retryAttachment).toHaveBeenCalledWith('a-3');
  });
});
