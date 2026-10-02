// @vitest-environment happy-dom

import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it, vi, type Mock } from 'vitest';

const capture = vi.hoisted(() => {
  const handlers = new Map<string, (payload: unknown) => void>();
  const stops: Array<() => void> = [];
  return { handlers, stops };
});

vi.mock('@/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/ipc')>()),
  onCoreEvent: vi.fn(),
}));

import { onCoreEvent } from '@/ipc';
import { router } from '@/app/router';
import { ROUTE } from '@/app/routes';
import { useChatStore } from '@/stores/chat';
import { usePeerStore } from '@/stores/peers';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';
import { FIXED_NOW, makeAttachment, makeMessage, makePeer, makeSettings } from '@/test/factories';

import { connectCoreEvents } from './connect';

/** `onCoreEvent` is generic; a concrete signature keeps the mock's calls readable in a spec. */
type Register = (name: string, handler: (payload: unknown) => void) => Promise<() => void>;
const onCoreEventMock = onCoreEvent as unknown as Mock<Register>;

/** Every event `connectCoreEvents` subscribes to, in registration order. */
const EVENT_NAMES = [
  'peers',
  'message',
  'message_status',
  'attachment',
  'settings_changed',
  'state_snapshot',
  'open_chat',
  'open_settings',
  'notice',
  'stopped',
] as const;

function handler(name: (typeof EVENT_NAMES)[number]): (payload: unknown) => void {
  const registered = capture.handlers.get(name);
  if (!registered) throw new Error(`no handler registered for ${name}`);
  return registered;
}

beforeEach(async () => {
  setActivePinia(createPinia());
  capture.handlers.clear();
  capture.stops.length = 0;

  onCoreEventMock.mockImplementation((name, callback) => {
    capture.handlers.set(name, callback);
    const stop = vi.fn();
    capture.stops.push(stop);
    return Promise.resolve(stop);
  });

  await connectCoreEvents();
});

describe('connectCoreEvents', () => {
  it('listens once per host event', () => {
    expect([...capture.handlers.keys()].sort()).toEqual([...EVENT_NAMES].sort());
    expect(onCoreEventMock).toHaveBeenCalledTimes(EVENT_NAMES.length);
  });

  it('replaces the whole peer list when the host sends one', () => {
    const peers = usePeerStore();
    peers.replace([makePeer({ deviceId: 'device-old' })]);

    handler('peers')({
      peers: [makePeer({ deviceId: 'device-a', unread: 2 }), makePeer({ deviceId: 'device-b' })],
    });

    expect(peers.peers.map((peer) => peer.deviceId)).toEqual(['device-a', 'device-b']);
    expect(peers.get('device-a')?.unread).toBe(2);
  });

  it('replaces the peer row a message arrived for, and adds the message', () => {
    const peers = usePeerStore();
    const chat = useChatStore();
    peers.replace([makePeer({ deviceId: 'device-a', unread: 0 })]);
    chat.peerId = 'device-a';

    handler('message')({
      peer: makePeer({ deviceId: 'device-a', unread: 1, lastActivityMs: FIXED_NOW }),
      message: makeMessage({ id: 'new-message', peer: 'device-a' }),
    });

    expect(peers.get('device-a')?.unread).toBe(1);
    expect(peers.get('device-a')?.lastActivityMs).toBe(FIXED_NOW);
    expect(chat.messages.map((message) => message.id)).toEqual(['new-message']);
  });

  it('does not invent a row for a peer the list never had', () => {
    const peers = usePeerStore();
    const chat = useChatStore();
    peers.replace([makePeer({ deviceId: 'device-a' })]);
    chat.peerId = 'device-new';

    handler('message')({
      peer: makePeer({ deviceId: 'device-new' }),
      message: makeMessage({ id: 'from-new', peer: 'device-new' }),
    });

    // Observed: `replacePeer` only substitutes rows that already exist, so a brand-new peer is
    // not inserted here — its row is expected to arrive on a `peers` event. The message itself
    // still lands.
    expect(peers.get('device-new')).toBeUndefined();
    expect(chat.messages.map((message) => message.id)).toEqual(['from-new']);
  });

  it('patches the status of a known message', () => {
    const chat = useChatStore();
    chat.peerId = 'device-a';
    chat.add(
      makeMessage({ id: 'pending', peer: 'device-a', status: 'sending', deliveredAt: null }),
    );

    handler('message_status')({
      peer: 'device-a',
      id: 'pending',
      status: 'delivered',
      deliveredAt: FIXED_NOW,
    });

    expect(chat.messages[0]?.status).toBe('delivered');
    expect(chat.messages[0]?.deliveredAt).toBe(FIXED_NOW);
  });

  it('applies a transfer row to the attachment it replaces', () => {
    const chat = useChatStore();
    chat.peerId = 'device-a';
    chat.add(
      makeMessage({
        id: 'with-file',
        peer: 'device-a',
        attachments: [makeAttachment({ id: 'file-1', state: 'sending', transferred: 0 })],
      }),
    );

    handler('attachment')({
      peer: 'device-a',
      attachment: makeAttachment({ id: 'file-1', state: 'complete', transferred: 1024 }),
    });

    expect(chat.messages[0]?.attachments[0]?.state).toBe('complete');
    expect(chat.messages[0]?.attachments[0]?.transferred).toBe(1024);
  });

  it('applies settings that changed', () => {
    const settings = useSettingsStore();
    const next = makeSettings({ appearance: { theme: 'dark', accent: '#123456' }, locale: 'ru' });

    handler('settings_changed')(next);

    expect(settings.document).toEqual(next);
    expect(settings.theme).toBe('dark');
    expect(settings.locale).toBe('ru');
  });

  it('replaces both documents from a state snapshot', () => {
    const peers = usePeerStore();
    const settings = useSettingsStore();

    handler('state_snapshot')({
      peers: [makePeer({ deviceId: 'device-z' })],
      settings: makeSettings({ locale: 'de' }),
    });

    expect(peers.peers.map((peer) => peer.deviceId)).toEqual(['device-z']);
    expect(settings.locale).toBe('de');
  });

  it('navigates to the chat route the tray asked for', () => {
    const replace = vi.spyOn(router, 'replace').mockResolvedValue(undefined);

    handler('open_chat')('device-x');

    expect(replace).toHaveBeenCalledWith({ name: ROUTE.chat, params: { deviceId: 'device-x' } });
  });

  it('navigates to settings', () => {
    const replace = vi.spyOn(router, 'replace').mockResolvedValue(undefined);

    handler('open_settings')(null);

    expect(replace).toHaveBeenCalledWith({ name: ROUTE.settings });
  });

  it('surfaces an error notice and ignores the lower levels', () => {
    const ui = useUiStore();

    handler('notice')({ level: 'error', message: 'gone wrong' });
    expect(ui.notice?.key).toBe('error.internal');
    expect(ui.notice?.tone).toBe('error');

    ui.dismiss();
    handler('notice')({ level: 'warning', message: 'careful' });
    handler('notice')({ level: 'info', message: 'fyi' });
    expect(ui.notice).toBeNull();
  });

  it('reports the host shutting down', () => {
    const ui = useUiStore();

    handler('stopped')(null);

    expect(ui.notice?.key).toBe('error.shuttingDown');
    expect(ui.notice?.tone).toBe('error');
  });

  it('stops every listener exactly once when disposed', async () => {
    capture.stops.length = 0;
    const dispose = await connectCoreEvents();

    dispose();

    expect(capture.stops).toHaveLength(EVENT_NAMES.length);
    for (const stop of capture.stops) {
      expect(stop).toHaveBeenCalledTimes(1);
    }
  });
});
