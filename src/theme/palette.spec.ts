import { describe, expect, it } from 'vitest';

import {
  ACCENT_PRESETS,
  DEFAULT_ACCENT,
  applyColorScheme,
  buildColorScheme,
  normalizeHex,
  type ColorRole,
  type ColorScheme,
} from './palette';

/** Source colours spread across the hue circle, so a scheme built from one is not special. */
const SOURCES = [DEFAULT_ACCENT, '#B3261E', '#006A6A', '#7A5900'];

/** Each `on-*` role with the surface it is drawn on. Material defines the pairing; the module
 * only maps roles to generated colours, so the test owns the list it verifies. */
const CONTRAST_PAIRS: readonly (readonly [ColorRole, ColorRole])[] = [
  ['on-primary', 'primary'],
  ['on-primary-container', 'primary-container'],
  ['on-primary-fixed', 'primary-fixed'],
  ['on-primary-fixed-variant', 'primary-fixed'],
  ['inverse-primary', 'inverse-surface'],
  ['on-secondary', 'secondary'],
  ['on-secondary-container', 'secondary-container'],
  ['on-tertiary', 'tertiary'],
  ['on-tertiary-container', 'tertiary-container'],
  ['on-error', 'error'],
  ['on-error-container', 'error-container'],
  ['on-background', 'background'],
  ['on-surface', 'surface'],
  ['on-surface-variant', 'surface-variant'],
  ['inverse-on-surface', 'inverse-surface'],
];

