import {
  Hct,
  MaterialDynamicColors,
  SchemeTonalSpot,
  argbFromHex,
  hexFromArgb,
  type DynamicColor,
  type DynamicScheme,
} from '@material/material-color-utilities';

const dynamicColors = new MaterialDynamicColors();

const ROLE_COLORS = {
  primary: dynamicColors.primary(),
  'on-primary': dynamicColors.onPrimary(),
  'primary-container': dynamicColors.primaryContainer(),
  'on-primary-container': dynamicColors.onPrimaryContainer(),
  'primary-fixed': dynamicColors.primaryFixed(),
  'primary-fixed-dim': dynamicColors.primaryFixedDim(),
  'on-primary-fixed': dynamicColors.onPrimaryFixed(),
  'on-primary-fixed-variant': dynamicColors.onPrimaryFixedVariant(),
  'inverse-primary': dynamicColors.inversePrimary(),

  secondary: dynamicColors.secondary(),
  'on-secondary': dynamicColors.onSecondary(),
  'secondary-container': dynamicColors.secondaryContainer(),
  'on-secondary-container': dynamicColors.onSecondaryContainer(),

  tertiary: dynamicColors.tertiary(),
  'on-tertiary': dynamicColors.onTertiary(),
  'tertiary-container': dynamicColors.tertiaryContainer(),
  'on-tertiary-container': dynamicColors.onTertiaryContainer(),

  error: dynamicColors.error(),
  'on-error': dynamicColors.onError(),
  'error-container': dynamicColors.errorContainer(),
  'on-error-container': dynamicColors.onErrorContainer(),

  background: dynamicColors.background(),
  'on-background': dynamicColors.onBackground(),

  surface: dynamicColors.surface(),
  'on-surface': dynamicColors.onSurface(),
  'surface-dim': dynamicColors.surfaceDim(),
  'surface-bright': dynamicColors.surfaceBright(),
  'surface-container-lowest': dynamicColors.surfaceContainerLowest(),
  'surface-container-low': dynamicColors.surfaceContainerLow(),
  'surface-container': dynamicColors.surfaceContainer(),
  'surface-container-high': dynamicColors.surfaceContainerHigh(),
  'surface-container-highest': dynamicColors.surfaceContainerHighest(),
  'on-surface-variant': dynamicColors.onSurfaceVariant(),
  'surface-variant': dynamicColors.surfaceVariant(),
  'inverse-surface': dynamicColors.inverseSurface(),
  'inverse-on-surface': dynamicColors.inverseOnSurface(),

  outline: dynamicColors.outline(),
  'outline-variant': dynamicColors.outlineVariant(),
  shadow: dynamicColors.shadow(),
  scrim: dynamicColors.scrim(),
} as const satisfies Record<string, DynamicColor>;

export type ColorRole = keyof typeof ROLE_COLORS;

export type ColorScheme = Readonly<Record<ColorRole, string>>;

/** The accent every other accent is compared against, and the one shown before a choice. */
export const DEFAULT_ACCENT = '#6750A4';

export const ACCENT_PRESETS: readonly { readonly id: string; readonly hex: string }[] = [
  { id: 'baseline', hex: '#6750A4' },
  { id: 'indigo', hex: '#4355B9' },
  { id: 'teal', hex: '#006A6A' },
  { id: 'green', hex: '#3F6838' },
  { id: 'amber', hex: '#7A5900' },
  { id: 'orange', hex: '#8B5000' },
  { id: 'red', hex: '#B3261E' },
  { id: 'pink', hex: '#8E4585' },
  { id: 'slate', hex: '#5A5D72' },
];

/**
 * Builds the full role palette for one theme from a source colour.
 *
 * Contrast level is fixed at 0 (the standard level): the "high contrast" levels change the
 * tone of every role by a fixed delta and are a system accessibility preference, not an
 * appearance setting, so exposing them here would be a setting nobody can evaluate.
 */
export function buildColorScheme(sourceHex: string, isDark: boolean): ColorScheme {
  const source = Hct.fromInt(argbFromHex(normalizeHex(sourceHex)));
  const scheme: DynamicScheme = new SchemeTonalSpot(source, isDark, 0);

  const resolved = {} as Record<ColorRole, string>;
  for (const [role, color] of Object.entries(ROLE_COLORS) as [ColorRole, DynamicColor][]) {
    resolved[role] = hexFromArgb(color.getArgb(scheme));
  }
  return resolved;
}

export function normalizeHex(value: string): string {
  const trimmed = value.trim();
  const candidate = trimmed.startsWith('#') ? trimmed : `#${trimmed}`;
  return /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/.test(candidate) ? candidate : DEFAULT_ACCENT;
}

export function applyColorScheme(element: HTMLElement, scheme: ColorScheme): void {
  for (const [role, color] of Object.entries(scheme) as [ColorRole, string][]) {
    element.style.setProperty(`--md-sys-color-${role}`, color);
  }
}
