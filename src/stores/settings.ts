/**
 * The settings document, and the side effects of changing it.
 *
 * The host owns the file; this store owns the *editing*. Every change is sent to the host and
 * the result is applied locally, so a failed write leaves the interface showing what is
 * actually stored rather than what the user tried to store. Two settings reach outside this
 * store: the theme drives the CSS custom properties, and the locale drives the catalogue — both
 * are told about a change here rather than watching the document.
 */
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

  /**
   * Applies a document to the interface.
   *
   * Safe to call with the host's own echo of a change: both paths run the same three
   * assignments, and they are idempotent.
   */
  function apply(next: Settings): void {
    document.value = next;
    setThemeMode(next.appearance.theme);
    setAccentColor(next.appearance.accent);
    applyLocale(next.locale);
  }

  /** Loads the document, for the paths that do not start with a bootstrap payload. */
  async function load(): Promise<void> {
    apply(await ipc.getSettings());
  }

  /**
   * Saves a modified document.
   *
   * The patch is applied to a copy of the current document, so a caller can change one field
   * without having to know the rest of the shape.
   */
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
