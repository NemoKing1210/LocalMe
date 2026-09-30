/**
 * Internationalisation.
 *
 * Design choices, and why they are not a framework:
 *
 * * the catalogue is typed (`MessageCatalog`), so a missing key in a translation is a
 *   compile error and a typo in `t('…')` is a compile error at the call site;
 * * the selected locale lives in a module-level `ref`, so templates re-render on change and
 *   nothing has to be threaded through props;
 * * plural selection goes through `Intl.PluralRules` and relative/absolute dates through
 *   `Intl`, so no language-specific formatting logic is written by hand;
 * * there is no global-injection `$t` — components import `useI18n`, which keeps the
 *   dependency greppable and avoids `this`-typing escapes.
 */
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

/** The locale messages are currently resolved against. */
export const locale: Readonly<Ref<Locale>> = readonly(selectedLocale);

/** Switches the interface language. Persisting it is the settings store's job. */
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

/**
 * Resolves a key, preferring a plural specialisation when `count` is given.
 *
 * Missing translations fall back to English rather than showing the key, but the `i18n`
 * test suite fails when a catalogue is incomplete, so the fallback is a safety net for a
 * half-finished translation, not the normal path.
 */
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

/**
 * The composition-API view of the message catalogue.
 *
 * Note that these are plain functions, not reactive proxies: each one reads the current
 * locale when it is called, so a component re-renders correctly when the locale changes
 * without any of them needing to be unwrapped in a template.
 */
export interface UseI18n {
  /** The active locale, reactive. */
  readonly locale: Readonly<Ref<Locale>>;
  /** Every supported locale, for the settings screen. */
  readonly locales: readonly Locale[];
  /** Switches the interface language. */
  readonly setLocale: (next: Locale) => void;
  /** Translates a key, substituting `{placeholders}`. */
  readonly t: (key: MessageKey, params?: MessageParams) => string;
  /** "5 минут назад" / "5 minutes ago", from `Intl.RelativeTimeFormat`. */
  readonly relative: (timestampMs: number, nowMs?: number) => string;
  /** Clock time of a message: "14:05" or "2:05 PM". */
  readonly clock: (timestampMs: number) => string;
  /** Full date and time, for tooltips. */
  readonly dateTime: (timestampMs: number) => string;
  /** Heading above a group of messages: Today, Yesterday, or a date. */
  readonly dayHeading: (timestampMs: number, nowMs?: number) => string;
  /** A file size in the language's own unit and number format: "1.5 kB". */
  readonly bytes: (value: number) => string;
  /** A count of days with the language's unit word: "14 days". */
  readonly days: (count: number) => string;
}

/** The composition-API entry point. */
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

/** Installs the locale into the application and keeps the document language in sync. */
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
