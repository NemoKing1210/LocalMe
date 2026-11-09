<script setup lang="ts">
import { computed } from 'vue';

import { AUTO_LOCALE, LOCALES, useI18n } from '@/i18n';
import type { LocalePreference, Settings } from '@/ipc';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';
import MdRadioGroup from '@/ui/MdRadioGroup.vue';

const settings = useSettingsStore();
const ui = useUiStore();
const i18n = useI18n();

const localeOptions = computed<
  readonly {
    readonly value: LocalePreference;
    readonly label: string;
    readonly description?: string;
  }[]
>(() => [
  {
    value: AUTO_LOCALE,
    label: i18n.t('locale.auto'),
    description: i18n.t('locale.autoHint'),
  },
  ...LOCALES.map((value) => ({ value, label: i18n.t(`locale.${value}`) })),
]);

function isLocalePreference(value: string): value is LocalePreference {
  return value === AUTO_LOCALE || LOCALES.some((locale) => locale === value);
}

function setLocale(value: string): void {
  if (!isLocalePreference(value)) return;
  void ui.attempt(
    () => settings.save((current: Settings) => ({ ...current, locale: value })),
    'the settings document was not accepted',
    'settings.saveFailed',
  );
}
</script>

<template>
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
    </div>
  </section>
</template>
