<script setup lang="ts">
import { useI18n } from '@/i18n';
import type { Settings } from '@/ipc';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';
import MdSwitch from '@/ui/MdSwitch.vue';

const settings = useSettingsStore();
const ui = useUiStore();
const i18n = useI18n();

function persist(patch: (current: Settings) => Settings): Promise<unknown> {
  return ui.attempt(
    () => settings.save(patch),
    'the settings document was not accepted',
    'settings.saveFailed',
  );
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
</script>

<template>
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
        :supporting-text="i18n.t('settings.startMinimizedHelp')"
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
</template>
