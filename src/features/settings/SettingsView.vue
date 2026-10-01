<script setup lang="ts">
/**
 * The settings screen.
 *
 * One screen, six groups of cards, and one rule: the host owns every value. Nothing here keeps
 * a copy of a setting, so a rejected write leaves the screen showing what is actually stored
 * rather than what was attempted — which is why the profile nickname is the one field with a
 * local error of its own, and why the settings store is written through rather than assigned.
 *
 * The two lists on this screen (known devices, diagnostics) are the only state the view loads
 * itself, because neither is part of the settings document.
 */
import { computed, onMounted, ref, watch } from 'vue';
import { useRouter } from 'vue-router';

import { ROUTE } from '@/app/routes';
import { LOCALES, useI18n, type Locale, type MessageKey } from '@/i18n';
import * as ipc from '@/ipc';
import { CommandError } from '@/ipc';
import type {
  Diagnostics,
  KnownDevice,
  LogLevel,
  LogsInfo,
  Profile,
  Settings,
  ThemeMode,
} from '@/ipc';
import { THEME_MODES, useTheme } from '@/theme/useTheme';
import { useChatStore } from '@/stores/chat';
import { usePeerStore } from '@/stores/peers';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';
import MdAvatar from '@/ui/MdAvatar.vue';
import MdBadge from '@/ui/MdBadge.vue';
import MdButton from '@/ui/MdButton.vue';
import MdDialog from '@/ui/MdDialog.vue';
import MdIconButton from '@/ui/MdIconButton.vue';
import MdListItem from '@/ui/MdListItem.vue';
import MdRadioGroup from '@/ui/MdRadioGroup.vue';
import MdSegmentedButton from '@/ui/MdSegmentedButton.vue';
import MdSwitch from '@/ui/MdSwitch.vue';
import MdTextField from '@/ui/MdTextField.vue';
import MdTopAppBar from '@/ui/MdTopAppBar.vue';
import type { IconName } from '@/ui/icons';

const settings = useSettingsStore();
const chat = useChatStore();
const peers = usePeerStore();
const ui = useUiStore();
const i18n = useI18n();
const theme = useTheme();
const router = useRouter();

const LOG_LEVELS: readonly LogLevel[] = ['error', 'warn', 'info', 'debug'];
/** A record rather than a computed key, so a level added to the host is a compile error here. */
const LOG_LEVEL_LABELS: Record<LogLevel, MessageKey> = {
  error: 'settings.logLevelError',
  warn: 'settings.logLevelWarn',
  info: 'settings.logLevelInfo',
  debug: 'settings.logLevelDebug',
};
/** Retention presets, in days; the host clamps anything outside 1..=365. */
const LOG_RETENTION_PRESETS: readonly number[] = [7, 14, 30, 90];

const profile = ref<Profile | null>(null);
const devices = ref<readonly KnownDevice[]>([]);
const diagnostics = ref<Diagnostics | null>(null);
const logs = ref<LogsInfo | null>(null);
const nickname = ref('');
const nicknameError = ref<string | undefined>(undefined);
const confirmClear = ref(false);
const confirmClearLogs = ref(false);

const themeOptions = computed<
  readonly { readonly value: ThemeMode; readonly label: string; readonly icon: IconName }[]
>(() => [
  { value: 'system', label: i18n.t('settings.themeSystem'), icon: 'display' },
  { value: 'light', label: i18n.t('settings.themeLight'), icon: 'sun' },
  { value: 'dark', label: i18n.t('settings.themeDark'), icon: 'moon' },
]);

/**
 * The nickname field's error, bound as a whole prop object.
 *
 * `MdTextField` declares `errorText` without `| undefined`, and this project compiles with
 * `exactOptionalPropertyTypes`, so a possibly-undefined value is a type error even though "no
 * error" is exactly what it means.
 */
const nicknameFieldProps = computed(() => {
  const error = nicknameError.value;
  return error === undefined ? {} : { errorText: error };
});

const localeOptions = computed<readonly { readonly value: Locale; readonly label: string }[]>(() =>
  LOCALES.map((value) => ({ value, label: i18n.t(`locale.${value}`) })),
);

const versionLine = computed<string | null>(() => {
  const info = diagnostics.value;
  return info === null ? null : i18n.t('settings.aboutVersion', { version: info.version });
});

