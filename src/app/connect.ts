/**
 * The bridge between the host's event stream and the stores.
 *
 * One subscription per event, one handler each, all of them in this file. That is the whole
 * point: a component never subscribes to the host directly, so what the interface does in
 * response to a message arriving is readable in one place instead of being spread across the
 * components that happened to need it.
 */
import { onCoreEvent } from '@/ipc';
import type { Peer } from '@/ipc';
import { useChatStore } from '@/stores/chat';
import { usePeerStore } from '@/stores/peers';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';

/**
 * Starts forwarding host events into the stores.
 *
 * Returns the function that stops it, which the shell calls when it unmounts — the only time
 * the interface and the host are not both alive.
 */
export async function connectCoreEvents(): Promise<() => void> {
  const peers = usePeerStore();
  const chat = useChatStore();
  const settings = useSettingsStore();
  const ui = useUiStore();

  /** Replaces one row of the list without disturbing the others. */
  function replacePeer(updated: Peer): void {
    const next = peers.peers.map((peer) =>
      peer.deviceId === updated.deviceId ? updated : peer,
    );
    peers.replace(next);
  }

  const unlisten = await Promise.all([
    onCoreEvent('peers', (payload) => {
      peers.replace(payload.peers);
    }),

    onCoreEvent('message', (payload) => {
      // The host sends the peer's row alongside the message, because it has just changed:
      // unread went up, or the activity timestamp moved the row up the list.
      replacePeer(payload.peer);
      chat.add(payload.message);
    }),

    onCoreEvent('message_status', (payload) => {
      chat.update(payload.id, { status: payload.status });
    }),

    onCoreEvent('settings_changed', (payload) => {
      settings.apply(payload);
    }),

    // The resynchronisation after the window comes back from the tray. Whole documents, not
    // deltas: the interface replaces what it has rather than trying to reconcile.
    onCoreEvent('state_snapshot', (payload) => {
      peers.replace(payload.peers);
      settings.apply(payload.settings);
    }),

    onCoreEvent('open_chat', (deviceId) => {
      ui.goTo('chat');
      peers.select(deviceId);
    }),

    onCoreEvent('notice', (payload) => {
      // The core's notices are written for the log, in English. The interface shows the one
      // actionable sentence it has for a failure and leaves the detail where it belongs.
      if (payload.level === 'error') ui.fail('error.internal');
    }),

    onCoreEvent('stopped', () => {
      ui.fail('error.shuttingDown');
    }),
  ]);

  return () => {
    for (const stop of unlisten) stop();
  };
}
