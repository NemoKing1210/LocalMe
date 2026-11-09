import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import { resolveLocale, setLocale as applyLocale } from '@/i18n';
import * as ipc from '@/ipc';
import type { LocalePreference, Settings, ThemeMode } from '@/ipc';
import { setAccentColor, setThemeMode } from '@/theme/useTheme';

export const useSettingsStore = defineStore('settings', () => {
  const document = ref<Settings | null>(null);
  const saving = ref(false);

  const theme = computed<ThemeMode>(() => document.value?.appearance.theme ?? 'system');
  const accent = computed<string>(() => document.value?.appearance.accent ?? '#6750A4');
  // The stored choice, not the language being displayed: `auto` is a real option in the
  // settings screen, and only `i18n.locale` holds what it resolved to.
  const locale = computed<LocalePreference>(() => document.value?.locale ?? 'auto');
  const notifications = computed(() => document.value?.notifications ?? null);
  const system = computed(() => document.value?.system ?? null);
  const onboarded = computed<boolean>(() => document.value?.onboarded ?? false);

  function apply(next: Settings): void {
    document.value = next;
    setThemeMode(next.appearance.theme);
    setAccentColor(next.appearance.accent);
    applyLocale(resolveLocale(next.locale));
  }

  // With `auto`, the system language can change while LocalMe is running — a regional setting
  // changed in the OS, or a different session — and the interface should follow without a
  // restart. The store is a singleton for the application's lifetime, so the listener does not
  // need removing.
  if (typeof window !== 'undefined') {
    window.addEventListener('languagechange', () => {
      if (document.value?.locale === 'auto') applyLocale(resolveLocale('auto'));
    });
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
