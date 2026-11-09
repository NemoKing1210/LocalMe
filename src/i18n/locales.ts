export const LOCALES = ['en', 'ru', 'es', 'de', 'fr', 'pt', 'zh'] as const;

export type Locale = (typeof LOCALES)[number];

export const FALLBACK_LOCALE: Locale = 'en';

/** The stored preference that follows the operating system instead of naming a language. */
export const AUTO_LOCALE = 'auto';

export type LocalePreference = Locale | typeof AUTO_LOCALE;

export function normalizeLocale(tag: string | undefined | null): Locale {
  if (!tag) return FALLBACK_LOCALE;
  const base = tag.toLowerCase().split(/[-_]/)[0];
  return LOCALES.find((locale) => locale === base) ?? FALLBACK_LOCALE;
}

/**
 * Picks the first supported language from a system preference list.
 *
 * Each tag is matched by its base language only (`de-CH` → `de`); a list whose languages are all
 * unsupported falls back to English rather than to the first entry.
 */
export function detectLocale(languages: readonly string[]): Locale {
  for (const tag of languages) {
    const base = tag.toLowerCase().split(/[-_]/)[0];
    const match = LOCALES.find((locale) => locale === base);
    if (match !== undefined) return match;
  }
  return FALLBACK_LOCALE;
}

/**
 * The system language list as the web view reports it.
 *
 * `navigator.languages` is the ordered preference list; `navigator.language` is the single
 * fallback for engines that do not expose it. Outside a browser (unit tests, prerendering)
 * there is no language to read, which resolves to English.
 */
export function systemLanguages(): readonly string[] {
  if (typeof navigator === 'undefined') return [];
  const preferred = navigator.languages;
  if (preferred !== undefined && preferred.length > 0) return preferred;
  return navigator.language ? [navigator.language] : [];
}

export function resolveLocale(
  preference: LocalePreference,
  languages: readonly string[] = systemLanguages(),
): Locale {
  return preference === AUTO_LOCALE ? detectLocale(languages) : normalizeLocale(preference);
}
