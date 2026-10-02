/**
 * Builders for the DTOs the host sends. A test states only the fields its subject reads; the
 * rest are valid, deterministic defaults, so a change to an unrelated field cannot break it.
 */

import type {
  Attachment,
  Bootstrap,
  Diagnostics,
  FilePick,
  KnownDevice,
  LogsInfo,
  Message,
  Peer,
  Profile,
  Settings,
} from '@/ipc';

/** A fixed instant, so no test depends on when it ran. */
export const FIXED_NOW = Date.UTC(2026, 2, 15, 12, 0, 0);

export function makeProfile(overrides: Partial<Profile> = {}): Profile {
  return { deviceId: 'device-self', nickname: 'Me', avatarSeed: 'seed-self', ...overrides };
}

export function makePeer(overrides: Partial<Peer> = {}): Peer {
  return {
    deviceId: 'device-a',
    nickname: 'Alice',
    avatarSeed: 'seed-a',
    online: true,
    lastSeenMs: FIXED_NOW,
    unread: 0,
    notifyMuted: false,
    lastActivityMs: FIXED_NOW,
    lastMessage: null,
    ...overrides,
  };
}

export function makeAttachment(overrides: Partial<Attachment> = {}): Attachment {
  return {
    id: 'attachment-1',
    messageId: 'message-1',
    peer: 'device-a',
    direction: 'outgoing',
    name: 'notes.txt',
    size: 1024,
    kind: 'file',
    state: 'complete',
    transferred: 1024,
    createdAt: FIXED_NOW,
    path: '/tmp/notes.txt',
    ...overrides,
  };
}

export function makeMessage(overrides: Partial<Message> = {}): Message {
  return {
    id: 'message-1',
    peer: 'device-a',
    direction: 'outgoing',
    body: 'hello',
    attachments: [],
    sentAt: FIXED_NOW,
    receivedAt: FIXED_NOW,
    deliveredAt: FIXED_NOW,
    status: 'delivered',
    read: true,
    ...overrides,
  };
}

export function makeKnownDevice(overrides: Partial<KnownDevice> = {}): KnownDevice {
  return {
    deviceId: 'device-a',
    nickname: 'Alice',
    avatarSeed: 'seed-a',
    forgotten: false,
    firstSeenMs: FIXED_NOW,
    lastSeenMs: FIXED_NOW,
    messageCount: 0,
    ...overrides,
  };
}

export function makeFilePick(overrides: Partial<FilePick> = {}): FilePick {
  return {
    path: '/tmp/notes.txt',
    name: 'notes.txt',
    size: 1024,
    kind: 'file',
    problem: null,
    ...overrides,
  };
}

export function makeSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    version: 3,
    onboarded: true,
    appearance: { theme: 'system', accent: '#6750A4' },
    locale: 'en',
    notifications: { enabled: true, showText: true, sound: false },
    system: { autostart: false, startMinimized: false, closeToTray: true },
    logging: { level: 'info', retentionDays: 14 },
    ...overrides,
  };
}

export function makeBootstrap(overrides: Partial<Bootstrap> = {}): Bootstrap {
  return {
    profile: makeProfile(),
    settings: makeSettings(),
    peers: [],
    port: 47820,
    storageRecovered: null,
    discoveryProblem: null,
    version: '0.9.0',
    ...overrides,
  };
}

export function makeDiagnostics(overrides: Partial<Diagnostics> = {}): Diagnostics {
  return {
    version: '0.9.0',
    protocolVersion: 1,
    tcpPort: 47820,
    deviceId: 'device-self',
    platform: 'windows',
    ...overrides,
  };
}

export function makeLogsInfo(overrides: Partial<LogsInfo> = {}): LogsInfo {
  return {
    directory: '/tmp/logs',
    files: [{ name: 'localme.2026-03-15.log', sizeBytes: 2048, modifiedMs: FIXED_NOW }],
    totalBytes: 2048,
    retentionDays: 14,
    ...overrides,
  };
}