/**
 * The diagnostics block, labelled with the field names the host uses.
 *
 * They are not translated on purpose: this is the block a user copies into a bug report to
 * match what the Rust side logs, and a translated field name would not match anything.
 */
const diagnosticsRows = computed<readonly { readonly label: string; readonly value: string }[]>(
  () => {
    const info = diagnostics.value;
    if (info === null) return [];
    return [
      { label: 'protocolVersion', value: `${info.protocolVersion}` },
      { label: 'tcpPort', value: `${info.tcpPort}` },
      { label: 'platform', value: info.platform },
      { label: 'deviceId', value: info.deviceId },
    ];
  },
);

const isCustomAccent = computed(() => !theme.presets.some((preset) => isActiveAccent(preset.hex)));

const logLevelOptions = computed<readonly { readonly value: string; readonly label: string }[]>(
  () => LOG_LEVELS.map((value) => ({ value, label: i18n.t(LOG_LEVEL_LABELS[value]) })),
);

const logRetentionOptions = computed<readonly { readonly value: string; readonly label: string }[]>(
  () => LOG_RETENTION_PRESETS.map((days) => ({ value: `${days}`, label: i18n.days(days) })),
);

/** The line under the folder path: how many files there are and how much room they take. */
const logSummary = computed<string>(() => {
  const info = logs.value;
  if (info === null) return '';
  if (info.files.length === 0) return i18n.t('settings.logEmpty');
  return i18n.t('settings.logSummary', {
    count: info.files.length,
    size: i18n.bytes(info.totalBytes),
  });
});

function isThemeMode(value: string): value is ThemeMode {
  return THEME_MODES.some((mode) => mode === value);
}

function isLocale(value: string): value is Locale {
  return LOCALES.some((locale) => locale === value);
}

function isLogLevel(value: string): value is LogLevel {
  return LOG_LEVELS.some((level) => level === value);
}

function isActiveAccent(hex: string): boolean {
  return hex.toLowerCase() === settings.accent.toLowerCase();
}

/** Leaves the page the way it was entered. */
function close(): void {
  // `back()` when the user arrived from inside the application, which restores the conversation
  // they were reading; the chat page when the address was opened directly, where going back
  // would leave the window on nothing.
  const previous = router.options.history.state['back'];
  if (typeof previous === 'string' && previous.length > 0) router.back();
  else void router.replace({ name: ROUTE.chat });
}

/** Sends one change to the host and reports the failure in the language of the interface. */
async function persist(patch: (current: Settings) => Settings): Promise<void> {
  try {
    await settings.save(patch);
  } catch (error) {
    console.error('[localme] the settings document was not accepted', error);
    ui.fail('settings.saveFailed');
  }
}

async function loadProfile(): Promise<void> {
  try {
    const own = await ipc.ownProfile();
    profile.value = own;
    nickname.value = own.nickname;
  } catch (error) {
    console.error('[localme] the own profile could not be read', error);
    ui.fail('error.internal');
  }
}

async function loadDevices(): Promise<void> {
  try {
    devices.value = await ipc.knownDevices();
  } catch (error) {
    console.error('[localme] the known devices could not be read', error);
    ui.fail('error.internal');
  }
}

async function loadDiagnostics(): Promise<void> {
  try {
    diagnostics.value = await ipc.diagnostics();
  } catch (error) {
    console.error('[localme] the diagnostics could not be read', error);
    ui.fail('error.internal');
  }
}

/**
 * Reads the log directory.
 *
 * The host is the only side that knows the path and the sizes, so this is reloaded after every
 * change that can move them — opening the group, and deleting the files.
 */
async function loadLogs(): Promise<void> {
  try {
    logs.value = await ipc.logsInfo();
  } catch (error) {
    console.error('[localme] the log directory could not be read', error);
    ui.fail('settings.logsFailed');
  }
}

function setLogLevel(value: string): void {
  if (!isLogLevel(value)) return;
  void persist((current) => ({
    ...current,
    logging: { ...current.logging, level: value },
  }));
}

/** Retention is sent as a number, and the summary is reloaded because pruning happens at once. */
async function setLogRetention(value: string): Promise<void> {
  const days = Number.parseInt(value, 10);
  if (!Number.isFinite(days)) return;
  await persist((current) => ({
    ...current,
    logging: { ...current.logging, retentionDays: days },
  }));
  await loadLogs();
}

async function openLogFolder(): Promise<void> {
  try {
    await ipc.openLogsFolder();
  } catch (error) {
    console.error('[localme] the log folder could not be opened', error);
    ui.fail('settings.logFolderFailed');
  }
}

