/**
 * English messages.
 *
 * This file is the source of truth for the message catalogue: `MessageKey` is derived from
 * its keys, so `t('typo.key')` is a compile error in every component.
 *
 * Plural variants use the CLDR categories the runtime reports through `Intl.PluralRules`:
 * a message `x.y` may be specialised as `x.y.one`, `x.y.few`, `x.y.many`, `x.y.other`, and a
 * locale that needs fewer categories simply declares fewer. English needs `one`/`other`;
 * Russian also needs `few`/`many`.
 *
 * Relative times are *not* in here. They come from `Intl.RelativeTimeFormat`, which already
 * knows that Russian says «5 минут назад» and English «5 minutes ago», including the
 * singular/plural agreement this file would otherwise have to reimplement.
 */
export const en = {
  'app.name': 'LocalMe',
  'app.tagline': 'Messages on your local network',

  'common.cancel': 'Cancel',
  'common.confirm': 'Confirm',
  'common.close': 'Close',
  'common.save': 'Save',
  'common.delete': 'Delete',
  'common.search': 'Search',
  'common.retry': 'Retry',
  'common.back': 'Back',
  'common.loading': 'Loading…',
  'common.you': 'You',
  'common.dismiss': 'Dismiss',
  'common.more': 'More',
  'common.options': 'Options',
  'common.clear': 'Clear',

  'onboarding.title': 'Welcome to LocalMe',
  'onboarding.subtitle':
    'LocalMe finds the other computers running it on your network. No account, no server.',
  'onboarding.nickname.label': 'What should people call you?',
  'onboarding.nickname.placeholder': 'Your name',
  'onboarding.nickname.help': 'Between 1 and 32 characters. This is not a unique username.',
  'onboarding.nickname.errorEmpty': 'Please enter a name.',
  'onboarding.nickname.errorTooLong': 'That name is longer than {max} characters.',
  'onboarding.nickname.errorControl': 'That name contains characters that cannot be used.',
  'onboarding.avatarPreview': 'This is how your avatar will look',
  'onboarding.submit': 'Start messaging',

  'users.title': 'People',
  'users.emptyTitle': 'Nobody here yet',
  'users.emptyBody':
    'Make sure the other computer is on the same network and has LocalMe running. Discovery usually takes a few seconds.',
  'users.searchPlaceholder': 'Search people',
  'users.clearSearch': 'Clear search',
  'users.searchEmpty': 'Nobody matches “{query}”.',
  'users.online': 'online',
  'users.offline': 'offline',
  'users.lastSeen': 'last seen {relative}',
  'users.neverSeen': 'never online',
  'users.unread': '{count} unread messages',
  'users.unread.one': '{count} unread message',
  'users.forget': 'Forget…',
  'users.muted': 'Notifications muted',
  'users.openChatWith': 'Conversation with {name}',
  'users.actionsFor': 'Actions for {name}',
  'users.mute': 'Mute notifications',
  'users.unmute': 'Unmute notifications',
  'users.you': 'You ({name})',
  'users.count': 'People: {count}',

  'forget.title': 'Forget {name}?',
  'forget.body':
    'They will disappear from your list. If their computer appears on the network again, it will be added as a new person.',
  'forget.deleteHistory': 'Also delete the message history',
  'forget.confirm': 'Forget',

  'chat.emptyTitle': 'Pick someone to talk to',
  'chat.emptyBody': 'Choose a person from the list to open the conversation.',
  'chat.composerPlaceholder': 'Message {name}',
  'chat.composerOffline': 'Messages cannot be sent while {name} is offline',
  'chat.composerOfflineHint': '{name} is offline. You can write again once they are back.',
  'chat.send': 'Send',
  'chat.tooLong': 'That message is longer than {max} characters.',
  'chat.composerHint': 'Enter sends, Shift+Enter starts a new line',
  'chat.statusSending': 'Sending…',
  'chat.statusSent': 'Sent',
  'chat.statusDelivered': 'Delivered',
  'chat.statusFailed': 'Not delivered',
  'chat.loadOlder': 'Load earlier messages',
  'chat.dayToday': 'Today',
  'chat.dayYesterday': 'Yesterday',
  'chat.jumpToLatest': 'Jump to latest messages',
  'chat.messageList': 'Messages with {name}',
  'chat.sending': 'Sending your message',
  'chat.historyEnd': 'This is the beginning of the conversation',

  'settings.title': 'Settings',
  'settings.close': 'Back to messages',
  'settings.groupProfile': 'Profile',
  'settings.groupAppearance': 'Appearance',
  'settings.groupLanguage': 'Language',
  'settings.groupNotifications': 'Notifications',
  'settings.groupSystem': 'System',
  'settings.groupData': 'Data',

  'settings.nickname': 'Nickname',
  'settings.nicknameHelp': 'Shown to everyone on the network.',

  'settings.theme': 'Theme',
  'settings.themeSystem': 'System',
  'settings.themeLight': 'Light',
  'settings.themeDark': 'Dark',
  'settings.accent': 'Accent colour',
  'settings.accentHelp': 'Changes the whole palette, not just one button.',

  'settings.language': 'Interface language',
  'settings.languageHelp': 'Applies immediately and is remembered.',

  'settings.notificationsEnabled': 'Show notifications',
  'settings.notificationsEnabledHelp': 'New messages can still arrive when this is off.',
  'settings.showMessageText': 'Include the message text',
  'settings.showMessageTextHelp':
    'Turn this off to be told a message arrived without showing what it says.',
  'settings.notificationSound': 'Play a sound',

  'settings.autostart': 'Start LocalMe when I sign in',
  'settings.startMinimized': 'Start minimised in the tray',
  'settings.closeToTray': 'Keep running in the tray when the window is closed',
  'settings.closeToTrayHelp':
    'With this off, closing the window quits LocalMe and you stop receiving messages.',

  'settings.clearHistory': 'Clear all message history',
  'settings.clearHistoryHelp': 'Removes every conversation on this computer.',
  'settings.clearHistoryConfirm': 'Delete every message?',
  'settings.knownDevices': 'Known devices',
  'settings.devicesEmpty': 'No devices have been seen yet.',
  'settings.deviceForgotten': 'forgotten',
  'settings.deviceKnown': 'known',
  'settings.restoreDevice': 'Start receiving from this device again',

  'settings.aboutTitle': 'About',
  'settings.aboutVersion': 'Version {version}',
  'settings.aboutArchitecture': 'Local network messenger',
  'settings.aboutLicense': 'Licensed under the MIT licence.',
  'settings.aboutDiagnostics': 'Diagnostics',
  'settings.devicesKnown': 'Known',
  'settings.devicesForgotten': 'Forgotten',
  'settings.saveFailed': 'That setting could not be saved.',
  'settings.historyCleared': 'Deleted {count} messages.',
  'settings.notificationsPerPeerHelp':
    'Individual people can be muted from the ⋮ menu in the list.',
  'locale.en': 'English',
  'locale.ru': 'Русский',

  'tray.open': 'Open LocalMe',
  'tray.mute': 'Pause notifications',
  'tray.unmute': 'Resume notifications',
  'tray.quit': 'Quit LocalMe',
  'tray.tooltipIdle': 'LocalMe — no unread messages',
  'tray.tooltipUnread': 'LocalMe — {count} unread',

  'notification.newMessage': 'New message',
  'notification.newMessageCount': '{count} new messages',
  'notification.newMessageCount.one': '{count} new message',

  'error.storage': 'LocalMe could not read its own database.',
  'error.discovery': 'LocalMe could not look for other devices on this network.',
  'error.network': 'That could not be delivered.',
  'error.internal': 'Something went wrong. The technical details are in the log.',
  'error.unknownPeer': 'That person is no longer in your list.',
  'error.peerOffline': 'They are offline right now.',
  'error.shuttingDown': 'LocalMe is closing.',
  'error.databaseRecovered':
    'The message database was damaged and could not be used. It was kept as {path} and a new database was created.',
};