function luminance(hex: string): number {
  const body = hex.slice(1);
  const [r, g, b] = [0, 2, 4].map((offset) => {
    const value = parseInt(body.slice(offset, offset + 2), 16) / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * (r ?? 0) + 0.7152 * (g ?? 0) + 0.0722 * (b ?? 0);
}

function contrastRatio(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return ((hi ?? 0) + 0.05) / ((lo ?? 0) + 0.05);
}

interface FakeElement {
  readonly element: HTMLElement;
  readonly properties: Record<string, string>;
}

/** A node that records what `applyColorScheme` sets without needing a DOM; the function only
 * reaches into `element.style.setProperty`. */
function makeElement(): FakeElement {
  const properties: Record<string, string> = {};
  const element = {
    style: {
      setProperty(name: string, value: string): void {
        properties[name] = value;
      },
    },
  } as unknown as HTMLElement;
  return { element, properties };
}

describe('normalizeHex', () => {
  it('accepts a six-digit colour with or without the hash', () => {
    expect(normalizeHex('#6750A4')).toBe('#6750A4');
    expect(normalizeHex('6750A4')).toBe('#6750A4');
  });

  it('accepts lower case and keeps the digits as typed', () => {
    expect(normalizeHex('#6750a4')).toBe('#6750a4');
    expect(normalizeHex('abcdef')).toBe('#abcdef');
  });

  it('accepts the three-digit shorthand', () => {
    expect(normalizeHex('#abc')).toBe('#abc');
    expect(normalizeHex('ABC')).toBe('#ABC');
  });

  it('trims surrounding whitespace before deciding', () => {
    expect(normalizeHex('  #6750A4  ')).toBe('#6750A4');
    expect(normalizeHex('\t6750a4\n')).toBe('#6750a4');
  });

  it('falls back to the default accent for malformed input', () => {
    const malformed = ['', '#', 'nope', '#12345', '#1234567', '#GGGGGG', 'rgb(1,2,3)', '-12345'];
    for (const value of malformed) {
      expect(normalizeHex(value), value).toBe(DEFAULT_ACCENT);
    }
  });

  it('treats a leading hash only, so an embedded one is malformed', () => {
    expect(normalizeHex('#6750#4')).toBe(DEFAULT_ACCENT);
  });
});

describe('ACCENT_PRESETS', () => {
  it('contains only values that normalise to themselves', () => {
    expect(ACCENT_PRESETS.length).toBeGreaterThan(0);
    for (const preset of ACCENT_PRESETS) {
      expect(normalizeHex(preset.hex), preset.id).toBe(preset.hex);
      expect(preset.hex, preset.id).toMatch(/^#[0-9a-fA-F]{6}$/);
    }
  });

  it('has unique ids and includes the baseline accent', () => {
    const ids = ACCENT_PRESETS.map((preset) => preset.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ACCENT_PRESETS.some((preset) => preset.hex === DEFAULT_ACCENT)).toBe(true);
  });
});

describe('buildColorScheme', () => {
  it('returns a valid six-digit colour for every role', () => {
    for (const source of SOURCES) {
      for (const isDark of [false, true]) {
        const scheme = buildColorScheme(source, isDark);
        const roles = Object.keys(scheme) as ColorRole[];
        expect(roles.length).toBeGreaterThan(20);
        for (const role of roles) {
          expect(scheme[role], `${source} ${isDark ? 'dark' : 'light'} ${role}`).toMatch(
            /^#[0-9a-fA-F]{6}$/,
          );
        }
      }
    }
  });

  it('is deterministic for the same source and theme', () => {
    expect(buildColorScheme('#6750A4', false)).toEqual(buildColorScheme('#6750A4', false));
    expect(buildColorScheme('#6750A4', true)).toEqual(buildColorScheme('6750A4', true));
  });

  it('produces a light and a dark scheme that differ', () => {
    const light = buildColorScheme(DEFAULT_ACCENT, false);
    const dark = buildColorScheme(DEFAULT_ACCENT, true);
    expect(light).not.toEqual(dark);
    expect(light.primary).not.toBe(dark.primary);
    expect(light.background).not.toBe(dark.background);
    // The light scheme is the lighter one: its background is brighter than its text.
    expect(luminance(light.background)).toBeGreaterThan(luminance(light['on-background']));
    expect(luminance(dark.background)).toBeLessThan(luminance(dark['on-background']));
  });

  it('gives every on- role enough contrast with its base', () => {
    for (const isDark of [false, true]) {
      const scheme = buildColorScheme(DEFAULT_ACCENT, isDark);
      const label = isDark ? 'dark' : 'light';
      for (const [on, base] of CONTRAST_PAIRS) {
        expect(
          contrastRatio(scheme[on], scheme[base]),
          `${label} ${on} / ${base}`,
        ).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it('normalises the source before building', () => {
    expect(buildColorScheme('6750a4', false)).toEqual(buildColorScheme('#6750a4', false));
    // Malformed input normalises to the default rather than throwing or producing an invalid argb.
    expect(buildColorScheme('not-a-colour', false)).toEqual(
      buildColorScheme(DEFAULT_ACCENT, false),
    );
  });
});

describe('applyColorScheme', () => {
  it('writes each role as a documented custom property', () => {
    const { element, properties } = makeElement();
    const scheme = buildColorScheme(DEFAULT_ACCENT, false);

    applyColorScheme(element, scheme);

    for (const [role, color] of Object.entries(scheme) as [ColorRole, string][]) {
      expect(properties[`--md-sys-color-${role}`]).toBe(color);
    }
    expect(Object.keys(properties).length).toBe(Object.keys(scheme).length);
  });

  it('overwrites a previously applied scheme', () => {
    const { element, properties } = makeElement();
    const first = buildColorScheme(DEFAULT_ACCENT, false);
    const second = buildColorScheme('#B3261E', false);
    expect(first.primary).not.toBe(second.primary);

    applyColorScheme(element, first);
    applyColorScheme(element, second);

    expect(properties['--md-sys-color-primary']).toBe(second.primary);
    expect(Object.keys(properties).length).toBe(Object.keys(second).length);
  });

  it('accepts a partial scheme and only sets the given roles', () => {
    const { element, properties } = makeElement();
    const partial = { primary: '#123456' } as ColorScheme;

    applyColorScheme(element, partial);

    expect(properties['--md-sys-color-primary']).toBe('#123456');
    expect(Object.keys(properties).length).toBe(1);
  });
});
