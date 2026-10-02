import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: vi.fn(),
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}));

import { FIXED_NOW, makePeer, makeSettings } from '@/test/factories';

import {
  CommandError,
  bootstrap,
  cancelAttachment,
  clearHistory,
  clearLogs,
  completeOnboarding,
  diagnostics,
  fileSrc,
  forgetPeer,
  getSettings,
  hideWindow,
  history,
  inspectFiles,
  isAutostartEnabled,
  knownDevices,
  listPeers,
  logFrontend,
  logsInfo,
  markRead,
  onCoreEvent,
  openAttachment,
  openLogsFolder,
  ownProfile,
  pickFiles,
  quit,
  restorePeer,
  retryAttachment,
  revealAttachment,
  saveAttachment,
  sendMessage,
  setActiveChat,
  setNickname,
  setPeerMuted,
  setUiLabels,
  setWindowAccent,
  showWindow,
  updateSettings,
} from './index';
import type { ApiError, CoreEventMap, PageCursor, UiLabels } from './types';

const PEER = 'peer-1';
const OTHER_PEER = 'peer-2';
const ATTACHMENT_ID = 'attachment-1';
const CURSOR: PageCursor = { sentAtMs: FIXED_NOW, id: 'msg-1' };

const settings = makeSettings();
const labels: UiLabels = {
  appName: 'LocalMe',
  open: 'Open',
  quit: 'Quit',
  tooltipIdle: 'Idle',
  tooltipUnread: 'Unread',
  newMessage: 'New message',
  conversations: 'Conversations',
  allConversations: 'All conversations',
  markAllRead: 'Mark all read',
  notifications: 'Notifications',
  closeToTray: 'Close to tray',
  autostart: 'Start at login',
  settings: 'Settings',
  openLogs: 'Open logs',
};

describe('CommandError', () => {
  const descriptionCases: ReadonlyArray<{ title: string; detail: ApiError; message: string }> = [
    {
      title: 'spells out the field of an invalid input',
      detail: { kind: 'invalid_input', field: 'nickname', message: 'must not be empty' },
      message: 'nickname: must not be empty',
    },
    {
      title: 'names the unknown device',
      detail: { kind: 'unknown_peer', deviceId: 'device-42' },
      message: 'unknown device device-42',
    },
    {
      title: 'prefixes a storage failure',
      detail: { kind: 'storage', message: 'disk full' },
      message: 'storage: disk full',
    },
    {
      title: 'prefixes a discovery failure',
      detail: { kind: 'discovery', message: 'no route' },
      message: 'discovery: no route',
    },
    {
      title: 'prefixes a network failure',
      detail: { kind: 'network', message: 'timed out' },
      message: 'network: timed out',
    },
    {
      title: 'describes shutdown without the host message',
      detail: { kind: 'shutting_down' },
      message: 'the application is closing',
    },
    {
      title: 'prefixes an internal failure',
      detail: { kind: 'internal', message: 'worker panicked' },
      message: 'internal: worker panicked',
    },
  ];

  it.each(descriptionCases)('$title', ({ detail, message }) => {
    const error = new CommandError(detail);

    expect(error).toBeInstanceOf(Error);
    expect(error.name).toBe('CommandError');
    expect(error.message).toBe(message);
    expect(error.kind).toBe(detail.kind);
    expect(error.detail).toBe(detail);
  });

  it('falls back to a generic sentence for an error tag this build does not know', () => {
    const detail = { kind: 'quantum_flux' } as unknown as ApiError;

    const error = new CommandError(detail);

    expect(error.message).toBe('the host reported a failure this build does not understand');
    expect(error.name).toBe('CommandError');
    expect(error.kind).toBe('quantum_flux');
    expect(error.detail).toBe(detail);
  });
});

async function rejectionOf(run: () => Promise<unknown>): Promise<CommandError> {
  try {
    await run();
  } catch (error) {
    if (error instanceof CommandError) return error;
    throw error;
  }
  throw new Error('the call resolved but a rejection was expected');
}

/** The object argument of the first `invoke` call, narrowed away from buffer-shaped variants. */
function recordedArgs(): Record<string, unknown> | undefined {
  const args = vi.mocked(invoke).mock.calls[0]?.[1];
  return typeof args === 'object' && args !== null && !Array.isArray(args)
    ? (args as Record<string, unknown>)
    : undefined;
}

