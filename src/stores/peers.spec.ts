import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { makePeer } from '@/test/factories';

vi.mock('@/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/ipc')>()),
  listPeers: vi.fn(),
}));

import * as ipc from '@/ipc';

import { usePeerStore } from './peers';

beforeEach(() => {
  setActivePinia(createPinia());
  vi.mocked(ipc.listPeers).mockReset();
});

describe('the peer store', () => {
  it('returns every peer while the search is empty or only whitespace', () => {
    const peers = usePeerStore();
    peers.replace([makePeer({ deviceId: 'a', nickname: 'Alice' }), makePeer({ deviceId: 'b' })]);

    expect(peers.visible).toHaveLength(2);

    peers.query = '   ';
    expect(peers.visible).toHaveLength(2);
  });

  it('matches the nickname as a trimmed, case-insensitive substring', () => {
    const peers = usePeerStore();
    peers.replace([
      makePeer({ deviceId: 'a', nickname: 'Alice' }),
      makePeer({ deviceId: 'b', nickname: 'Bob' }),
      makePeer({ deviceId: 'c', nickname: 'Alicia' }),
    ]);

    peers.query = '  ALI  ';

    expect(peers.visible.map((peer) => peer.deviceId)).toEqual(['a', 'c']);
  });

  it('does not treat the query as a regular expression or a whole-word match', () => {
    const peers = usePeerStore();
    peers.replace([makePeer({ deviceId: 'a', nickname: 'a.c' })]);

    peers.query = '.';

    expect(peers.visible.map((peer) => peer.deviceId)).toEqual(['a']);
  });

  it('resolves the selected peer by device identifier and clears it when it is gone', () => {
    const peers = usePeerStore();
    peers.replace([makePeer({ deviceId: 'a' }), makePeer({ deviceId: 'b' })]);
    expect(peers.selected).toBeNull();

    peers.select('b');
    expect(peers.selected?.deviceId).toBe('b');

    peers.select('missing');
    expect(peers.selected).toBeNull();
  });

  it('sums the unread counter over every peer', () => {
    const peers = usePeerStore();
    peers.replace([
      makePeer({ deviceId: 'a', unread: 2 }),
      makePeer({ deviceId: 'b', unread: 5 }),
      makePeer({ deviceId: 'c', unread: 0 }),
    ]);

    expect(peers.unreadTotal).toBe(7);
  });

  it('finds a peer by identifier', () => {
    const peers = usePeerStore();
    peers.replace([makePeer({ deviceId: 'a', nickname: 'Alice' })]);

    expect(peers.get('a')?.nickname).toBe('Alice');
    expect(peers.get('missing')).toBeUndefined();
  });

  it('keeps the selected peer when it survives a replacement', () => {
    const peers = usePeerStore();
    peers.replace([makePeer({ deviceId: 'a' }), makePeer({ deviceId: 'b' })]);
    peers.select('b');

    peers.replace([makePeer({ deviceId: 'b' }), makePeer({ deviceId: 'c' })]);

    expect(peers.selectedId).toBe('b');
    expect(peers.selected?.deviceId).toBe('b');
  });

  it('drops the selection when the chosen peer disappears from the list', () => {
    const peers = usePeerStore();
    peers.replace([makePeer({ deviceId: 'a' }), makePeer({ deviceId: 'b' })]);
    peers.select('b');

    peers.replace([makePeer({ deviceId: 'a' })]);

    expect(peers.selectedId).toBeNull();
    expect(peers.selected).toBeNull();
  });

  it('loads the list from the host and replaces what was there', async () => {
    const peers = usePeerStore();
    peers.replace([makePeer({ deviceId: 'stale' })]);
    vi.mocked(ipc.listPeers).mockResolvedValue([makePeer({ deviceId: 'fresh' })]);

    await peers.load();

    expect(ipc.listPeers).toHaveBeenCalledTimes(1);
    expect(peers.peers.map((peer) => peer.deviceId)).toEqual(['fresh']);
  });
});
