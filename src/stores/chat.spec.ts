import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Attachment, Message } from '@/ipc';

vi.mock('@/ipc', () => ({
  history: vi.fn().mockResolvedValue([]),
  sendMessage: vi.fn(),
}));

import * as ipc from '@/ipc';

import { useChatStore } from './chat';

const PEER = '018f2b9c-0000-7000-8000-0000000000aa';
const OTHER = '018f2b9c-0000-7000-8000-0000000000bb';

function attachment(id: string, state: Attachment['state'], transferred: number): Attachment {
  return {
    id,
    messageId: 'm',
    peer: PEER,
    direction: 'incoming',
    name: `${id}.bin`,
    size: 1_024,
    kind: 'file',
    state,
    transferred,
    createdAt: 1,
    path: null,
  };
}

function message(id: string, sentAt: number, peer = PEER): Message {
  return {
    id,
    peer,
    direction: 'incoming',
    body: `message ${id}`,
    attachments: [],
    sentAt,
    receivedAt: sentAt,
    deliveredAt: null,
    status: 'received',
    read: true,
  };
}

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((innerResolve) => {
    resolve = innerResolve;
  });
  return { promise, resolve };
}

beforeEach(() => {
  setActivePinia(createPinia());
  vi.mocked(ipc.history).mockReset();
  vi.mocked(ipc.history).mockResolvedValue([]);
  vi.mocked(ipc.sendMessage).mockReset();
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

    chat.update('missing', { status: 'queued' });
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

  it('counts everything that has not been delivered yet', async () => {
    const chat = useChatStore();
    await chat.open(PEER);

    chat.add({ ...message('a', 1_000), status: 'queued' });
    chat.add({ ...message('b', 2_000), status: 'sending' });
    chat.add({ ...message('c', 3_000), status: 'queued' });
    chat.add({ ...message('d', 4_000), status: 'delivered' });

    expect(chat.pendingCount).toBe(3);
  });

  it('carries the delivery time of a message that waited in the outbox', async () => {
    const chat = useChatStore();
    await chat.open(PEER);

    chat.add({ ...message('a', 1_000), direction: 'outgoing', status: 'queued' });
    expect(chat.messages[0]?.deliveredAt).toBeNull();

    // What the host sends when the acknowledgement arrives.
    chat.update('a', { status: 'delivered', deliveredAt: 90_000 });
    expect(chat.messages[0]?.status).toBe('delivered');
    expect(chat.messages[0]?.deliveredAt).toBe(90_000);
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

  it('replaces one attachment inside its message and leaves the rest alone', async () => {
    const chat = useChatStore();
    await chat.open(PEER);

    const file = attachment('f1', 'receiving', 0);
    const other = attachment('f2', 'receiving', 0);
    const withFiles = { ...message('a', 1_000), attachments: [file, other] };
    chat.add(withFiles);
    chat.add(message('b', 2_000));

    // What the host sends on every acknowledgement: the whole row, further along.
    chat.applyAttachment({ ...file, transferred: 512, state: 'complete', path: '/tmp/a.bin' });

    expect(chat.messages[0]?.attachments[0]?.state).toBe('complete');
    expect(chat.messages[0]?.attachments[0]?.transferred).toBe(512);
    expect(chat.messages[0]?.attachments[1]?.state).toBe('receiving');

    // A row for a message that is not loaded — a page away — must change nothing.
    chat.applyAttachment(attachment('f9', 'complete', 4));
    expect(chat.messages[0]?.attachments).toHaveLength(2);
    expect(chat.messages[1]?.attachments).toHaveLength(0);
  });

  it('drops the loaded page when the conversation changes', async () => {
    const chat = useChatStore();
    await chat.open(PEER);
    chat.add(message('a', 1_000));

    await chat.open(OTHER);
    expect(chat.messages).toHaveLength(0);
    expect(chat.peerId).toBe(OTHER);
  });

  it('clears the conversation when opened with no peer', async () => {
    const chat = useChatStore();
    await chat.open(PEER);
    chat.add(message('a', 1_000));

    await chat.open(null);

    expect(chat.peerId).toBeNull();
    expect(chat.messages).toHaveLength(0);
    // No conversation means nothing to fetch.
    expect(ipc.history).toHaveBeenCalledTimes(1);
  });

  it('loads the first page with the module page size and tracks a full page', async () => {
    const page = Array.from({ length: 50 }, (_, index) => message(`m${index}`, index + 1));
    vi.mocked(ipc.history).mockResolvedValue(page);
    const chat = useChatStore();

    await chat.open(PEER);

    expect(ipc.history).toHaveBeenCalledWith(PEER, null, 50);
    expect(chat.messages).toHaveLength(50);
    // A full page is the signal that the host may hold older messages.
    expect(chat.hasMore).toBe(true);
  });

  it('does not offer more pages when the first page is short', async () => {
    vi.mocked(ipc.history).mockResolvedValue([message('a', 1_000)]);
    const chat = useChatStore();

    await chat.open(PEER);

    expect(chat.hasMore).toBe(false);
  });

  it('ignores a first page that arrives after the conversation changed', async () => {
    const request = deferred<Message[]>();
    vi.mocked(ipc.history).mockReturnValueOnce(request.promise);
    vi.mocked(ipc.history).mockResolvedValueOnce([message('o', 1_000, OTHER)]);
    const chat = useChatStore();

    const pending = chat.open(PEER);
    await chat.open(OTHER);

    request.resolve([message('stale', 1_000, PEER)]);
    await pending;

    expect(chat.peerId).toBe(OTHER);
    expect(chat.messages.map((entry) => entry.id)).toEqual(['o']);
  });

  it('does not ask for older messages before a page is loaded', async () => {
    const chat = useChatStore();
    await chat.open(PEER);
    expect(ipc.history).toHaveBeenCalledTimes(1);

    await chat.loadOlder();

    expect(ipc.history).toHaveBeenCalledTimes(1);
  });

  it('requests the page before the oldest message and prepends it', async () => {
    vi.mocked(ipc.history).mockResolvedValueOnce([message('c', 3_000)]);
    const chat = useChatStore();
    await chat.open(PEER);

    vi.mocked(ipc.history).mockResolvedValueOnce([message('a', 1_000), message('b', 2_000)]);
    await chat.loadOlder();

    expect(ipc.history).toHaveBeenLastCalledWith(PEER, { sentAtMs: 3_000, id: 'c' }, 50);
    expect(chat.messages.map((entry) => entry.id)).toEqual(['a', 'b', 'c']);
    expect(chat.hasMore).toBe(false);
  });

  it('keeps offering older pages while the host returns a full one', async () => {
    vi.mocked(ipc.history).mockResolvedValueOnce([message('z', 100_000)]);
    const chat = useChatStore();
    await chat.open(PEER);

    const older = Array.from({ length: 50 }, (_, index) => message(`o${index}`, index + 1));
    vi.mocked(ipc.history).mockResolvedValueOnce(older);
    await chat.loadOlder();

    expect(chat.hasMore).toBe(true);
  });

  it('stores a sent message with its files and adds the returned row', async () => {
    const stored: Message = {
      ...message('s', 5_000),
      direction: 'outgoing',
      body: 'hi',
      status: 'queued',
      deliveredAt: null,
    };
    vi.mocked(ipc.sendMessage).mockResolvedValue(stored);
    const chat = useChatStore();
    await chat.open(PEER);

    await chat.send('hi', ['/tmp/a.txt']);

    expect(ipc.sendMessage).toHaveBeenCalledWith(PEER, 'hi', ['/tmp/a.txt']);
    expect(chat.messages.map((entry) => entry.id)).toEqual(['s']);
  });

  it('does not add a second row when the host also delivers the sent message', async () => {
    const stored: Message = {
      ...message('s', 5_000),
      direction: 'outgoing',
      body: 'hi',
      status: 'queued',
      deliveredAt: null,
    };
    vi.mocked(ipc.sendMessage).mockResolvedValue(stored);
    const chat = useChatStore();
    await chat.open(PEER);
    await chat.send('hi');

    // The host fans the same stored row out as a `message` event; `add` merges by identifier.
    chat.add({ ...stored, status: 'delivered', deliveredAt: 6_000 });

    expect(chat.messages).toHaveLength(1);
    expect(chat.messages[0]?.status).toBe('delivered');
  });

  it('does not send with no conversation open or while a send is in flight', async () => {
    const chat = useChatStore();
    await chat.send('nobody');
    expect(ipc.sendMessage).not.toHaveBeenCalled();

    await chat.open(PEER);
    const request = deferred<Message>();
    vi.mocked(ipc.sendMessage).mockReturnValue(request.promise);

    const first = chat.send('one');
    await chat.send('two');
    expect(ipc.sendMessage).toHaveBeenCalledTimes(1);

    request.resolve(message('s', 5_000));
    await first;
  });
});