describe('call failure mapping', () => {
  it('wraps a host ApiError in a CommandError with the same kind and detail', async () => {
    const apiError: ApiError = { kind: 'network', message: 'no route to host' };
    vi.mocked(invoke).mockRejectedValue(apiError);

    const error = await rejectionOf(() => listPeers());

    expect(error.kind).toBe('network');
    expect(error.detail).toBe(apiError);
    expect(error.message).toBe('network: no route to host');
  });

  it('treats a rejected string as an internal error carrying that message', async () => {
    vi.mocked(invoke).mockRejectedValue('the command is not registered');

    const error = await rejectionOf(() => listPeers());

    expect(error.kind).toBe('internal');
    expect(error.message).toBe('internal: the command is not registered');
  });

  it('treats a rejected non-string as an internal error with the generic sentence', async () => {
    vi.mocked(invoke).mockRejectedValue({ code: 7 });

    const error = await rejectionOf(() => listPeers());

    expect(error.kind).toBe('internal');
    expect(error.message).toBe('internal: the host refused the command');
  });

  it('resolves the host value unchanged', async () => {
    const peers = [makePeer({ deviceId: PEER }), makePeer({ deviceId: OTHER_PEER })];
    vi.mocked(invoke).mockResolvedValue(peers);

    await expect(listPeers()).resolves.toBe(peers);
  });
});

interface CommandCase {
  readonly title: string;
  readonly run: () => Promise<unknown>;
  readonly command: string;
  readonly args: Record<string, unknown> | undefined;
}

const commandCases: readonly CommandCase[] = [
  { title: 'bootstrap', run: () => bootstrap(), command: 'bootstrap', args: undefined },
  { title: 'listPeers', run: () => listPeers(), command: 'list_peers', args: undefined },
  {
    title: 'history with a cursor and a limit',
    run: () => history(PEER, CURSOR, 25),
    command: 'history',
    args: { peerId: PEER, before: CURSOR, limit: 25 },
  },
  {
    title: 'history with a null cursor and no limit',
    run: () => history(PEER, null),
    command: 'history',
    args: { peerId: PEER, before: null, limit: undefined },
  },
  {
    title: 'sendMessage with attachments',
    run: () => sendMessage(PEER, 'hello', ['/a.png', '/b.pdf']),
    command: 'send_message',
    args: { peerId: PEER, body: 'hello', attachments: ['/a.png', '/b.pdf'] },
  },
  { title: 'pickFiles', run: () => pickFiles(), command: 'pick_files', args: undefined },
  {
    title: 'inspectFiles',
    run: () => inspectFiles(['/a.png', '/b.pdf']),
    command: 'inspect_files',
    args: { paths: ['/a.png', '/b.pdf'] },
  },
  {
    title: 'openAttachment',
    run: () => openAttachment(ATTACHMENT_ID),
    command: 'open_attachment',
    args: { attachmentId: ATTACHMENT_ID },
  },
  {
    title: 'revealAttachment',
    run: () => revealAttachment(ATTACHMENT_ID),
    command: 'reveal_attachment',
    args: { attachmentId: ATTACHMENT_ID },
  },
  {
    title: 'saveAttachment',
    run: () => saveAttachment(ATTACHMENT_ID),
    command: 'save_attachment',
    args: { attachmentId: ATTACHMENT_ID },
  },
  {
    title: 'cancelAttachment',
    run: () => cancelAttachment(ATTACHMENT_ID),
    command: 'cancel_attachment',
    args: { attachmentId: ATTACHMENT_ID },
  },
  {
    title: 'retryAttachment',
    run: () => retryAttachment(ATTACHMENT_ID),
    command: 'retry_attachment',
    args: { attachmentId: ATTACHMENT_ID },
  },
  {
    title: 'markRead',
    run: () => markRead(PEER),
    command: 'mark_read',
    args: { peerId: PEER },
  },
  {
    title: 'forgetPeer',
    run: () => forgetPeer(PEER, true),
    command: 'forget_peer',
    args: { peerId: PEER, deleteHistory: true },
  },
  {
    title: 'restorePeer',
    run: () => restorePeer(PEER),
    command: 'restore_peer',
    args: { peerId: PEER },
  },
  {
    title: 'setPeerMuted',
    run: () => setPeerMuted(PEER, true),
    command: 'set_peer_muted',
    args: { peerId: PEER, muted: true },
  },
  { title: 'knownDevices', run: () => knownDevices(), command: 'known_devices', args: undefined },
  { title: 'clearHistory', run: () => clearHistory(), command: 'clear_history', args: undefined },
  { title: 'ownProfile', run: () => ownProfile(), command: 'own_profile', args: undefined },
  {
    title: 'setNickname',
    run: () => setNickname('Ada'),
    command: 'set_nickname',
    args: { nickname: 'Ada' },
  },
  {
    title: 'completeOnboarding',
    run: () => completeOnboarding('Ada'),
    command: 'complete_onboarding',
    args: { nickname: 'Ada' },
  },
  { title: 'getSettings', run: () => getSettings(), command: 'get_settings', args: undefined },
  {
    title: 'updateSettings',
    run: () => updateSettings(settings),
    command: 'update_settings',
    args: { settings },
  },
  {
    title: 'isAutostartEnabled',
    run: () => isAutostartEnabled(),
    command: 'is_autostart_enabled',
    args: undefined,
  },
  {
    title: 'setUiLabels',
    run: () => setUiLabels(labels),
    command: 'set_ui_labels',
    args: { labels },
  },
  {
    title: 'setWindowAccent',
    run: () => setWindowAccent('#101010', '#ffffff'),
    command: 'set_window_accent',
    args: { accent: '#101010', onAccent: '#ffffff' },
  },
  {
    title: 'setActiveChat with a peer',
    run: () => setActiveChat(PEER),
    command: 'set_active_chat',
    args: { peerId: PEER },
  },
  {
    title: 'setActiveChat with no peer',
    run: () => setActiveChat(null),
    command: 'set_active_chat',
    args: { peerId: null },
  },
  { title: 'showWindow', run: () => showWindow(), command: 'show_window', args: undefined },
  { title: 'hideWindow', run: () => hideWindow(), command: 'hide_window', args: undefined },
  { title: 'quit', run: () => quit(), command: 'quit', args: undefined },
  { title: 'diagnostics', run: () => diagnostics(), command: 'diagnostics', args: undefined },
  { title: 'logsInfo', run: () => logsInfo(), command: 'logs_info', args: undefined },
  {
    title: 'openLogsFolder',
    run: () => openLogsFolder(),
    command: 'open_logs_folder',
    args: undefined,
  },
  { title: 'clearLogs', run: () => clearLogs(), command: 'clear_logs', args: undefined },
  {
    title: 'logFrontend with no context',
    run: () => logFrontend('warn', 'something happened'),
    command: 'log_frontend',
    args: { level: 'warn', message: 'something happened', context: null },
  },
  {
    title: 'logFrontend with a context',
    run: () => logFrontend('error', 'something broke', 'chat.spec'),
    command: 'log_frontend',
    args: { level: 'error', message: 'something broke', context: 'chat.spec' },
  },
];

