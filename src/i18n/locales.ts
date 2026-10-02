export const LOCALES = ['en', 'ru', 'es', 'de', 'fr', 'pt', 'zh'] as const;

export type Locale = (typeof LOCALES)[number];

export const FALLBACK_LOCALE: Locale = 'en';

export function normalizeLocale(tag: string | undefined | null): Locale {
  if (!tag) return FALLBACK_LOCALE;
  const base = tag.toLowerCase().split(/[-_]/)[0];
  return LOCALES.find((locale) => locale === base) ?? FALLBACK_LOCALE;
}
