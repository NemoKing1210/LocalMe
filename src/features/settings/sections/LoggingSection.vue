<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';

import { useI18n } from '@/i18n';
import type { MessageKey } from '@/i18n';
import * as ipc from '@/ipc';
import type { LogLevel, LogsInfo, Settings } from '@/ipc';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';
import MdButton from '@/ui/MdButton.vue';
import MdDialog from '@/ui/MdDialog.vue';
import MdSegmentedButton from '@/ui/MdSegmentedButton.vue';

const settings = useSettingsStore();
const ui = useUiStore();
const i18n = useI18n();

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

const logs = ref<LogsInfo | null>(null);
const confirmClearLogs = ref(false);

const logLevelOptions = computed<readonly { readonly value: string; readonly label: string }[]>(
  () => LOG_LEVELS.map((value) => ({ value, label: i18n.t(LOG_LEVEL_LABELS[value]) })),
);

const logRetentionOptions = computed<readonly { readonly value: string; readonly label: string }[]>(
  () => LOG_RETENTION_PRESETS.map((days) => ({ value: `${days}`, label: i18n.days(days) })),
);

const logSummary = computed<string>(() => {
  const info = logs.value;
  if (info === null) return '';
  if (info.files.length === 0) return i18n.t('settings.logEmpty');
  return i18n.t('settings.logSummary', {
    count: info.files.length,
    size: i18n.bytes(info.totalBytes),
  });
});

function isLogLevel(value: string): value is LogLevel {
  return LOG_LEVELS.some((level) => level === value);
}

// The host owns the path and sizes, so this is reloaded after any change that can move them.
async function loadLogs(): Promise<void> {
  const info = await ui.attempt(
    () => ipc.logsInfo(),
    'the log directory could not be read',
    'settings.logsFailed',
  );
  if (info.ok) logs.value = info.value;
}

async function persist(patch: (current: Settings) => Settings): Promise<boolean> {
  const saved = await ui.attempt(
    () => settings.save(patch),
    'the settings document was not accepted',
    'settings.saveFailed',
  );
  return saved.ok;
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
  const saved = await persist((current) => ({
    ...current,
    logging: { ...current.logging, retentionDays: days },
  }));
  if (saved) await loadLogs();
}

async function openLogFolder(): Promise<void> {
  await ui.attempt(
    () => ipc.openLogsFolder(),
    'the log folder could not be opened',
    'settings.logFolderFailed',
  );
}

async function clearLogs(): Promise<void> {
  confirmClearLogs.value = false;
  const freed = await ui.attempt(() => ipc.clearLogs(), 'the log files could not be deleted');
  if (!freed.ok) return;
  ui.notify('settings.logsCleared', { size: i18n.bytes(freed.value) });
  await loadLogs();
}

onMounted(() => {
  void loadLogs();
});
</script>

<template>
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
          :label="i18n.t('settings.logLevel')"
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
          :label="i18n.t('settings.logRetention')"
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
      <p v-if="logs" class="md-typescale-body-small settings__help settings__mono" data-selectable>
        {{ logs.directory }}
      </p>
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
  </section>
</template>
