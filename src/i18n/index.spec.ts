// @vitest-environment happy-dom

import { nextTick } from 'vue';
import { afterEach, describe, expect, it } from 'vitest';
import type { App } from 'vue';

import {
  FALLBACK_LOCALE,
  formatBytes,
  formatClockTime,
  formatDateTime,
  formatDays,
  formatRelativeTime,
  i18n,
  interpolate,
  locale,
  setLocale,
  translate,
  useI18n,
} from './index';
import type { Locale } from './locales';

afterEach(() => {
  setLocale(FALLBACK_LOCALE);
});

describe('interpolate', () => {
  it('returns the template untouched when there are no parameters', () => {
    expect(interpolate('nothing to fill in')).toBe('nothing to fill in');
  });

  it('substitutes named placeholders, repeating one that appears twice', () => {
    expect(interpolate('{name} and {name} plus {size}', { name: 'Ada', size: 3 })).toBe(
      'Ada and Ada plus 3',
    );
  });

  it('leaves an unknown or undefined placeholder in place rather than blanking it', () => {
    expect(interpolate('{known} {unknown}', { known: 'yes' })).toBe('yes {unknown}');
  });
});

describe('translate', () => {
  it('ignores a locale that is not in the list', () => {
    setLocale('klingon' as Locale);
    expect(locale.value).toBe(FALLBACK_LOCALE);
  });

  it('selects the plural form the language actually uses', () => {
    setLocale('en');
    expect(translate('users.unread', { count: 1 })).toContain('1');

    setLocale('ru');
    expect(translate('users.unread', { count: 2 })).toBe('2 непрочитанных сообщения');
    expect(translate('users.unread', { count: 5 })).toBe('5 непрочитанных сообщений');
    expect(translate('users.unread', { count: 1 })).toBe('1 непрочитанное сообщение');
  });

  it('falls back to the base key when the selected plural category has no variant', () => {
    setLocale('en');
    const withCount = translate('users.unread', { count: 5 });
    // English has only the `one` variant, so the plural path must not blank the message.
    expect(withCount).not.toBe('');
    expect(withCount).toContain('5');
  });
});

describe('useI18n', () => {
  it('exposes the current locale and every formatter bound to it', () => {
    setLocale('de');
    const api = useI18n();
    const at = Date.UTC(2026, 2, 15, 12, 0, 0);

    expect(api.locale.value).toBe('de');
    expect(api.t('app.name')).toBe('LocalMe');
    // Each formatter must use the *selected* locale; comparing against the standalone functions
    // is what proves the binding, without depending on this machine's time zone.
    expect(api.clock(at)).toBe(formatClockTime(at, 'de'));
    expect(api.bytes(2048)).toBe(formatBytes(2048, 'de'));
    expect(api.dateTime(at)).toBe(formatDateTime(at, 'de'));
    expect(api.days(2)).toBe(formatDays(2, 'de'));
    expect(api.relative(at - 60_000, at)).toBe(formatRelativeTime(at - 60_000, at, 'de'));
  });

  it('names a day heading from the catalogue strings for the current locale', () => {
    setLocale('en');
    const now = Date.UTC(2026, 2, 15, 12, 0, 0);
    expect(useI18n().dayHeading(now, now)).toBe(translate('chat.dayToday'));
    expect(useI18n().dayHeading(now - 86_400_000, now)).toBe(translate('chat.dayYesterday'));
  });
});

describe('the i18n plugin', () => {
  it('publishes the locale on the app and mirrors it onto the document language', async () => {
    const app = { config: { globalProperties: {} } } as unknown as App;
    i18n.install(app);

    expect(app.config.globalProperties.$locale.value).toBe(FALLBACK_LOCALE);

    setLocale('zh');
    await nextTick();
    expect(document.documentElement.lang).toBe('zh');
  });
});
