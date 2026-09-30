/**
 * One conversation: its loaded messages, its paging cursor and its sending state.
 *
 * Messages are held newest-last (the order they are rendered in) while pages are fetched
 * newest-first, because that is the shape every virtualised list and every `scrollTop`
 * calculation expects. Only one conversation is held at a time: keeping every conversation in
 * memory would undo the point of paging.
 */
import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import * as ipc from '@/ipc';
import type { DeviceId, Message, PageCursor } from '@/ipc';

/** How many messages one page holds. Matches the host's default. */
const PAGE_SIZE = 50;

/**
 * Whether `right` sorts after `left`.
 *
 * Timestamp first, identifier second — the same total order the storage index uses, so a
 * conversation loaded by pages and extended by live messages cannot interleave differently
 * from the way it was stored.
 */
function isAfter(left: Message, right: Message): boolean {
  return left.sentAt === right.sentAt
    ? left.id.localeCompare(right.id) < 0
    : left.sentAt < right.sentAt;
}

export const useChatStore = defineStore('chat', () => {
  const peerId = ref<DeviceId | null>(null);
  const messages = ref<readonly Message[]>([]);
  const loading = ref(false);
  const hasMore = ref(false);
  const sending = ref(false);

  /** The oldest loaded message, which is where the previous page starts from. */
  const oldestCursor = computed<PageCursor | null>(() => {
    const oldest = messages.value[0];
    return oldest ? { sentAtMs: oldest.sentAt, id: oldest.id } : null;
  });

  /** A message that has not been acknowledged yet and can therefore still fail. */
  const pendingCount = computed(
    () => messages.value.filter((message) => message.status === 'sending').length,
  );

  /** Opens a conversation, loading its first page. */
  async function open(next: DeviceId | null): Promise<void> {
    peerId.value = next;
    messages.value = [];
    hasMore.value = false;
    if (next === null) return;

    loading.value = true;
    try {
      const page = await ipc.history(next, null, PAGE_SIZE);
      if (peerId.value !== next) return; // the user moved on while the page was loading
      messages.value = page;
      hasMore.value = page.length === PAGE_SIZE;
    } finally {
      loading.value = false;
    }
  }

  /** Loads the page before the oldest loaded message. */
  async function loadOlder(): Promise<void> {
    const current = peerId.value;
    const cursor = oldestCursor.value;
    if (current === null || cursor === null || loading.value) return;

    loading.value = true;
    try {
      const page = await ipc.history(current, cursor, PAGE_SIZE);
      if (peerId.value !== current) return;
      // The cursor is exclusive, so a repeated page would only happen if a message were
      // rewritten; de-duplicating by id keeps that from showing twice.
      const known = new Set(messages.value.map((message) => message.id));
      const older = page.filter((message) => !known.has(message.id));
      messages.value = [...older, ...messages.value];
      hasMore.value = page.length === PAGE_SIZE;
    } finally {
      loading.value = false;
    }
  }

  /** Sends a message, adding it immediately so the composer feels instant. */
  async function send(body: string): Promise<void> {
    const current = peerId.value;
    if (current === null || sending.value) return;

    sending.value = true;
    try {
      const stored = await ipc.sendMessage(current, body);
      if (peerId.value !== current) return;
      add(stored);
    } finally {
      sending.value = false;
    }
  }

  /** Records a message for this conversation, keeping the list in time order. */
  function add(message: Message): void {
    if (message.peer !== peerId.value) return;
    const existing = messages.value.find((known) => known.id === message.id);
    if (existing) {
      update(message.id, { status: message.status, read: message.read });
      return;
    }
    // Sorted on insert rather than on read: the list is rendered on every scroll, and a
    // computed that rebuilds the array would invalidate it every time.
    const index = messages.value.findIndex((known) => isAfter(known, message));
    const next = [...messages.value];
    next.splice(index === -1 ? next.length : index, 0, message);
    messages.value = next;
  }

  /** Applies a delivery-status change, wherever the message came from. */
  function update(id: string, patch: Partial<Message>): void {
    messages.value = messages.value.map((message) =>
      message.id === id ? { ...message, ...patch } : message,
    );
  }

  /** Forgets everything about the open conversation, for the forget path. */
  function clear(deviceId: DeviceId): void {
    if (peerId.value !== deviceId) return;
    messages.value = [];
    hasMore.value = false;
  }

  return {
    peerId,
    messages,
    loading,
    hasMore,
    sending,
    pendingCount,
    open,
    loadOlder,
    send,
    add,
    update,
    clear,
  };
});
