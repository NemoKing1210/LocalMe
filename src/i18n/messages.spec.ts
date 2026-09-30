/**
 * Catalogue tests.
 *
 * The type system already guarantees that `t('…')` is called with a key the English catalogue
 * has. What it cannot see is the *other* catalogues: each is typed as
 * `Record<MessageKey, string>` plus optional plural variants, which catches a missing key but
 * not an extra one, a plural variant for a key that has no base, or a placeholder that was
 * renamed in one language and not the other. Those three are what this file checks, because
 * each of them produces a visible bug in exactly one language.
 *
 * The languages themselves are a list in two places — `LOCALES` here and the `Locale` enum in
 * `localme-core` — so the tests below also pin the invariants a new language has to satisfy:
 * a catalogue, a label for itself, and plural forms its own `Intl` data actually selects.
 */
import { describe, expect, it } from 'vitest';

import { setLocale, translate } from './index';
import { de } from './messages/de';
import { en } from './messages/en';
import { es } from './messages/es';
import { fr } from './messages/fr';
import { pt } from './messages/pt';
import { ru } from './messages/ru';
import { zh } from './messages/zh';
import { LOCALES, normalizeLocale } from './locales';

/** `{placeholder}` names in a message, sorted. */
function placeholders(template: string): string[] {
  return [...template.matchAll(/\{(\w+)\}/g)].map((match) => match[1] ?? '').sort();
}

const CATALOGUES = { en, ru, es, de, fr, pt, zh } as const;

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
        expect(placeholders(translated), `${locale}:${key} — "{…}" placeholders differ`).toEqual(
          reference,
        );
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
        expect(catalogue[key as keyof typeof catalogue], `${locale}: ${key}`).toContain('{count}');
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

  it('names every language in its own list', () => {
    // The settings screen renders `locale.<code>` for each entry of `LOCALES`, so a language
    // without that key would show the key itself instead of its name.
    for (const locale of LOCALES) {
      expect(en[`locale.${locale}` as keyof typeof en], `locale.${locale}`).toBeTruthy();
    }
  });

  it('renders plurals with their number in every language', () => {
    // Walks the plural path for real: `Intl.PluralRules` in the runtime has to find a form for
    // every count, and whatever it finds must carry the number. A category the catalogue does
    // not declare falls back to the base message, which is why this asserts on the rendered
    // text rather than on the catalogue's shape.
    const counts = [0, 1, 2, 5, 11, 21, 101, 1_000_000];
    const keys = ['users.unread', 'notification.newMessageCount'] as const;
    for (const locale of LOCALES) {
      setLocale(locale);
      for (const key of keys) {
        for (const count of counts) {
          const text = translate(key, { count });
          expect(text, `${locale}:${key}:${count}`).not.toContain('{count}');
          expect(text, `${locale}:${key}:${count}`).toContain(String(count));
        }
      }
    }
    // Leave the module's locale as it was found.
    setLocale('en');
  });

  it('never falls back to English for a language it ships', () => {
    for (const locale of LOCALES) {
      if (locale === 'en') continue;
      const catalogue = CATALOGUES[locale];
      const identical = baseKeys.filter((key) => catalogue[key] === en[key]);
      // A handful of values are legitimately identical (the product name, the language names,
      // the `⋮`-style hints); a whole catalogue of them would mean the file was never
      // translated.
      expect(
        identical.length,
        `${locale} copies ${identical.length} English messages`,
      ).toBeLessThan(baseKeys.length / 4);
    }
  });
});

describe('locale selection', () => {
  it('falls back to English for anything it does not know', () => {
    expect(normalizeLocale(undefined)).toBe('en');
    expect(normalizeLocale(null)).toBe('en');
    expect(normalizeLocale('')).toBe('en');
    expect(normalizeLocale('fi-FI')).toBe('en');
    expect(normalizeLocale('xx')).toBe('en');
  });

  it('accepts a regional tag for a language it has', () => {
    expect(normalizeLocale('ru')).toBe('ru');
    expect(normalizeLocale('ru-RU')).toBe('ru');
    expect(normalizeLocale('RU_ru')).toBe('ru');
    expect(normalizeLocale('en-GB')).toBe('en');
    expect(normalizeLocale('de-DE')).toBe('de');
    expect(normalizeLocale('pt-BR')).toBe('pt');
    expect(normalizeLocale('zh-Hans-CN')).toBe('zh');
  });
});
