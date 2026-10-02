import { onCoreEvent } from '@/ipc';
import type { Peer } from '@/ipc';
import { router } from '@/app/router';
import { ROUTE } from '@/app/routes';
import { useChatStore } from '@/stores/chat';
import { usePeerStore } from '@/stores/peers';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';

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
      void router.replace({ name: ROUTE.chat, params: { deviceId } });
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
