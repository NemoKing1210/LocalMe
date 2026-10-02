import { readonly, ref, watch, type App, type Ref } from 'vue';

import {
  formatBytes,
  formatClockTime,
  formatDateTime,
  formatDayHeading,
  formatDays,
  formatRelativeTime,
  pluralRulesFor,
} from './format';
import { FALLBACK_LOCALE, LOCALES, type Locale } from './locales';
import { de } from './messages/de';
import { en } from './messages/en';
import { es } from './messages/es';
import { fr } from './messages/fr';
import { pt } from './messages/pt';
import { ru } from './messages/ru';
import { zh } from './messages/zh';
import type { MessageCatalog, MessageKey, MessageParams } from './messages/types';

export {
  byteUnit,
  dayKey,
  formatBytes,
  formatClockTime,
  formatDateTime,
  formatDayHeading,
  formatDays,
  formatRelativeTime,
} from './format';
export { LOCALES, normalizeLocale, FALLBACK_LOCALE } from './locales';
export type { Locale } from './locales';
export type { MessageKey, MessageParams } from './messages/types';

const CATALOGS: Record<Locale, MessageCatalog> = { en, ru, es, de, fr, pt, zh };

const selectedLocale = ref<Locale>(FALLBACK_LOCALE);

export const locale: Readonly<Ref<Locale>> = readonly(selectedLocale);

export function setLocale(next: Locale): void {
  if (LOCALES.includes(next)) selectedLocale.value = next;
}

/**
 * Substitutes `{placeholders}`.
 *
 * Unknown placeholders are left in place rather than blanked: a visible `{name}` in the UI
 * is a bug report, an empty string is a mystery.
 */
export function interpolate(template: string, params?: MessageParams): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) => {
    const value = params[name];
    return value === undefined ? match : String(value);
  });
}

export function translate(key: MessageKey, params?: MessageParams): string {
  const catalog = CATALOGS[selectedLocale.value];
  const count = typeof params?.count === 'number' ? params.count : undefined;

  if (count !== undefined) {
    const category = pluralRulesFor(selectedLocale.value).select(count);
    const specialised = catalog[`${key}.${category}`] ?? catalog[`${key}.other`];
    if (specialised) return interpolate(specialised, params);
  }

  const message = catalog[key] ?? CATALOGS[FALLBACK_LOCALE][key];
  return interpolate(message, params);
}

export interface UseI18n {
  readonly locale: Readonly<Ref<Locale>>;
  readonly locales: readonly Locale[];
  readonly setLocale: (next: Locale) => void;
  readonly t: (key: MessageKey, params?: MessageParams) => string;
  readonly relative: (timestampMs: number, nowMs?: number) => string;
  readonly clock: (timestampMs: number) => string;
  readonly dateTime: (timestampMs: number) => string;
  readonly dayHeading: (timestampMs: number, nowMs?: number) => string;
  readonly bytes: (value: number) => string;
  readonly days: (count: number) => string;
}

export function useI18n(): UseI18n {
  return {
    locale: readonly(selectedLocale),
    locales: LOCALES,
    setLocale,
    t: translate,
    relative(timestampMs: number, nowMs = Date.now()): string {
      return formatRelativeTime(timestampMs, nowMs, selectedLocale.value);
    },
    clock(timestampMs: number): string {
      return formatClockTime(timestampMs, selectedLocale.value);
    },
    dateTime(timestampMs: number): string {
      return formatDateTime(timestampMs, selectedLocale.value);
    },
    dayHeading(timestampMs: number, nowMs = Date.now()): string {
      return formatDayHeading(timestampMs, nowMs, selectedLocale.value, {
        today: translate('chat.dayToday'),
        yesterday: translate('chat.dayYesterday'),
      });
    },
    bytes(value: number): string {
      return formatBytes(value, selectedLocale.value);
    },
    days(count: number): string {
      return formatDays(count, selectedLocale.value);
    },
  };
}

export const i18n = {
  install(app: App): void {
    app.config.globalProperties.$locale = selectedLocale;
    watch(
      selectedLocale,
      (current) => {
        document.documentElement.lang = current;
      },
      { immediate: true },
    );
  },
};