/** Deletes the files, then re-reads the directory so the summary cannot claim they are there. */
async function clearLogs(): Promise<void> {
  confirmClearLogs.value = false;
  try {
    const freed = await ipc.clearLogs();
    ui.notify('settings.logsCleared', { size: i18n.bytes(freed) });
    await loadLogs();
  } catch (error) {
    console.error('[localme] the log files could not be deleted', error);
    ui.fail('error.internal');
  }
}

/** Commits the nickname; the host validates it, and its refusal is shown on the field. */
async function commitNickname(): Promise<void> {
  const value = nickname.value.trim();
  const current = profile.value;
  if (current === null || value === current.nickname) {
    nickname.value = current?.nickname ?? nickname.value;
    return;
  }
  try {
    const updated = await ipc.setNickname(value);
    profile.value = updated;
    nickname.value = updated.nickname;
    nicknameError.value = undefined;
  } catch (error) {
    nicknameError.value = error instanceof CommandError ? error.message : i18n.t('error.internal');
  }
}

/** Lets a forgotten device back into the list. */
async function restoreDevice(deviceId: string): Promise<void> {
  try {
    await ipc.restorePeer(deviceId);
    await loadDevices();
  } catch (error) {
    console.error('[localme] the device could not be restored', error);
    ui.fail('error.internal');
  }
}

/** Deletes every stored message, after the confirmation dialog has said so twice. */
async function clearHistory(): Promise<void> {
  confirmClear.value = false;
  try {
    const count = await ipc.clearHistory();
    // The rows are gone from the database, so the open conversation has to be reloaded from it:
    // leaving the deleted messages on screen would say the opposite of what just happened.
    await chat.open(peers.selectedId);
    ui.notify('settings.historyCleared', { count });
    await loadDevices();
  } catch (error) {
    console.error('[localme] the history could not be cleared', error);
    ui.fail('error.internal');
  }
}

function setTheme(value: string): void {
  if (!isThemeMode(value)) return;
  void persist((current) => ({
    ...current,
    appearance: { ...current.appearance, theme: value },
  }));
}

function setAccent(hex: string): void {
  void persist((current) => ({
    ...current,
    appearance: { ...current.appearance, accent: hex },
  }));
}

/** The native colour input; `change` rather than `input`, so a drag is one write, not fifty. */
function onCustomAccent(event: Event): void {
  const target = event.target;
  if (target instanceof HTMLInputElement) setAccent(target.value);
}

function setLocale(value: string): void {
  if (!isLocale(value)) return;
  void persist((current) => ({ ...current, locale: value }));
}

function setNotificationsEnabled(value: boolean): void {
  void persist((current) => ({
    ...current,
    notifications: { ...current.notifications, enabled: value },
  }));
}

function setShowMessageText(value: boolean): void {
  void persist((current) => ({
    ...current,
    notifications: { ...current.notifications, showText: value },
  }));
}

function setNotificationSound(value: boolean): void {
  void persist((current) => ({
    ...current,
    notifications: { ...current.notifications, sound: value },
  }));
}

function setAutostart(value: boolean): void {
  void persist((current) => ({ ...current, system: { ...current.system, autostart: value } }));
}

function setStartMinimized(value: boolean): void {
  void persist((current) => ({ ...current, system: { ...current.system, startMinimized: value } }));
}

function setCloseToTray(value: boolean): void {
  void persist((current) => ({ ...current, system: { ...current.system, closeToTray: value } }));
}

/** What a known-device row says under the name: whether it is known, and when we last saw it. */
function deviceSupporting(device: KnownDevice): string {
  const status = device.forgotten
    ? i18n.t('settings.deviceForgotten')
    : i18n.t('settings.deviceKnown');
  const seen =
    device.lastSeenMs === null
      ? i18n.t('users.neverSeen')
      : i18n.t('users.lastSeen', { relative: i18n.relative(device.lastSeenMs) });
  return `${status} · ${seen}`;
}

// The error belongs to the value that was refused; typing a new one makes it stale.
watch(nickname, () => {
  nicknameError.value = undefined;
});

onMounted(() => {
  void loadProfile();
  void loadDevices();
  void loadDiagnostics();
  void loadLogs();
});
</script>

