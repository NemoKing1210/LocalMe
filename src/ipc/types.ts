export type DeviceId = string;

export type MessageId = string;

export type AttachmentId = string;

export type UnixMillis = number;

export type MessageDirection = 'incoming' | 'outgoing';

export type MessageStatus = 'queued' | 'sending' | 'delivered' | 'received';

/** An image the interface can show instead of a file chip. */
export type AttachmentKind = 'image' | 'file';

export type AttachmentState =
  'queued' | 'sending' | 'receiving' | 'complete' | 'cancelled' | 'failed';

export interface Attachment {
  readonly id: AttachmentId;
  readonly messageId: MessageId;
  readonly peer: DeviceId;
  readonly direction: MessageDirection;
  readonly name: string;
  readonly size: number;
  readonly kind: AttachmentKind;
  readonly state: AttachmentState;
  /** Bytes that have arrived, on the side that owns this row. */
  readonly transferred: number;
  readonly createdAt: UnixMillis;
  /**
   * Where the file is on this machine. `null` while an incoming file is still being received
   * and after a failure; the sender's source otherwise.
   */
  readonly path: string | null;
  /** The whole-file digest, when one has been computed. */
  readonly sha256?: string;
}

/** A file the host has looked at, on its way into the composer. */
export interface FilePick {
  readonly path: string;
  /** The name the recipient will see: sanitised by the same code that will send it. */
  readonly name: string;
  readonly size: number;
  readonly kind: AttachmentKind;
  /** `null` when the file can be attached. */
  readonly problem: 'missing' | 'directory' | 'tooLarge' | null;
}

export interface Profile {
  readonly deviceId: DeviceId;
  readonly nickname: string;
  readonly avatarSeed: string;
}

export interface MessagePreview {
  readonly direction: MessageDirection;
  readonly body: string;
}

export interface Peer {
  readonly deviceId: DeviceId;
  readonly nickname: string;
  readonly avatarSeed: string;
  readonly online: boolean;
  readonly lastSeenMs: UnixMillis | null;
  readonly unread: number;
  readonly notifyMuted: boolean;
  readonly lastActivityMs: UnixMillis | null;
  readonly lastMessage: MessagePreview | null;
}

export interface Message {
  readonly id: MessageId;
  readonly peer: DeviceId;
  readonly direction: MessageDirection;
  /** `null` for a message that carries only files. */
  readonly body: string | null;
  /** Files attached to this message, in the order the sender listed them. */
  readonly attachments: readonly Attachment[];
  readonly sentAt: UnixMillis;
  readonly receivedAt: UnixMillis;
  /**
   * When the recipient acknowledged an outgoing message, on this machine's clock; `null` while
   * it is queued or in flight, and always `null` for an incoming message. It is the second date
   * a message that waited in the outbox shows.
   */
  readonly deliveredAt: UnixMillis | null;
  readonly status: MessageStatus;
  readonly read: boolean;
}

export interface KnownDevice {
  readonly deviceId: DeviceId;
  readonly nickname: string;
  readonly avatarSeed: string;
  readonly forgotten: boolean;
  readonly firstSeenMs: UnixMillis;
  readonly lastSeenMs: UnixMillis | null;
  readonly messageCount: number;
}

export type ThemeMode = 'system' | 'light' | 'dark';

/** Interface language. Mirrors the `Locale` enum in `localme-core`. */
export type Locale = 'en' | 'ru' | 'es' | 'de' | 'fr' | 'pt' | 'zh';

export interface AppearanceSettings {
  theme: ThemeMode;
  accent: string;
}

export interface NotificationSettings {
  enabled: boolean;
  showText: boolean;
  sound: boolean;
}

export interface SystemSettings {
  autostart: boolean;
  startMinimized: boolean;
  closeToTray: boolean;
}

export type LogLevel = 'error' | 'warn' | 'info' | 'debug';

export interface LoggingSettings {
  level: LogLevel;
  retentionDays: number;
}

export interface Settings {
  version: number;
  onboarded: boolean;
  appearance: AppearanceSettings;
  locale: Locale;
  notifications: NotificationSettings;
  system: SystemSettings;
  logging: LoggingSettings;
}

export interface PageCursor {
  readonly sentAtMs: UnixMillis;
  readonly id: MessageId;
}

export interface Bootstrap {
  readonly profile: Profile;
  readonly settings: Settings;
  readonly peers: readonly Peer[];
  readonly port: number;
  readonly storageRecovered: string | null;
  readonly discoveryProblem: string | null;
  readonly version: string;
}

export interface Diagnostics {
  readonly version: string;
  readonly protocolVersion: number;
  readonly tcpPort: number;
  readonly deviceId: string;
  readonly platform: string;
}

export interface LogFile {
  readonly name: string;
  readonly sizeBytes: number;
  readonly modifiedMs: number | null;
}

export interface LogsInfo {
  readonly directory: string;
  readonly files: readonly LogFile[];
  readonly totalBytes: number;
  readonly retentionDays: number;
}

export interface UiLabels {
  appName: string;
  open: string;
  quit: string;
  tooltipIdle: string;
  tooltipUnread: string;
  newMessage: string;
  conversations: string;
  allConversations: string;
  markAllRead: string;
  notifications: string;
  closeToTray: string;
  autostart: string;
  settings: string;
  openLogs: string;
}

export type ApiError =
  | { readonly kind: 'invalid_input'; readonly field: string; readonly message: string }
  | { readonly kind: 'unknown_peer'; readonly deviceId: string }
  | { readonly kind: 'storage'; readonly message: string }
  | { readonly kind: 'discovery'; readonly message: string }
  | { readonly kind: 'network'; readonly message: string }
  | { readonly kind: 'shutting_down' }
  | { readonly kind: 'internal'; readonly message: string };

export type NoticeLevel = 'info' | 'warning' | 'error';

/**
 * Events the host pushes to the interface.
 *
 * The payload types are the core's own types, serialised — not a second projection of them.
 */
export interface CoreEventMap {
  peers: { readonly peers: readonly Peer[] };
  message: { readonly peer: Peer; readonly message: Message };
  message_status: {
    readonly peer: DeviceId;
    readonly id: MessageId;
    readonly status: MessageStatus;
    readonly deliveredAt: UnixMillis | null;
  };
  attachment: { readonly peer: DeviceId; readonly attachment: Attachment };
  own_profile: { readonly nickname: string; readonly avatarSeed: string };
  notice: { readonly level: NoticeLevel; readonly message: string };
  stopped: null;
  settings_changed: Settings;
  state_snapshot: { readonly peers: readonly Peer[]; readonly settings: Settings };
  open_chat: DeviceId;
  open_settings: null;
  onboarding_complete: null;
}

export type CoreEventName = keyof CoreEventMap;
