import type { DeviceId, Peer } from '@/ipc';

/** The pinned conversations in the order the list gives them. */
export function pinnedIds(peers: readonly Peer[]): DeviceId[] {
  return peers.filter((peer) => peer.pinned).map((peer) => peer.deviceId);
}

/** A copy of `ids` with `id` moved to `to`, clamped inside the list. */
export function movePinned(ids: readonly DeviceId[], id: DeviceId, to: number): DeviceId[] {
  const from = ids.indexOf(id);
  if (from === -1) return [...ids];

  const target = Math.min(Math.max(to, 0), ids.length - 1);
  const next = [...ids];
  next.splice(from, 1);
  next.splice(target, 0, id);
  return next;
}

/** The pinned row under the pointer, clamped to the block. */
export function pinnedDropIndex(
  clientY: number,
  listTop: number,
  scrollTop: number,
  count: number,
  rowHeight: number,
): number {
  if (count <= 0) return 0;
  const index = Math.floor((clientY - listTop + scrollTop) / rowHeight);
  return Math.min(Math.max(index, 0), count - 1);
}
