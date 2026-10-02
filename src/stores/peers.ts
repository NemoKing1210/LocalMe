import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import * as ipc from '@/ipc';
import type { DeviceId, Peer } from '@/ipc';

/** Case-insensitive substring match over the nickname, mirroring the host's rule. */
function matches(peer: Peer, needle: string): boolean {
  return peer.nickname.toLocaleLowerCase().includes(needle);
}

export const usePeerStore = defineStore('peers', () => {
  const peers = ref<readonly Peer[]>([]);
  const query = ref('');
  const selectedId = ref<DeviceId | null>(null);

  const visible = computed<readonly Peer[]>(() => {
    const needle = query.value.trim().toLocaleLowerCase();
    if (needle.length === 0) return peers.value;
    return peers.value.filter((peer) => matches(peer, needle));
  });

  const selected = computed<Peer | null>(
    () => peers.value.find((peer) => peer.deviceId === selectedId.value) ?? null,
  );

  const unreadTotal = computed(() => peers.value.reduce((total, peer) => total + peer.unread, 0));

  function get(deviceId: DeviceId): Peer | undefined {
    return peers.value.find((peer) => peer.deviceId === deviceId);
  }

  function replace(next: readonly Peer[]): void {
    peers.value = next;
    // A peer that disappeared — forgotten, or removed while this window was in the tray —
    // must not stay selected, or the chat view would render a conversation with nobody in it.
    if (selectedId.value !== null && !next.some((peer) => peer.deviceId === selectedId.value)) {
      selectedId.value = null;
    }
  }

  function select(deviceId: DeviceId | null): void {
    selectedId.value = deviceId;
  }

  async function load(): Promise<void> {
    replace(await ipc.listPeers());
  }

  return {
    peers,
    query,
    visible,
    selectedId,
    selected,
    unreadTotal,
    get,
    replace,
    select,
    load,
  };
});
