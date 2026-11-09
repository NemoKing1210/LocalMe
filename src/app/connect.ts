import { onCoreEvent } from '@/ipc';
import type { CoreEventName, Peer } from '@/ipc';
import { router } from '@/app/router';
import { ROUTE } from '@/app/routes';
import { useChatStore } from '@/stores/chat';
import { usePeerStore } from '@/stores/peers';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';

/**
 * Every host event, so one added to `CoreEventMap` is a compile error here until it is either
 * handled below or consciously listed as ignored. Exported for the spec that checks the list and
 * the subscriptions agree.
 */
export const HANDLED_EVENTS = {
  peers: true,
  message: true,
  message_status: true,
  attachment: true,
  attachment_saved: true,
  settings_changed: true,
  state_snapshot: true,
  open_chat: true,
  open_settings: true,
  notice: true,
  stopped: true,
} as const satisfies Record<CoreEventName, true>;

export async function connectCoreEvents(): Promise<() => void> {
  const peers = usePeerStore();
  const chat = useChatStore();
  const settings = useSettingsStore();
  const ui = useUiStore();

  function replacePeer(updated: Peer): void {
    const next = peers.peers.map((peer) => (peer.deviceId === updated.deviceId ? updated : peer));
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
      chat.update(payload.id, { status: payload.status, deliveredAt: payload.deliveredAt });
    }),

    // A transfer moved: progress, completion, a failure. The whole row arrives, so applying it
    // is one replacement rather than a reconciliation.
    onCoreEvent('attachment', (payload) => {
      chat.applyAttachment(payload.attachment);
    }),

    // The host reports the copy itself, because the command returns as soon as the transfer
    // starts — so this is the only place a save is announced, and it is announced once.
    onCoreEvent('attachment_saved', (payload) => {
      if (payload.path === null) {
        ui.fail('chat.attachmentSaveFailed');
      } else {
        ui.notify('chat.attachmentSaved', { path: payload.path });
      }
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
      void router.replace({ name: ROUTE.chat, params: { deviceId } });
    }),

    onCoreEvent('open_settings', () => {
      void router.replace({ name: ROUTE.settings });
    }),

    onCoreEvent('notice', (payload) => {
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