describe('command wrappers', () => {
  it.each(commandCases)('$title', async ({ run, command, args }) => {
    await run();

    expect(invoke).toHaveBeenCalledWith(command, args);
  });

  it('passes sendMessage a fresh copy of the attachment list', async () => {
    const attachments = ['/a.png', '/b.pdf'];

    await sendMessage(PEER, 'hello', attachments);

    const passed = recordedArgs()?.['attachments'];
    expect(passed).toEqual(['/a.png', '/b.pdf']);
    expect(passed).not.toBe(attachments);
  });

  it('defaults sendMessage attachments to an empty array', async () => {
    await sendMessage(PEER, 'hello');

    expect(invoke).toHaveBeenCalledWith('send_message', {
      peerId: PEER,
      body: 'hello',
      attachments: [],
    });
  });

  it('passes inspectFiles a fresh copy of the path list', async () => {
    const paths = ['/a.png', '/b.pdf'];

    await inspectFiles(paths);

    const passed = recordedArgs()?.['paths'];
    expect(passed).toEqual(['/a.png', '/b.pdf']);
    expect(passed).not.toBe(paths);
  });

  it('resolves a file source through convertFileSrc without invoking a command', () => {
    vi.mocked(convertFileSrc).mockReturnValue('asset://localhost/stored.png');

    expect(fileSrc('/tmp/stored.png')).toBe('asset://localhost/stored.png');
    expect(convertFileSrc).toHaveBeenCalledWith('/tmp/stored.png');
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe('onCoreEvent', () => {
  it('hands the handler the event payload and returns the unlisten function', async () => {
    const unlisten = vi.fn();
    let delivered: ((event: { payload: unknown }) => void) | undefined;
    vi.mocked(listen).mockImplementation((_name, handler) => {
      delivered = handler as unknown as (event: { payload: unknown }) => void;
      return Promise.resolve(unlisten);
    });
    const handler = vi.fn<(payload: CoreEventMap['notice']) => void>();

    const result = await onCoreEvent('notice', handler);

    expect(listen).toHaveBeenCalledWith('notice', expect.any(Function));
    expect(result).toBe(unlisten);

    const payload: CoreEventMap['notice'] = { level: 'error', message: 'boom' };
    const envelope = { event: 'notice', id: 7, payload };
    delivered?.(envelope);

    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler).toHaveBeenCalledWith(payload);
    expect(handler).not.toHaveBeenCalledWith(envelope);
  });
});

beforeEach(() => {
  vi.mocked(invoke).mockResolvedValue(undefined);
});