<template>
  <section class="settings">
    <MdTopAppBar :title="i18n.t('settings.title')">
      <template #leading>
        <MdIconButton icon="back" :label="i18n.t('settings.close')" @click="close" />
      </template>
    </MdTopAppBar>

    <div class="settings__scroll">
      <section class="settings__group">
        <h2 class="md-typescale-title-medium settings__heading">
          {{ i18n.t('settings.groupProfile') }}
        </h2>
        <div v-if="profile" class="settings__card">
          <div class="settings__profile">
            <MdAvatar
              :seed="profile.avatarSeed"
              :name="profile.nickname"
              :size="72"
              animate="always"
            />
            <div class="settings__profile-field" @focusout="commitNickname">
              <MdTextField
                v-bind="nicknameFieldProps"
                v-model="nickname"
                :label="i18n.t('settings.nickname')"
                :supporting-text="i18n.t('settings.nicknameHelp')"
                @submit="commitNickname"
              />
            </div>
          </div>
        </div>
      </section>

      <section class="settings__group">
        <h2 class="md-typescale-title-medium settings__heading">
          {{ i18n.t('settings.groupAppearance') }}
        </h2>
        <div class="settings__card">
          <div class="settings__row">
            <span class="md-typescale-body-large">{{ i18n.t('settings.theme') }}</span>
            <MdSegmentedButton
              :model-value="settings.theme"
              :options="themeOptions"
              @update:model-value="setTheme"
            />
          </div>
          <div class="settings__row">
            <span class="md-typescale-body-large settings__row-text">
              {{ i18n.t('settings.accent') }}
              <span class="md-typescale-body-medium settings__help">
                {{ i18n.t('settings.accentHelp') }}
              </span>
            </span>
            <div class="settings__swatches">
              <button
                v-for="preset in theme.presets"
                :key="preset.id"
                class="settings__swatch"
                :class="{ 'settings__swatch--active': isActiveAccent(preset.hex) }"
                type="button"
                :style="{ background: preset.hex }"
                :aria-label="preset.id"
                :aria-pressed="isActiveAccent(preset.hex)"
                @click="setAccent(preset.hex)"
              />
              <input
                class="settings__custom"
                :class="{ 'settings__custom--active': isCustomAccent }"
                type="color"
                :value="settings.accent"
                :aria-label="i18n.t('settings.accent')"
                @change="onCustomAccent"
              />
            </div>
          </div>
        </div>
      </section>

      <section class="settings__group">
        <h2 class="md-typescale-title-medium settings__heading">
          {{ i18n.t('settings.groupLanguage') }}
        </h2>
        <div class="settings__card">
          <MdRadioGroup
            :model-value="settings.locale"
            :label="i18n.t('settings.language')"
            :options="localeOptions"
            @update:model-value="setLocale"
          />
          <p class="md-typescale-body-small settings__help">
            {{ i18n.t('settings.languageHelp') }}
          </p>
        </div>
      </section>

      <section class="settings__group">
        <h2 class="md-typescale-title-medium settings__heading">
          {{ i18n.t('settings.groupNotifications') }}
        </h2>
        <div class="settings__card">
          <MdSwitch
            :model-value="settings.notifications?.enabled ?? false"
            :label="i18n.t('settings.notificationsEnabled')"
            :supporting-text="i18n.t('settings.notificationsEnabledHelp')"
            @update:model-value="setNotificationsEnabled"
          />
          <MdSwitch
            :model-value="settings.notifications?.showText ?? false"
            :label="i18n.t('settings.showMessageText')"
            :supporting-text="i18n.t('settings.showMessageTextHelp')"
            @update:model-value="setShowMessageText"
          />
          <MdSwitch
            :model-value="settings.notifications?.sound ?? false"
            :label="i18n.t('settings.notificationSound')"
            @update:model-value="setNotificationSound"
          />
          <p class="md-typescale-body-small settings__help">
            {{ i18n.t('settings.notificationsPerPeerHelp') }}
          </p>
        </div>
      </section>

      <section class="settings__group">
        <h2 class="md-typescale-title-medium settings__heading">
          {{ i18n.t('settings.groupSystem') }}
        </h2>
        <div class="settings__card">
          <MdSwitch
            :model-value="settings.system?.autostart ?? false"
            :label="i18n.t('settings.autostart')"
            @update:model-value="setAutostart"
          />
          <MdSwitch
            :model-value="settings.system?.startMinimized ?? false"
            :label="i18n.t('settings.startMinimized')"
            @update:model-value="setStartMinimized"
          />
          <MdSwitch
            :model-value="settings.system?.closeToTray ?? false"
            :label="i18n.t('settings.closeToTray')"
            :supporting-text="i18n.t('settings.closeToTrayHelp')"
            @update:model-value="setCloseToTray"
          />
        </div>
      </section>

      <section class="settings__group">
        <h2 class="md-typescale-title-medium settings__heading">
          {{ i18n.t('settings.groupData') }}
        </h2>
        <div class="settings__card">
          <h3 class="md-typescale-title-small">{{ i18n.t('settings.knownDevices') }}</h3>
          <p v-if="devices.length === 0" class="md-typescale-body-small settings__help">
            {{ i18n.t('settings.devicesEmpty') }}
          </p>
          <ul v-else class="settings__devices">
            <li v-for="device in devices" :key="device.deviceId">
              <MdListItem :headline="device.nickname" :supporting="deviceSupporting(device)">
                <template #leading>
                  <MdAvatar
                    :seed="device.avatarSeed"
                    :name="device.nickname"
                    :size="40"
                    :dimmed="device.forgotten"
                    animate="hover"
                  />
                </template>
                <template #trailing>
                  <MdBadge :value="device.messageCount" />
                  <MdIconButton
                    v-if="device.forgotten"
                    icon="refresh"
                    size="small"
                    :label="i18n.t('settings.restoreDevice')"
                    @click="restoreDevice(device.deviceId)"
                  />
                </template>
              </MdListItem>
            </li>
          </ul>
          <div class="settings__danger">
            <p class="md-typescale-body-small settings__help">
              {{ i18n.t('settings.clearHistoryHelp') }}
            </p>
            <MdButton
              class="settings__clear"
              variant="text"
              icon="trash"
              @click="confirmClear = true"
            >
              {{ i18n.t('settings.clearHistory') }}
            </MdButton>
          </div>
        </div>
      </section>

      <section class="settings__group">
        <h2 class="md-typescale-title-medium settings__heading">
          {{ i18n.t('settings.groupLogging') }}
        </h2>
        <div class="settings__card">
          <div class="settings__row">
            <span class="md-typescale-body-large settings__row-text">
              {{ i18n.t('settings.logLevel') }}
              <span class="md-typescale-body-medium settings__help">
                {{ i18n.t('settings.logLevelHelp') }}
              </span>
            </span>
            <MdSegmentedButton
              :model-value="settings.document?.logging.level ?? 'info'"
              :options="logLevelOptions"
              @update:model-value="setLogLevel"
            />
          </div>
          <div class="settings__row">
            <span class="md-typescale-body-large settings__row-text">
              {{ i18n.t('settings.logRetention') }}
              <span class="md-typescale-body-medium settings__help">
                {{ i18n.t('settings.logRetentionHelp') }}
              </span>
            </span>
            <MdSegmentedButton
              :model-value="`${settings.document?.logging.retentionDays ?? 14}`"
              :options="logRetentionOptions"
              @update:model-value="setLogRetention"
            />
          </div>
          <div class="settings__row">
            <span class="md-typescale-body-large settings__row-text">
              {{ i18n.t('settings.logFolder') }}
              <span class="md-typescale-body-medium settings__help">
                {{ logSummary }}
              </span>
            </span>
            <div class="settings__log-actions">
              <MdButton variant="tonal" icon="folder" @click="openLogFolder">
                {{ i18n.t('settings.openLogFolder') }}
              </MdButton>
              <MdButton
                class="settings__clear"
                variant="text"
                icon="trash"
                :disabled="(logs?.files.length ?? 0) === 0"
                @click="confirmClearLogs = true"
              >
                {{ i18n.t('settings.clearLogs') }}
              </MdButton>
            </div>
          </div>
          <p
            v-if="logs"
            class="md-typescale-body-small settings__help settings__mono"
            data-selectable
          >
            {{ logs.directory }}
          </p>
        </div>
      </section>

      <section class="settings__group">
        <h2 class="md-typescale-title-medium settings__heading">
          {{ i18n.t('settings.aboutTitle') }}
        </h2>
        <div class="settings__card">
          <p v-if="versionLine !== null" class="md-typescale-body-medium">{{ versionLine }}</p>
          <p class="md-typescale-body-medium settings__help">
            {{ i18n.t('settings.aboutArchitecture') }}
          </p>
          <p class="md-typescale-body-medium settings__help">
            {{ i18n.t('settings.aboutLicense') }}
          </p>
        </div>
        <div v-if="diagnosticsRows.length > 0" class="settings__card">
          <h3 class="md-typescale-title-small">{{ i18n.t('settings.aboutDiagnostics') }}</h3>
          <dl class="settings__pairs">
            <template v-for="row in diagnosticsRows" :key="row.label">
              <dt class="md-typescale-label-small settings__help">{{ row.label }}</dt>
              <dd class="md-typescale-body-small settings__mono" data-selectable>
                {{ row.value }}
              </dd>
            </template>
          </dl>
        </div>
      </section>
    </div>

    <MdDialog
      :open="confirmClearLogs"
      :headline="i18n.t('settings.clearLogsConfirm')"
      @close="confirmClearLogs = false"
    >
      <p class="md-typescale-body-medium">{{ i18n.t('settings.clearLogsHelp') }}</p>
      <template #actions>
        <MdButton variant="text" @click="confirmClearLogs = false">
          {{ i18n.t('common.cancel') }}
        </MdButton>
        <MdButton variant="filled" @click="clearLogs">
          {{ i18n.t('common.delete') }}
        </MdButton>
      </template>
    </MdDialog>

    <MdDialog
      :open="confirmClear"
      :headline="i18n.t('settings.clearHistoryConfirm')"
      @close="confirmClear = false"
    >
      <p class="md-typescale-body-medium">{{ i18n.t('settings.clearHistoryHelp') }}</p>
      <template #actions>
        <MdButton variant="text" @click="confirmClear = false">
          {{ i18n.t('common.cancel') }}
        </MdButton>
        <MdButton variant="filled" @click="clearHistory">
          {{ i18n.t('common.delete') }}
        </MdButton>
      </template>
    </MdDialog>
  </section>
