/**
 * Locale-aware formatting.
 *
 * Everything user-visible that depends on a locale goes through `Intl`. Hand-written
 * "5 минуты назад" strings are the classic source of wrong grammar in exactly the language
 * this application ships beside English, and `Intl.RelativeTimeFormat` already implements
 * the CLDR rules for both.
 *
 * Formatters are memoised per locale: constructing an `Intl.DateTimeFormat` is not cheap and
 * a virtualised message list formats a timestamp per visible row on every scroll.
 */
import type { Locale } from './locales';

const relativeFormatters = new Map<Locale, Intl.RelativeTimeFormat>();
const timeFormatters = new Map<Locale, Intl.DateTimeFormat>();
const dateFormatters = new Map<Locale, Intl.DateTimeFormat>();
const dateTimeFormatters = new Map<Locale, Intl.DateTimeFormat>();
const pluralRules = new Map<Locale, Intl.PluralRules>();

const SECOND = 1_000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
const MONTH = 30 * DAY;
const YEAR = 365 * DAY;

/** Below this age a relative time is expressed in seconds, and `numeric: 'auto'` says "now". */
const SECONDS_THRESHOLD = 45 * SECOND;

function relativeFormatter(locale: Locale): Intl.RelativeTimeFormat {
  const cached = relativeFormatters.get(locale);
  if (cached) return cached;
  const created = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
  relativeFormatters.set(locale, created);
  return created;
}

export function pluralRulesFor(locale: Locale): Intl.PluralRules {
  const cached = pluralRules.get(locale);
  if (cached) return cached;
  const created = new Intl.PluralRules(locale);
  pluralRules.set(locale, created);
  return created;
}

function timeFormatter(locale: Locale): Intl.DateTimeFormat {
  const cached = timeFormatters.get(locale);
  if (cached) return cached;
  const created = new Intl.DateTimeFormat(locale, { timeStyle: 'short' });
  timeFormatters.set(locale, created);
  return created;
}

function dateFormatter(locale: Locale): Intl.DateTimeFormat {
  const cached = dateFormatters.get(locale);
  if (cached) return cached;
  const created = new Intl.DateTimeFormat(locale, { dateStyle: 'medium' });
  dateFormatters.set(locale, created);
  return created;
}

function dateTimeFormatter(locale: Locale): Intl.DateTimeFormat {
  const cached = dateTimeFormatters.get(locale);
  if (cached) return cached;
  const created = new Intl.DateTimeFormat(locale, {
    dateStyle: 'short',
    timeStyle: 'short',
  });
  dateTimeFormatters.set(locale, created);
  return created;
}

/**
 * Chooses the largest unit that keeps the number small, then lets `Intl` phrase it.
 *
 * "45 seconds ago" is the last second-based reading; from there the unit steps up to
 * minutes, hours, days, months, years. Millions of seconds are never shown.
 */
export function formatRelativeTime(timestampMs: number, nowMs: number, locale: Locale): string {
  const delta = timestampMs - nowMs;
  const magnitude = Math.abs(delta);
  const formatter = relativeFormatter(locale);

  if (magnitude < SECONDS_THRESHOLD) {
    return formatter.format(Math.round(delta / SECOND), 'second');
  }
  if (magnitude < HOUR) {
    return formatter.format(Math.round(delta / MINUTE), 'minute');
  }
  if (magnitude < DAY) {
    return formatter.format(Math.round(delta / HOUR), 'hour');
  }
  if (magnitude < MONTH) {
    return formatter.format(Math.round(delta / DAY), 'day');
  }
  if (magnitude < YEAR) {
    return formatter.format(Math.round(delta / MONTH), 'month');
  }
  return formatter.format(Math.round(delta / YEAR), 'year');
}

/** Clock time of a message, e.g. "14:05" or "2:05 PM". */
export function formatClockTime(timestampMs: number, locale: Locale): string {
  return timeFormatter(locale).format(new Date(timestampMs));
}

/** Full date and time, used by the device list and tooltips. */
export function formatDateTime(timestampMs: number, locale: Locale): string {
  return dateTimeFormatter(locale).format(new Date(timestampMs));
}

/** Calendar day of a timestamp, in local time. */
export function dayKey(timestampMs: number): string {
  const date = new Date(timestampMs);
  const month = `${date.getMonth() + 1}`.padStart(2, '0');
  const day = `${date.getDate()}`.padStart(2, '0');
  return `${date.getFullYear()}-${month}-${day}`;
}

/**
 * Heading for a day separator: "Today", "Yesterday", or a localised date.
 *
 * `today` and `yesterday` are passed in rather than looked up here so this function stays
 * free of the message catalogue.
 */
export function formatDayHeading(
  timestampMs: number,
  nowMs: number,
  locale: Locale,
  labels: { today: string; yesterday: string },
): string {
  const key = dayKey(timestampMs);
  if (key === dayKey(nowMs)) return labels.today;

  const yesterday = new Date(nowMs);
  yesterday.setDate(yesterday.getDate() - 1);
  if (key === dayKey(yesterday.getTime())) return labels.yesterday;

  return dateFormatter(locale).format(new Date(timestampMs));
}
