/**
 * Catalogue tests.
 *
 * The type system already guarantees that `t('…')` is called with a key the English catalogue
 * has. What it cannot see is the *other* catalogues: Russian is typed as
 * `Record<MessageKey, string>` plus optional plural variants, which catches a missing key but
 * not an extra one, a plural variant for a key that has no base, or a placeholder that was
 * renamed in one language and not the other. Those three are what this file checks, because
 * each of them produces a visible bug in exactly one language.
 */
import { describe, expect, it } from 'vitest';

import { en } from './messages/en';
import { ru } from './messages/ru';
import { LOCALES, normalizeLocale } from './locales';

/** `{placeholder}` names in a message, sorted. */
function placeholders(template: string): string[] {
  return [...template.matchAll(/\{(\w+)\}/g)].map((match) => match[1] ?? '').sort();
}

const CATALOGUES = { en, ru } as const;

describe('message catalogues', () => {
  const baseKeys = Object.keys(en) as (keyof typeof en)[];

  it('translates every base key into every language', () => {
    for (const locale of LOCALES) {
      const catalogue = CATALOGUES[locale];
      const missing = baseKeys.filter((key) => !(key in catalogue));
      expect(missing, `${locale} is missing: ${missing.join(', ')}`).toEqual([]);
    }
  });

  it('has no key that is neither a base key nor a plural variant of one', () => {
    for (const locale of LOCALES) {
      const catalogue = CATALOGUES[locale];
      const unknown = Object.keys(catalogue).filter((key) => {
        if (baseKeys.includes(key as keyof typeof en)) return false;
        const base = key.replace(/\.(zero|one|two|few|many|other)$/, '');
        return !baseKeys.includes(base as keyof typeof en);
      });
      expect(unknown, `${locale} has stray keys: ${unknown.join(', ')}`).toEqual([]);
    }
  });

  it('uses the same placeholders in every language', () => {
    for (const locale of LOCALES) {
      const catalogue = CATALOGUES[locale];
      for (const key of baseKeys) {
        const reference = placeholders(en[key]);
        const translated = catalogue[key];
        expect(
          placeholders(translated),
          `${locale}:${key} — "{…}" placeholders differ`,
        ).toEqual(reference);
      }
    }
  });

  it('declares plural variants only for keys that exist', () => {
    for (const locale of LOCALES) {
      const catalogue = CATALOGUES[locale];
      const pluralKeys = Object.keys(catalogue).filter((key) =>
        /\.(zero|one|two|few|many|other)$/.test(key),
      );
      for (const key of pluralKeys) {
        const base = key.replace(/\.(zero|one|two|few|many|other)$/, '');
        expect(key, `${locale}: ${key} has no base message`).toContain('.');
        expect(
          baseKeys.includes(base as keyof typeof en),
          `${locale}: ${key} has no base message`,
        ).toBe(true);
        // Every plural variant must interpolate the count, or the number would be missing
        // from exactly the languages that needed a special form.
        expect(catalogue[key as keyof typeof catalogue], `${locale}: ${key}`).toContain(
          '{count}',
        );
      }
    }
  });

  it('gives Russian the plural categories it actually needs', () => {
    // 1 → one, 2..4 → few, 5..20 → many. English needs only one/other; if the Russian
    // catalogue lacked `few` or `many` the interface would say «5 непрочитанных сообщения».
    const russian = ru as Record<string, string>;
    for (const suffix of ['one', 'few', 'many']) {
      expect(russian[`users.unread.${suffix}`], `users.unread.${suffix}`).toBeDefined();
    }
  });

  it('has no empty message', () => {
    for (const locale of LOCALES) {
      const catalogue = CATALOGUES[locale];
      const empty = Object.entries(catalogue).filter(([, text]) => text.trim().length === 0);
      expect(empty, `${locale} has empty messages`).toEqual([]);
    }
  });
});

describe('locale selection', () => {
  it('falls back to English for anything it does not know', () => {
    expect(normalizeLocale(undefined)).toBe('en');
    expect(normalizeLocale(null)).toBe('en');
    expect(normalizeLocale('')).toBe('en');
    expect(normalizeLocale('de-DE')).toBe('en');
    expect(normalizeLocale('xx')).toBe('en');
  });

  it('accepts a regional tag for a language it has', () => {
    expect(normalizeLocale('ru')).toBe('ru');
    expect(normalizeLocale('ru-RU')).toBe('ru');
    expect(normalizeLocale('RU_ru')).toBe('ru');
    expect(normalizeLocale('en-GB')).toBe('en');
  });
});
