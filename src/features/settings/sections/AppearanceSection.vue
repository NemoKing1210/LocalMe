<script setup lang="ts">
import { computed } from 'vue';

import { useI18n } from '@/i18n';
import type { MessageKey } from '@/i18n';
import type { Settings, ThemeMode } from '@/ipc';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';
import { THEME_MODES, useTheme } from '@/theme/useTheme';
import MdSegmentedButton from '@/ui/MdSegmentedButton.vue';
import type { IconName } from '@/ui/icons';

const settings = useSettingsStore();
const ui = useUiStore();
const i18n = useI18n();
const theme = useTheme();

/**
 * Names for the preset swatches.
 *
 * A record rather than a computed key, so an accent added to the palette is a compile error here
 * and an accessible name the screen reader can actually say: `preset.id` is an English identifier.
 */
const ACCENT_LABELS = {
  baseline: 'settings.accentName.baseline',
  indigo: 'settings.accentName.indigo',
  teal: 'settings.accentName.teal',
  green: 'settings.accentName.green',
  amber: 'settings.accentName.amber',
  orange: 'settings.accentName.orange',
  red: 'settings.accentName.red',
  pink: 'settings.accentName.pink',
  slate: 'settings.accentName.slate',
} as const satisfies Record<string, MessageKey>;

function accentLabel(id: string): MessageKey {
  return Object.hasOwn(ACCENT_LABELS, id)
    ? ACCENT_LABELS[id as keyof typeof ACCENT_LABELS]
    : 'settings.accent';
}

const themeOptions = computed<
  readonly { readonly value: ThemeMode; readonly label: string; readonly icon: IconName }[]
>(() => [
  { value: 'system', label: i18n.t('settings.themeSystem'), icon: 'display' },
  { value: 'light', label: i18n.t('settings.themeLight'), icon: 'sun' },
  { value: 'dark', label: i18n.t('settings.themeDark'), icon: 'moon' },
]);

const isCustomAccent = computed(() => !theme.presets.some((preset) => isActiveAccent(preset.hex)));

function isThemeMode(value: string): value is ThemeMode {
  return THEME_MODES.some((mode) => mode === value);
}

function isActiveAccent(hex: string): boolean {
  return hex.toLowerCase() === settings.accent.toLowerCase();
}

async function persist(patch: (current: Settings) => Settings): Promise<void> {
  await ui.attempt(
    () => settings.save(patch),
    'the settings document was not accepted',
    'settings.saveFailed',
  );
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
</script>

<template>
  <section class="settings__group">
    <h2 class="md-typescale-title-medium settings__heading">
      {{ i18n.t('settings.groupAppearance') }}
    </h2>
    <div class="settings__card">
      <div class="settings__row">
        <span class="md-typescale-body-large">{{ i18n.t('settings.theme') }}</span>
        <MdSegmentedButton
          :model-value="settings.theme"
          :label="i18n.t('settings.theme')"
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
            :aria-label="i18n.t(accentLabel(preset.id))"
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
</template>
