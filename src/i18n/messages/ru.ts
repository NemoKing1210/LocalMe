/**
 * Russian messages.
 *
 * Typed against the English catalogue's keys through `MessageCatalog`, with the additional
 * CLDR plural categories Russian needs (`few`, `many`) permitted. The `i18n` test suite
 * fails if a key is missing, unknown, or if a placeholder does not match the English one.
 */
import type { MessageCatalog } from './types';

export const ru: MessageCatalog = {
  'app.name': 'LocalMe',
  'app.tagline': 'Сообщения в вашей локальной сети',

  'common.cancel': 'Отмена',
  'common.confirm': 'Подтвердить',
  'common.close': 'Закрыть',
  'common.save': 'Сохранить',
  'common.delete': 'Удалить',
  'common.search': 'Поиск',
  'common.retry': 'Повторить',
  'common.back': 'Назад',
  'common.loading': 'Загрузка…',
  'common.you': 'Вы',
  'common.dismiss': 'Скрыть',
  'common.more': 'Ещё',
  'common.options': 'Действия',
  'common.clear': 'Очистить',

  'onboarding.title': 'Добро пожаловать в LocalMe',
  'onboarding.subtitle':
    'LocalMe находит другие компьютеры с этой программой в вашей сети. Без аккаунта и сервера.',
  'onboarding.nickname.label': 'Как вас называть?',
  'onboarding.nickname.placeholder': 'Ваше имя',
  'onboarding.nickname.help': 'От 1 до 32 символов. Это не уникальное имя пользователя.',
  'onboarding.nickname.errorEmpty': 'Введите имя.',
  'onboarding.nickname.errorTooLong': 'Имя длиннее {max} символов.',
  'onboarding.nickname.errorControl': 'В имени есть недопустимые символы.',
  'onboarding.avatarPreview': 'Так будет выглядеть ваш аватар',
  'onboarding.submit': 'Начать общение',

  'users.title': 'Люди',
  'users.emptyTitle': 'Пока никого нет',
  'users.emptyBody':
    'Убедитесь, что другой компьютер в той же сети и LocalMe запущен. Обнаружение обычно занимает несколько секунд.',
  'users.searchPlaceholder': 'Поиск людей',
  'users.searchEmpty': 'Никто не найден по запросу «{query}».',
  'users.online': 'в сети',
  'users.offline': 'не в сети',
  'users.lastSeen': 'был(а) в сети {relative}',
  'users.neverSeen': 'ещё не был(а) в сети',
  'users.unread': '{count} непрочитанных сообщений',
  'users.unread.one': '{count} непрочитанное сообщение',
  'users.unread.few': '{count} непрочитанных сообщения',
  'users.unread.many': '{count} непрочитанных сообщений',
  'users.forget': 'Забыть…',
  'users.muted': 'Уведомления отключены',
  'users.openChatWith': 'Переписка с {name}',
  'users.actionsFor': 'Действия для {name}',
  'users.mute': 'Отключить уведомления',
  'users.unmute': 'Включить уведомления',
  'users.you': 'Вы ({name})',
  'users.count': 'Человек: {count}',

  'forget.title': 'Забыть {name}?',
  'forget.body':
    'Пользователь исчезнет из списка. Если его компьютер снова появится в сети, он будет добавлен как новый.',
  'forget.deleteHistory': 'Также удалить историю переписки',
  'forget.confirm': 'Забыть',

  'chat.emptyTitle': 'Выберите собеседника',
  'chat.emptyBody': 'Выберите человека из списка, чтобы открыть переписку.',
  'chat.composerPlaceholder': 'Сообщение для {name}',
  'chat.composerOffline': 'Нельзя отправлять сообщения, пока {name} не в сети',
  'chat.composerOfflineHint': '{name} не в сети. Написать снова можно, когда он(а) вернётся.',
  'chat.send': 'Отправить',
  'chat.tooLong': 'Сообщение длиннее {max} символов.',
  'chat.composerHint': 'Enter — отправить, Shift+Enter — новая строка',
  'chat.statusSending': 'Отправляется…',
  'chat.statusSent': 'Отправлено',
  'chat.statusDelivered': 'Доставлено',
  'chat.statusFailed': 'Не доставлено',
  'chat.loadOlder': 'Загрузить более ранние',
  'chat.dayToday': 'Сегодня',
  'chat.dayYesterday': 'Вчера',
  'chat.jumpToLatest': 'К последним сообщениям',
  'chat.messageList': 'Сообщения с {name}',
  'chat.sending': 'Отправляем сообщение',
  'chat.historyEnd': 'Это начало переписки',

  'settings.title': 'Настройки',
  'settings.close': 'К списку сообщений',
  'settings.groupProfile': 'Профиль',
  'settings.groupAppearance': 'Оформление',
  'settings.groupLanguage': 'Язык',
  'settings.groupNotifications': 'Уведомления',
  'settings.groupSystem': 'Система',
  'settings.groupData': 'Данные',

  'settings.nickname': 'Никнейм',
  'settings.nicknameHelp': 'Его видят все в сети.',
  'settings.deviceId': 'Идентификатор устройства',
  'settings.deviceIdHelp':
    'Этот идентификатор и есть вы. Он не меняется, даже если вы смените имя.',

  'settings.theme': 'Тема',
  'settings.themeSystem': 'Системная',
  'settings.themeLight': 'Светлая',
  'settings.themeDark': 'Тёмная',
  'settings.accent': 'Акцентный цвет',
  'settings.accentHelp': 'Меняет всю палитру, а не только одну кнопку.',

  'settings.language': 'Язык интерфейса',
  'settings.languageHelp': 'Применяется сразу и сохраняется.',

  'settings.notificationsEnabled': 'Показывать уведомления',
  'settings.notificationsEnabledHelp': 'Сообщения продолжат приходить, даже если это выключено.',
  'settings.showMessageText': 'Показывать текст сообщения',
  'settings.showMessageTextHelp':
    'Выключите, чтобы видеть только факт нового сообщения, без его текста.',
  'settings.notificationSound': 'Проигрывать звук',

  'settings.autostart': 'Запускать LocalMe при входе в систему',
  'settings.startMinimized': 'Запускать свёрнутым в трей',
  'settings.closeToTray': 'Оставаться в трее при закрытии окна',
  'settings.closeToTrayHelp':
    'Если выключено, закрытие окна завершает LocalMe и вы перестаёте получать сообщения.',

  'settings.clearHistory': 'Очистить всю историю сообщений',
  'settings.clearHistoryHelp': 'Удаляет все переписки на этом компьютере.',
  'settings.clearHistoryConfirm': 'Удалить все сообщения?',
  'settings.knownDevices': 'Известные устройства',
  'settings.devicesEmpty': 'Пока не найдено ни одного устройства.',
  'settings.deviceForgotten': 'забыто',
  'settings.deviceKnown': 'известно',
  'settings.restoreDevice': 'Снова принимать сообщения от этого устройства',

  'settings.aboutTitle': 'О приложении',
  'settings.aboutVersion': 'Версия {version}',
  'settings.aboutArchitecture': 'Мессенджер для локальной сети',
  'settings.aboutLicense': 'Распространяется по лицензии MIT.',
  'settings.aboutDiagnostics': 'Диагностика',
  'settings.devicesKnown': 'Известно',
  'settings.devicesForgotten': 'Забыто',
  'settings.saveFailed': 'Не удалось сохранить настройку.',
  'settings.historyCleared': 'Удалено сообщений: {count}.',
  'settings.notificationsPerPeerHelp':
    'Отдельным людям можно отключить уведомления в меню ⋮ в списке.',
  'locale.en': 'English',
  'locale.ru': 'Русский',

  'tray.open': 'Открыть LocalMe',
  'tray.mute': 'Приостановить уведомления',
  'tray.unmute': 'Возобновить уведомления',
  'tray.quit': 'Выйти из LocalMe',
  'tray.tooltipIdle': 'LocalMe — нет непрочитанных',
  'tray.tooltipUnread': 'LocalMe — непрочитанных: {count}',

  'notification.newMessage': 'Новое сообщение',
  'notification.newMessageCount': '{count} новых сообщений',
  'notification.newMessageCount.one': '{count} новое сообщение',
  'notification.newMessageCount.few': '{count} новых сообщения',
  'notification.newMessageCount.many': '{count} новых сообщений',

  'error.storage': 'LocalMe не удалось прочитать собственную базу данных.',
  'error.discovery': 'LocalMe не удалось найти другие устройства в этой сети.',
  'error.network': 'Не удалось доставить.',
  'error.internal': 'Что-то пошло не так. Технические подробности — в журнале.',
  'error.unknownPeer': 'Этого человека больше нет в вашем списке.',
  'error.peerOffline': 'Сейчас он(а) не в сети.',
  'error.shuttingDown': 'LocalMe завершает работу.',
  'error.databaseRecovered':
    'База сообщений была повреждена и не могла быть использована. Она сохранена как {path}, создана новая база.',
};
