import { describe, expect, it } from 'vitest';

import { byteUnit, dayKey, formatClockTime, formatDayHeading, formatRelativeTime } from './format';

const SECOND = 1_000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

const NOW = new Date('2026-03-15T12:00:00Z').getTime();

describe('formatRelativeTime', () => {
  it('says "now" for a moment ago', () => {
    // `numeric: 'auto'` turns the zero-second case into a word rather than "0 seconds ago".
    expect(formatRelativeTime(NOW, NOW, 'en')).toBe('now');
    expect(formatRelativeTime(NOW - 5 * SECOND, NOW, 'en')).toBe('5 seconds ago');
  });

  it('switches to minutes once seconds stop being useful', () => {
    expect(formatRelativeTime(NOW - 44 * SECOND, NOW, 'en')).toBe('44 seconds ago');
    expect(formatRelativeTime(NOW - 45 * SECOND, NOW, 'en')).toBe('1 minute ago');
    expect(formatRelativeTime(NOW - 2 * MINUTE, NOW, 'en')).toBe('2 minutes ago');
  });

  it('steps up through hours, days, months and years', () => {
    expect(formatRelativeTime(NOW - 59 * MINUTE, NOW, 'en')).toBe('59 minutes ago');
    expect(formatRelativeTime(NOW - HOUR, NOW, 'en')).toBe('1 hour ago');
    expect(formatRelativeTime(NOW - 23 * HOUR, NOW, 'en')).toBe('23 hours ago');
    expect(formatRelativeTime(NOW - DAY, NOW, 'en')).toBe('yesterday');
    expect(formatRelativeTime(NOW - 6 * DAY, NOW, 'en')).toBe('6 days ago');
    expect(formatRelativeTime(NOW - 60 * DAY, NOW, 'en')).toBe('2 months ago');
    expect(formatRelativeTime(NOW - 400 * DAY, NOW, 'en')).toBe('last year');
  });

  it('never shows a raw number of seconds in the thousands', () => {
    for (const age of [MINUTE, HOUR, DAY, 30 * DAY, 400 * DAY]) {
      const text = formatRelativeTime(NOW - age, NOW, 'en');
      expect(text).not.toMatch(/second/i);
    }
  });

  it('declines correctly in Russian', () => {
    // The reason relative times go through `Intl` instead of the message catalogue: getting
    // «1 минуту», «2 минуты», «5 минут» and «21 минуту» right by hand is exactly the bug this
    // avoids. `numeric: 'auto'` only drops the numeral for whole days, so "1 minute ago" keeps
    // its number here.
    expect(formatRelativeTime(NOW - MINUTE, NOW, 'ru')).toBe('1 минуту назад');
    expect(formatRelativeTime(NOW - 2 * MINUTE, NOW, 'ru')).toBe('2 минуты назад');
    expect(formatRelativeTime(NOW - 5 * MINUTE, NOW, 'ru')).toBe('5 минут назад');
    expect(formatRelativeTime(NOW - 21 * MINUTE, NOW, 'ru')).toBe('21 минуту назад');
    // A whole day is the one case where the numeral is replaced by a word.
    expect(formatRelativeTime(NOW - DAY, NOW, 'ru')).toBe('вчера');
  });

  it('handles a timestamp in the future without inventing a unit', () => {
    // A peer's clock is never trusted, but a stored timestamp can still be ahead of ours if
    // the machine's clock moved backwards. The text must stay sensible.
    expect(formatRelativeTime(NOW + 30 * SECOND, NOW, 'en')).toBe('in 30 seconds');
    expect(formatRelativeTime(NOW + 2 * HOUR, NOW, 'en')).toBe('in 2 hours');
  });
});

describe('formatClockTime', () => {
  it('produces a locale-shaped clock reading', () => {
    // The exact string depends on the ICU data the runtime ships, so the assertion is on the
    // shape: a time of day, never a date and never the raw timestamp.
    const en = formatClockTime(NOW, 'en');
    const ru = formatClockTime(NOW, 'ru');
    expect(en).toMatch(/\d{1,2}[:.]\d{2}/);
    expect(ru).toMatch(/\d{1,2}[:.]\d{2}/);
  });
});

describe('dayKey and formatDayHeading', () => {
  it('uses the local calendar day', () => {
    const morning = new Date(2026, 2, 15, 0, 5).getTime();
    const evening = new Date(2026, 2, 15, 23, 55).getTime();
    expect(dayKey(morning)).toBe(dayKey(evening));
    expect(dayKey(morning)).toBe('2026-03-15');
  });

  it('names today and yesterday and dates anything older', () => {
    const labels = { today: 'Today', yesterday: 'Yesterday' };
    const today = new Date(2026, 2, 15, 9, 0).getTime();
    const yesterday = new Date(2026, 2, 14, 9, 0).getTime();
    const older = new Date(2026, 2, 10, 9, 0).getTime();

    expect(formatDayHeading(today, NOW, 'en', labels)).toBe('Today');
    expect(formatDayHeading(yesterday, NOW, 'en', labels)).toBe('Yesterday');
    const dated = formatDayHeading(older, NOW, 'en', labels);
    expect(dated).not.toBe('Today');
    expect(dated).not.toBe('Yesterday');
    expect(dated).toMatch(/2026/);
  });

  it('crosses a month boundary correctly', () => {
    const firstOfMarch = new Date(2026, 2, 1, 10, 0).getTime();
    const lastOfFebruary = new Date(2026, 1, 28, 10, 0).getTime();
    const labels = { today: 'Today', yesterday: 'Yesterday' };
    expect(formatDayHeading(firstOfMarch, NOW, 'en', labels)).not.toBe('Yesterday');
    expect(formatDayHeading(lastOfFebruary, NOW, 'en', labels)).not.toBe('Yesterday');
  });
});

describe('byteUnit', () => {
  it('keeps bytes whole and everything above them to one decimal', () => {
    expect(byteUnit(0)).toEqual({ unit: 'byte', digits: 0, scaled: 0 });
    expect(byteUnit(1023)).toEqual({ unit: 'byte', digits: 0, scaled: 1023 });
  });

  it('steps up a unit exactly at the boundary', () => {
    // The boundary is the whole point: a log directory of 1024 bytes is "1 kB", not "1024 B".
    expect(byteUnit(1024)).toEqual({ unit: 'kilobyte', digits: 1, scaled: 1 });
    expect(byteUnit(1536)).toEqual({ unit: 'kilobyte', digits: 1, scaled: 1.5 });
    expect(byteUnit(1024 * 1024)).toEqual({ unit: 'megabyte', digits: 1, scaled: 1 });
    expect(byteUnit(3 * 1024 * 1024 * 1024)).toEqual({
      unit: 'gigabyte',
      digits: 1,
      scaled: 3,
    });
  });

  it('treats a negative size as nothing rather than inventing a unit', () => {
    expect(byteUnit(-5)).toEqual({ unit: 'byte', digits: 0, scaled: 0 });
  });
});
