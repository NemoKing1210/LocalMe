/**
 * The IPC surface: one typed function per command, one typed subscription per event.
 *
 * This module is the only place that knows a command's name as a string. Everything else
 * imports a function, so a rename is a compile error at every call site instead of a runtime
 * rejection, and there is exactly one file to read to see what the application can ask the
 * host to do.
 */
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type {
  ApiError,
  Bootstrap,
  CoreEventMap,
  CoreEventName,
  DeviceId,
  Diagnostics,
  KnownDevice,
  Message,
  PageCursor,
  Peer,
  Profile,
  Settings,
  UiLabels,
} from './types';

export type * from './types';

/**
 * A command rejection, normalised.
 *
 * Tauri delivers the serialised `ApiError` from Rust; anything else — a missing command, a
 * panic in the host — is wrapped so callers only ever deal with one shape.
 */
export class CommandError extends Error {
  readonly kind: ApiError['kind'];
  readonly detail: ApiError;

  constructor(detail: ApiError) {
    super(describe(detail));
    this.name = 'CommandError';
    this.kind = detail.kind;
    this.detail = detail;
  }

  /** Whether this failure means "the peer is not reachable right now". */
  get isOffline(): boolean {
    return this.kind === 'peer_offline';
  }
}

function describe(error: ApiError): string {
  switch (error.kind) {
    case 'invalid_input':
      return `${error.field}: ${error.message}`;
    case 'unknown_peer':
      return `unknown device ${error.deviceId}`;
    case 'peer_offline':
      return `device ${error.deviceId} is offline`;
    case 'storage':
      return `storage: ${error.message}`;
    case 'discovery':
      return `discovery: ${error.message}`;
    case 'network':
      return `network: ${error.message}`;
    case 'shutting_down':
      return 'the application is closing';
    case 'internal':
      return `internal: ${error.message}`;
  }
}

function isApiError(value: unknown): value is ApiError {
  return (
    typeof value === 'object' &&
    value !== null &&
    'kind' in value &&
    typeof value.kind === 'string'
  );
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    if (isApiError(raw)) throw new CommandError(raw);
    throw new CommandError({
      kind: 'internal',
      message: typeof raw === 'string' ? raw : 'the host refused the command',
    });
  }
}

/** Subscribes to a host event. Always returns the unlisten function. */
export async function onCoreEvent<K extends CoreEventName>(
  name: K,
  handler: (payload: CoreEventMap[K]) => void,
): Promise<UnlistenFn> {
  return listen<CoreEventMap[K]>(name, (event) => {
    handler(event.payload);
  });
}

/** Everything the interface needs for its first paint. */
export function bootstrap(): Promise<Bootstrap> {
  return call('bootstrap');
}

/** The arranged user list. */
export function listPeers(): Promise<Peer[]> {
  return call('list_peers');
}

/** One page of a conversation, newest first. */
export function history(
  peerId: DeviceId,
  before: PageCursor | null,
  limit?: number,
): Promise<Message[]> {
  return call('history', { peerId, before, limit });
}

/** Sends a message. */
export function sendMessage(peerId: DeviceId, body: string): Promise<Message> {
  return call('send_message', { peerId, body });
}

/** Marks a conversation as read. Returns how many messages changed. */
export function markRead(peerId: DeviceId): Promise<number> {
  return call('mark_read', { peerId });
}

/** Forgets a device, optionally deleting its conversation. */
export function forgetPeer(peerId: DeviceId, deleteHistory: boolean): Promise<void> {
  return call('forget_peer', { peerId, deleteHistory });
}

/** Lets a forgotten device back into the list. */
export function restorePeer(peerId: DeviceId): Promise<void> {
  return call('restore_peer', { peerId });
}

/** Suppresses or restores notifications for one device. */
export function setPeerMuted(peerId: DeviceId, muted: boolean): Promise<void> {
  return call('set_peer_muted', { peerId, muted });
}

/** Every device this installation has seen, forgotten ones included. */
export function knownDevices(): Promise<KnownDevice[]> {
  return call('known_devices');
}

/** Deletes every stored message. Returns how many were deleted. */
export function clearHistory(): Promise<number> {
  return call('clear_history');
}

/** This device's identity. */
export function ownProfile(): Promise<Profile> {
  return call('own_profile');
}

/** Renames this device and tells every connected peer. */
export function setNickname(nickname: string): Promise<Profile> {
  return call('set_nickname', { nickname });
}

/** Completes the first-run screen: sets the nickname and records that the user was asked. */
export function completeOnboarding(nickname: string): Promise<Profile> {
  return call('complete_onboarding', { nickname });
}

/** The settings document. */
export function getSettings(): Promise<Settings> {
  return call('get_settings');
}

/** Replaces the settings document. */
export function updateSettings(settings: Settings): Promise<Settings> {
  return call('update_settings', { settings });
}

/** Whether the application is registered to start at sign-in. */
export function isAutostartEnabled(): Promise<boolean> {
  return call('is_autostart_enabled');
}

/** Replaces the labels the tray and the notifications are drawn with. */
export function setUiLabels(labels: UiLabels): Promise<void> {
  return call('set_ui_labels', { labels });
}

/** Tells the host which conversation is on screen. */
export function setActiveChat(peerId: DeviceId | null): Promise<void> {
  return call('set_active_chat', { peerId });
}

/** Shows and focuses the main window. */
export function showWindow(): Promise<void> {
  return call('show_window');
}

/** Hides the main window, leaving the application in the tray. */
export function hideWindow(): Promise<void> {
  return call('hide_window');
}

/** Quits the application gracefully. */
export function quit(): Promise<void> {
  return call('quit');
}

/** Ports, identity and versions. */
export function diagnostics(): Promise<Diagnostics> {
  return call('diagnostics');
}
