import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import { setLocale as applyLocale } from '@/i18n';
import * as ipc from '@/ipc';
import type { Locale, Settings, ThemeMode } from '@/ipc';
import { setAccentColor, setThemeMode } from '@/theme/useTheme';

export const useSettingsStore = defineStore('settings', () => {
  const document = ref<Settings | null>(null);
  const saving = ref(false);

  const theme = computed<ThemeMode>(() => document.value?.appearance.theme ?? 'system');
  const accent = computed<string>(() => document.value?.appearance.accent ?? '#6750A4');
  const locale = computed<Locale>(() => document.value?.locale ?? 'en');
  const notifications = computed(() => document.value?.notifications ?? null);
  const system = computed(() => document.value?.system ?? null);
  const onboarded = computed<boolean>(() => document.value?.onboarded ?? false);

  function apply(next: Settings): void {
    document.value = next;
    setThemeMode(next.appearance.theme);
    setAccentColor(next.appearance.accent);
    applyLocale(next.locale);
  }

  async function load(): Promise<void> {
    apply(await ipc.getSettings());
  }

  async function save(patch: (current: Settings) => Settings): Promise<void> {
    const current = document.value;
    if (current === null || saving.value) return;

    saving.value = true;
    try {
      apply(await ipc.updateSettings(patch(current)));
    } finally {
      saving.value = false;
    }
  }

  return {
    document,
    saving,
    theme,
    accent,
    locale,
    notifications,
    system,
    onboarded,
    apply,
    load,
    save,
  };
});