</template>

<style scoped>
.settings {
  display: flex;
  flex: 1;
  min-height: 0;
  flex-direction: column;
}

.settings__scroll {
  display: flex;
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  flex-direction: column;
  gap: 24px;
  padding: 24px;
  /* As in the message log: the pane is sized by the shell, not by the height of its content. */
  contain: size;
}

.settings__group {
  display: flex;
  width: 100%;
  max-width: 720px;
  margin: 0 auto;
  flex-direction: column;
  gap: 8px;
}

.settings__heading {
  color: var(--md-sys-color-primary);
}

.settings__card {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 16px;
  border-radius: var(--md-sys-shape-corner-medium);
  background: var(--md-sys-color-surface-container-low);
}

.settings__row {
  display: flex;
  gap: 16px;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
}

.settings__row-text {
  display: flex;
  flex-direction: column;
}

.settings__help {
  color: var(--md-sys-color-on-surface-variant);
}

.settings__mono {
  font-family: var(--md-sys-typescale-font-family-mono);
  overflow-wrap: anywhere;
}

.settings__profile {
  display: flex;
  gap: 20px;
  align-items: flex-start;
}

.settings__profile-field {
  flex: 1;
  min-width: 0;
}

/* Field/value rows: the device id, and the diagnostics block. */
.settings__pairs {
  display: grid;
  grid-template-columns: minmax(0, auto) minmax(0, 1fr);
  gap: 4px 16px;
  margin: 0;
}

.settings__pairs dd {
  margin: 0;
}

.settings__swatches {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
}

.settings__swatch {
  width: 32px;
  height: 32px;
  border-radius: var(--md-sys-shape-corner-full);
}

.settings__swatch--active,
.settings__custom--active {
  box-shadow:
    0 0 0 2px var(--md-sys-color-surface-container-low),
    0 0 0 4px var(--md-sys-color-primary);
}

.settings__custom {
  width: 32px;
  height: 32px;
  padding: 0;
  border: none;
  border-radius: var(--md-sys-shape-corner-full);
  background: none;
  cursor: pointer;
  appearance: none;
}

.settings__custom::-webkit-color-swatch-wrapper {
  padding: 0;
}

.settings__custom::-webkit-color-swatch {
  border: none;
  border-radius: var(--md-sys-shape-corner-full);
}

.settings__devices {
  display: flex;
  flex-direction: column;
}

.settings__log-actions {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
}

.settings__danger {
  display: flex;
  gap: 12px;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
}

/* The one destructive action on the screen, and the only place the error role is a button. */
.settings__card :deep(.settings__clear) {
  color: var(--md-sys-color-error);
}
</style>
