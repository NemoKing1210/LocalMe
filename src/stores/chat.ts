import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import * as ipc from '@/ipc';
import type { DeviceId, Message, PageCursor } from '@/ipc';

/** How many messages one page holds. Matches the host's default. */
const PAGE_SIZE = 50;

function precedes(left: Message, right: Message): boolean {
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

  const undrawn = new Set<string>();
  const UNDRAWN_LIMIT = 64;

  const oldestCursor = computed<PageCursor | null>(() => {
    const oldest = messages.value[0];
    return oldest ? { sentAtMs: oldest.sentAt, id: oldest.id } : null;
  });

  /** Messages that have not been delivered yet: waiting in the outbox or on the wire. */
  const pendingCount = computed(
    () =>
      messages.value.filter(
        (message) => message.status === 'queued' || message.status === 'sending',
      ).length,
  );

  async function open(next: DeviceId | null): Promise<void> {
    peerId.value = next;
    messages.value = [];
    hasMore.value = false;
    undrawn.clear();
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

  function add(message: Message): void {
    if (message.peer !== peerId.value) return;
    const existing = messages.value.find((known) => known.id === message.id);
    if (existing) {
      update(message.id, {
        status: message.status,
        read: message.read,
        deliveredAt: message.deliveredAt,
      });
      return;
    }
    // Sorted on insert rather than on read: the list is rendered on every scroll, and a
    // computed that rebuilt the array would invalidate it on every frame. The insertion point
    // is the first message that sorts *after* the new one, so a message that arrives out of
    // order — a page being merged, a clock moving — still lands in the right place.
    const index = messages.value.findIndex((known) => precedes(message, known));
    const next = [...messages.value];
    next.splice(index === -1 ? next.length : index, 0, message);
    messages.value = next;
    if (undrawn.size >= UNDRAWN_LIMIT) {
      const oldest = undrawn.values().next().value;
      if (oldest !== undefined) undrawn.delete(oldest);
    }
    undrawn.add(message.id);
  }

  function consumeEntrance(id: string): boolean {
    return undrawn.delete(id);
  }

  function update(id: string, patch: Partial<Message>): void {
    messages.value = messages.value.map((message) =>
      message.id === id ? { ...message, ...patch } : message,
    );
  }

  function clear(deviceId: DeviceId): void {
    if (peerId.value !== deviceId) return;
    messages.value = [];
    hasMore.value = false;
    undrawn.clear();
  }

  return {
    peerId,
    messages,
    loading,
    hasMore,
    sending,
    pendingCount,
    oldestCursor,
    open,
    loadOlder,
    send,
    add,
    update,
    clear,
    consumeEntrance,
  };
});
