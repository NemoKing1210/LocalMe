<script setup lang="ts">
import { computed } from 'vue';

import { LOCALES, useI18n } from '@/i18n';
import type { Locale, Settings } from '@/ipc';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';
import MdRadioGroup from '@/ui/MdRadioGroup.vue';

const settings = useSettingsStore();
const ui = useUiStore();
const i18n = useI18n();

const localeOptions = computed<readonly { readonly value: Locale; readonly label: string }[]>(() =>
  LOCALES.map((value) => ({ value, label: i18n.t(`locale.${value}`) })),
);

function isLocale(value: string): value is Locale {
  return LOCALES.some((locale) => locale === value);
}

function setLocale(value: string): void {
  if (!isLocale(value)) return;
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
