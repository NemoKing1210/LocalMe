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
  LogLevel,
  LogsInfo,
  Message,
  PageCursor,
  Peer,
  Profile,
  Settings,
  UiLabels,
} from './types';

export type * from './types';

export class CommandError extends Error {
  readonly kind: ApiError['kind'];
  readonly detail: ApiError;

  constructor(detail: ApiError) {
    super(describe(detail));
    this.name = 'CommandError';
    this.kind = detail.kind;
    this.detail = detail;
  }
}

function describe(error: ApiError): string {
  switch (error.kind) {
    case 'invalid_input':
      return `${error.field}: ${error.message}`;
    case 'unknown_peer':
      return `unknown device ${error.deviceId}`;
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
  // The switch covers every variant of `ApiError`; this line exists so the function is total
  // for the linter, and would only be reached if the host sent a tag this build does not know.
  return 'the host reported a failure this build does not understand';
}

function isApiError(value: unknown): value is ApiError {
  return (
    typeof value === 'object' && value !== null && 'kind' in value && typeof value.kind === 'string'
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

export async function onCoreEvent<K extends CoreEventName>(
  name: K,
  handler: (payload: CoreEventMap[K]) => void,
): Promise<UnlistenFn> {
  return listen<CoreEventMap[K]>(name, (event) => {
    handler(event.payload);
  });
}

export function bootstrap(): Promise<Bootstrap> {
  return call('bootstrap');
}

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

export function sendMessage(peerId: DeviceId, body: string): Promise<Message> {
  return call('send_message', { peerId, body });
}

export function markRead(peerId: DeviceId): Promise<number> {
  return call('mark_read', { peerId });
}

export function forgetPeer(peerId: DeviceId, deleteHistory: boolean): Promise<void> {
  return call('forget_peer', { peerId, deleteHistory });
}

export function restorePeer(peerId: DeviceId): Promise<void> {
  return call('restore_peer', { peerId });
}

export function setPeerMuted(peerId: DeviceId, muted: boolean): Promise<void> {
  return call('set_peer_muted', { peerId, muted });
}

export function knownDevices(): Promise<KnownDevice[]> {
  return call('known_devices');
}

export function clearHistory(): Promise<number> {
  return call('clear_history');
}

export function ownProfile(): Promise<Profile> {
  return call('own_profile');
}

export function setNickname(nickname: string): Promise<Profile> {
  return call('set_nickname', { nickname });
}

export function completeOnboarding(nickname: string): Promise<Profile> {
  return call('complete_onboarding', { nickname });
}

export function getSettings(): Promise<Settings> {
  return call('get_settings');
}

export function updateSettings(settings: Settings): Promise<Settings> {
  return call('update_settings', { settings });
}

export function isAutostartEnabled(): Promise<boolean> {
  return call('is_autostart_enabled');
}

export function setUiLabels(labels: UiLabels): Promise<void> {
  return call('set_ui_labels', { labels });
}

/** Paints the native window frame; `onAccent` is the label colour and must contrast with it. */
export function setWindowAccent(accent: string, onAccent: string): Promise<void> {
  return call('set_window_accent', { accent, onAccent });
}

export function setActiveChat(peerId: DeviceId | null): Promise<void> {
  return call('set_active_chat', { peerId });
}

export function showWindow(): Promise<void> {
  return call('show_window');
}

export function hideWindow(): Promise<void> {
  return call('hide_window');
}

export function quit(): Promise<void> {
  return call('quit');
}

export function diagnostics(): Promise<Diagnostics> {
  return call('diagnostics');
}

export function logsInfo(): Promise<LogsInfo> {
  return call('logs_info');
}

export function openLogsFolder(): Promise<void> {
  return call('open_logs_folder');
}

export function clearLogs(): Promise<number> {
  return call('clear_logs');
}

/**
 * Writes a front-end failure to the same daily file as the host's own records.
 *
 * Deliberately not failing loudly: a log we could not deliver must never turn a handled error
 * into an unhandled one, so the caller ignores the rejection.
 */
export function logFrontend(level: LogLevel, message: string, context?: string): Promise<void> {
  return call('log_frontend', { level, message, context: context ?? null });
}
