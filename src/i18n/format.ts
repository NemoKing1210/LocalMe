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

export function formatClockTime(timestampMs: number, locale: Locale): string {
  return timeFormatter(locale).format(new Date(timestampMs));
}

export function formatDateTime(timestampMs: number, locale: Locale): string {
  return dateTimeFormatter(locale).format(new Date(timestampMs));
}

export function dayKey(timestampMs: number): string {
  const date = new Date(timestampMs);
  const month = `${date.getMonth() + 1}`.padStart(2, '0');
  const day = `${date.getDate()}`.padStart(2, '0');
  return `${date.getFullYear()}-${month}-${day}`;
}

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

export function byteUnit(value: number): {
  readonly unit: 'byte' | 'kilobyte' | 'megabyte' | 'gigabyte';
  readonly digits: number;
  readonly scaled: number;
} {
  let unit: 'byte' | 'kilobyte' | 'megabyte' | 'gigabyte' = 'byte';
  let digits = 0;
  let scaled = Math.max(0, value);
  if (scaled >= 1024) {
    unit = 'kilobyte';
    digits = 1;
    scaled /= 1024;
  }
  if (scaled >= 1024) {
    unit = 'megabyte';
    scaled /= 1024;
  }
  if (scaled >= 1024) {
    unit = 'gigabyte';
    scaled /= 1024;
  }
  return { unit, digits, scaled };
}

export function formatBytes(value: number, locale: Locale): string {
  const { unit, digits, scaled } = byteUnit(value);
  return new Intl.NumberFormat(locale, {
    style: 'unit',
    unit,
    maximumFractionDigits: digits,
  }).format(scaled);
}

export function formatDays(count: number, locale: Locale): string {
  return new Intl.NumberFormat(locale, { style: 'unit', unit: 'day' }).format(count);
}
