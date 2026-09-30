/**
 * Types mirrored from the Rust side.
 *
 * These are hand-written rather than generated. `tauri-specta` was the alternative and was
 * rejected because it is at `2.0.0-rc`: a release candidate in the build to generate a few
 * dozen type pairs is a supply-chain and churn risk for a saving of one file. The property
 * that actually matters — a renamed Rust field breaks the front-end build — is kept by
 * declaring the shapes here exactly as serde emits them (`camelCase`, see the `rename_all`
 * attributes in `localme-core`) and by keeping every consumer type-checked against them.
 */

/** A device identifier: a UUID string. */
export type DeviceId = string;

/** A message identifier: a UUID v7 string. */
export type MessageId = string;

/** Milliseconds since the Unix epoch, as observed by this machine. */
export type UnixMillis = number;

/** Which way a message travelled. */
export type MessageDirection = 'incoming' | 'outgoing';

/** Delivery progress of a message. */
export type MessageStatus = 'sending' | 'sent' | 'delivered' | 'received' | 'failed';

/** A device's public identity. */
export interface Profile {
  readonly deviceId: DeviceId;
  readonly nickname: string;
  readonly avatarSeed: string;
}

/** One row of the user list, already ordered as the interface should show it. */
export interface Peer {
  readonly deviceId: DeviceId;
  readonly nickname: string;
  readonly avatarSeed: string;
  readonly online: boolean;
  readonly lastSeenMs: UnixMillis | null;
  readonly unread: number;
  readonly notifyMuted: boolean;
  readonly lastActivityMs: UnixMillis | null;
}

/** A stored chat message. */
export interface Message {
  readonly id: MessageId;
  readonly peer: DeviceId;
  readonly direction: MessageDirection;
  readonly body: string;
  readonly sentAt: UnixMillis;
  readonly receivedAt: UnixMillis;
  readonly status: MessageStatus;
  readonly read: boolean;
}

/** A device this installation has seen, for the settings screen. */
export interface KnownDevice {
  readonly deviceId: DeviceId;
  readonly nickname: string;
  readonly avatarSeed: string;
  readonly forgotten: boolean;
  readonly firstSeenMs: UnixMillis;
  readonly lastSeenMs: UnixMillis | null;
  readonly messageCount: number;
}

/** Which palette to use. */
export type ThemeMode = 'system' | 'light' | 'dark';

/** Interface language. */
export type Locale = 'en' | 'ru';

/** Appearance settings. */
export interface AppearanceSettings {
  theme: ThemeMode;
  accent: string;
}

/** Notification settings. */
export interface NotificationSettings {
  enabled: boolean;
  showText: boolean;
  sound: boolean;
}

/** Operating-system integration settings. */
export interface SystemSettings {
  autostart: boolean;
  startMinimized: boolean;
  closeToTray: boolean;
}

/** The settings document, as stored by the host. */
export interface Settings {
  version: number;
  onboarded: boolean;
  appearance: AppearanceSettings;
  locale: Locale;
  notifications: NotificationSettings;
  system: SystemSettings;
}

/** Where a page of history starts. */
export interface PageCursor {
  readonly sentAtMs: UnixMillis;
  readonly id: MessageId;
}

/** Everything the interface needs for its first paint. */
export interface Bootstrap {
  readonly profile: Profile;
  readonly settings: Settings;
  readonly peers: readonly Peer[];
  readonly port: number;
  readonly storageRecovered: string | null;
  readonly discoveryProblem: string | null;
  readonly version: string;
}

/** Ports, identity and versions, for the About section. */
export interface Diagnostics {
  readonly version: string;
  readonly protocolVersion: number;
  readonly tcpPort: number;
  readonly deviceId: string;
  readonly platform: string;
}

/** Labels the host draws outside the web view. */
export interface UiLabels {
  appName: string;
  open: string;
  mute: string;
  unmute: string;
  quit: string;
  tooltipIdle: string;
  tooltipUnread: string;
  newMessage: string;
}

/**
 * A failure returned by a command.
 *
 * A tagged enum rather than a message string, so the interface can say something useful in the
 * user's language per case instead of printing English from the Rust layer.
 */
export type ApiError =
  | { readonly kind: 'invalid_input'; readonly field: string; readonly message: string }
  | { readonly kind: 'unknown_peer'; readonly deviceId: string }
  | { readonly kind: 'peer_offline'; readonly deviceId: string }
  | { readonly kind: 'storage'; readonly message: string }
  | { readonly kind: 'discovery'; readonly message: string }
  | { readonly kind: 'network'; readonly message: string }
  | { readonly kind: 'shutting_down' }
  | { readonly kind: 'internal'; readonly message: string };

/** Severity of a notice from the core. */
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
  };
  own_profile: { readonly nickname: string; readonly avatarSeed: string };
  notice: { readonly level: NoticeLevel; readonly message: string };
  stopped: null;
  settings_changed: Settings;
  state_snapshot: { readonly peers: readonly Peer[]; readonly settings: Settings };
  open_chat: DeviceId;
  onboarding_complete: null;
}

/** The name of a core event. */
export type CoreEventName = keyof CoreEventMap;
