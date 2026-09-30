/**
 * Theme state and application.
 *
 * The theme is a small piece of global state with three inputs — the chosen mode, the chosen
 * accent, and the operating system's own preferences — and one output: the CSS custom
 * properties on `:root`. It lives in a module rather than a Pinia store because the settings
 * store *feeds* it (it is the persisted source of the mode and accent) and a store depending
 * on itself would be circular; here the dependency runs one way.
 *
 * Everything is event-driven. `matchMedia` listeners fire on change; there is no polling and
 * no timer.
 */
import { computed, readonly, ref, watch, type ComputedRef, type Ref } from 'vue';

import {
  ACCENT_PRESETS,
  DEFAULT_ACCENT,
  applyColorScheme,
  buildColorScheme,
  normalizeHex,
  type ColorScheme,
} from './palette';

export type ThemeMode = 'system' | 'light' | 'dark';
export type EffectiveTheme = 'light' | 'dark';

export const THEME_MODES: readonly ThemeMode[] = ['system', 'light', 'dark'];

const DARK_QUERY = '(prefers-color-scheme: dark)';
const REDUCED_MOTION_QUERY = '(prefers-reduced-motion: reduce)';

const mode = ref<ThemeMode>('system');
const accent = ref<string>(DEFAULT_ACCENT);
const systemPrefersDark = ref(false);
const reducedMotion = ref(false);
/** Whether a frame has been painted already, i.e. whether a change has something to fade from. */
let hasPainted = false;

const effectiveTheme: ComputedRef<EffectiveTheme> = computed(() => {
  if (mode.value === 'system') return systemPrefersDark.value ? 'dark' : 'light';
  return mode.value;
});

/** Palettes are pure functions of (accent, isDark); two entries is the whole cache. */
const paletteCache = new Map<string, ColorScheme>();

function schemeFor(source: string, isDark: boolean): ColorScheme {
  const key = `${source}|${isDark ? 'dark' : 'light'}`;
  const cached = paletteCache.get(key);
  if (cached) return cached;
  const built = buildColorScheme(source, isDark);
  paletteCache.set(key, built);
  return built;
}

function paint(): void {
  const root = document.documentElement;
  const apply = (): void => {
    // Order matters: the `data-theme` attribute selects the fallbacks, then the generated
    // custom properties are written as inline styles, which win over them.
    root.dataset['theme'] = effectiveTheme.value;
    applyColorScheme(root, schemeFor(accent.value, effectiveTheme.value === 'dark'));
  };

  // Changing the theme or the accent repaints every surface in the window at once. The View
  // Transitions API cross-fades the two states, which is what it is for; it is a no-op on an
  // engine without it, and the first paint is not a transition because there is nothing to
  // cross-fade from.
  const canTransition = hasPainted && !reducedMotion.value && 'startViewTransition' in document;
  if (canTransition) {
    document.startViewTransition(apply);
  } else {
    apply();
  }
  hasPainted = true;
}

function watchMedia(query: string, target: Ref<boolean>): void {
  const media = window.matchMedia(query);
  target.value = media.matches;
  media.addEventListener('change', (event) => {
    target.value = event.matches;
  });
}

/** Installs the media-query listeners and paints the first frame. */
export function initTheme(): void {
  watchMedia(DARK_QUERY, systemPrefersDark);
  watchMedia(REDUCED_MOTION_QUERY, reducedMotion);
  watch([effectiveTheme, accent], paint, { immediate: true });
}

/** Sets the theme mode. Persisting it is the settings store's job. */
export function setThemeMode(next: ThemeMode): void {
  mode.value = next;
}

/** Sets the accent colour, falling back to the default for anything unparseable. */
export function setAccentColor(next: string): void {
  accent.value = normalizeHex(next);
}

/** The composition-API view of the theme. */
export interface UseTheme {
  /** The user's choice: system, light or dark. */
  readonly mode: Readonly<Ref<ThemeMode>>;
  /** The mode actually in effect, after the system preference is applied. */
  readonly effective: ComputedRef<EffectiveTheme>;
  /** The accent colour the palette is generated from. */
  readonly accent: Readonly<Ref<string>>;
  /** Curated accents offered in settings. */
  readonly presets: readonly { readonly id: string; readonly hex: string }[];
  /** Whether the operating system asks for reduced motion. */
  readonly reducedMotion: Readonly<Ref<boolean>>;
  /** Sets the theme mode; persisting it is the settings store's job. */
  readonly setMode: (mode: ThemeMode) => void;
  /** Sets the accent colour; persisting it is the settings store's job. */
  readonly setAccent: (hex: string) => void;
}

/** The composition-API view of the theme. */
export function useTheme(): UseTheme {
  return {
    mode: readonly(mode),
    effective: effectiveTheme,
    accent: readonly(accent),
    presets: ACCENT_PRESETS,
    reducedMotion: readonly(reducedMotion),
    setMode: setThemeMode,
    setAccent: setAccentColor,
  };
}
