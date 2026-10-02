import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Message } from '@/ipc';

vi.mock('@/ipc', () => ({
  history: vi.fn().mockResolvedValue([]),
  sendMessage: vi.fn(),
}));

import { useChatStore } from './chat';

const PEER = '018f2b9c-0000-7000-8000-0000000000aa';
const OTHER = '018f2b9c-0000-7000-8000-0000000000bb';

function message(id: string, sentAt: number, peer = PEER): Message {
  return {
    id,
    peer,
    direction: 'incoming',
    body: `message ${id}`,
    sentAt,
    receivedAt: sentAt,
    status: 'received',
    read: true,
  };
}

beforeEach(() => {
  setActivePinia(createPinia());
});

describe('the chat store', () => {
  it('starts empty and loads nothing until a conversation is opened', () => {
    const chat = useChatStore();
    expect(chat.messages).toEqual([]);
    expect(chat.peerId).toBeNull();
  });

  it('appends messages that arrive in order', async () => {
    const chat = useChatStore();
    await chat.open(PEER);

    chat.add(message('a', 1_000));
    chat.add(message('b', 2_000));
    chat.add(message('c', 3_000));

    expect(chat.messages.map((entry) => entry.id)).toEqual(['a', 'b', 'c']);
  });

  it('inserts a message that arrives out of order in the right place', async () => {
    // The regression: a late message — a retransmission, or one that was queued while the
    // connection was being re-established — must land where its timestamp says, not on top.
    const chat = useChatStore();
    await chat.open(PEER);

    chat.add(message('a', 1_000));
    chat.add(message('c', 3_000));
    chat.add(message('b', 2_000));

    expect(chat.messages.map((entry) => entry.id)).toEqual(['a', 'b', 'c']);
  });

  it('breaks a timestamp tie by identifier, as the storage index does', async () => {
    const chat = useChatStore();
    await chat.open(PEER);

    chat.add(message('b', 5_000));
    chat.add(message('a', 5_000));
    chat.add(message('c', 5_000));

    expect(chat.messages.map((entry) => entry.id)).toEqual(['a', 'b', 'c']);
  });

  it('ignores a message for another conversation', async () => {
    const chat = useChatStore();
    await chat.open(PEER);

    chat.add(message('a', 1_000, OTHER));
    chat.add(message('b', 2_000));

    expect(chat.messages.map((entry) => entry.id)).toEqual(['b']);
  });

  it('treats a repeated identifier as a status update, not a second row', async () => {
    const chat = useChatStore();
    await chat.open(PEER);

    chat.add(message('a', 1_000));
    chat.add({ ...message('a', 1_000), status: 'delivered' });

    expect(chat.messages).toHaveLength(1);
    expect(chat.messages[0]?.status).toBe('delivered');
  });

  it('applies a delivery status by identifier', async () => {
    const chat = useChatStore();
    await chat.open(PEER);
    chat.add(message('a', 1_000));

    chat.update('a', { status: 'delivered' });
    expect(chat.messages[0]?.status).toBe('delivered');

    chat.update('missing', { status: 'failed' });
    expect(chat.messages[0]?.status).toBe('delivered');
  });

  it('reports the oldest loaded message as the page cursor', async () => {
    const chat = useChatStore();
    await chat.open(PEER);
    expect(chat.oldestCursor).toBeNull();

    chat.add(message('b', 2_000));
    chat.add(message('a', 1_000));

    expect(chat.oldestCursor).toEqual({ sentAtMs: 1_000, id: 'a' });
  });

  it('counts only messages that can still fail', async () => {
    const chat = useChatStore();
    await chat.open(PEER);

    chat.add({ ...message('a', 1_000), status: 'sending' });
    chat.add({ ...message('b', 2_000), status: 'sent' });
    chat.add({ ...message('c', 3_000), status: 'sending' });

    expect(chat.pendingCount).toBe(2);
  });

  it('clears only the conversation it is asked to clear', async () => {
    const chat = useChatStore();
    await chat.open(PEER);
    chat.add(message('a', 1_000));

    chat.clear(OTHER);
    expect(chat.messages).toHaveLength(1);

    chat.clear(PEER);
    expect(chat.messages).toHaveLength(0);
  });

  it('drops the loaded page when the conversation changes', async () => {
    const chat = useChatStore();
    await chat.open(PEER);
    chat.add(message('a', 1_000));

    await chat.open(OTHER);
    expect(chat.messages).toHaveLength(0);
    expect(chat.peerId).toBe(OTHER);
  });
});
