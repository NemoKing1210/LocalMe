<script setup lang="ts">
import { useI18n } from '@/i18n';
import type { Settings } from '@/ipc';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';
import MdSwitch from '@/ui/MdSwitch.vue';

const settings = useSettingsStore();
const ui = useUiStore();
const i18n = useI18n();

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

function persist(patch: (current: Settings) => Settings): Promise<unknown> {
  return ui.attempt(
    () => settings.save(patch),
    'the settings document was not accepted',
    'settings.saveFailed',
  );
}
</script>

<template>
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
</template>
