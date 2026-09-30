/**
 * Supported interface languages.
 *
 * Adding a language is a two-line change here plus a message file; the `Messages` type
 * makes a missing translation a compile error rather than a silent English fallback.
 */
export const LOCALES = ['en', 'ru', 'es', 'de', 'fr', 'pt', 'zh'] as const;

export type Locale = (typeof LOCALES)[number];

/** Locale used before the user, or the system, has expressed a preference. */
export const FALLBACK_LOCALE: Locale = 'en';

/** Narrows an arbitrary string, such as `navigator.language`, to a supported locale. */
export function normalizeLocale(tag: string | undefined | null): Locale {
  if (!tag) return FALLBACK_LOCALE;
  const base = tag.toLowerCase().split(/[-_]/)[0];
  return LOCALES.find((locale) => locale === base) ?? FALLBACK_LOCALE;
}
