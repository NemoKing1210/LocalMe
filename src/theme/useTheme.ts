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
let hasPainted = false;

const effectiveTheme: ComputedRef<EffectiveTheme> = computed(() => {
  if (mode.value === 'system') return systemPrefersDark.value ? 'dark' : 'light';
  return mode.value;
});

const paletteCache = new Map<string, ColorScheme>();

function schemeFor(source: string, isDark: boolean): ColorScheme {
  const key = `${source}|${isDark ? 'dark' : 'light'}`;
  const cached = paletteCache.get(key);
  if (cached) return cached;
  const built = buildColorScheme(source, isDark);
  paletteCache.set(key, built);
  return built;
}

const scheme: ComputedRef<ColorScheme> = computed(() =>
  schemeFor(accent.value, effectiveTheme.value === 'dark'),
);

function paint(): void {
  const root = document.documentElement;
  const apply = (): void => {
    // Order matters: the `data-theme` attribute selects the fallbacks, then the generated
    // custom properties are written as inline styles, which win over them.
    root.dataset['theme'] = effectiveTheme.value;
    applyColorScheme(root, scheme.value);
  };

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

export function initTheme(): void {
  watchMedia(DARK_QUERY, systemPrefersDark);
  watchMedia(REDUCED_MOTION_QUERY, reducedMotion);
  watch([effectiveTheme, accent], paint, { immediate: true });
}

/** Sets the theme mode. Persisting it is the settings store's job. */
export function setThemeMode(next: ThemeMode): void {
  mode.value = next;
}

export function setAccentColor(next: string): void {
  accent.value = normalizeHex(next);
}

export interface UseTheme {
  readonly mode: Readonly<Ref<ThemeMode>>;
  readonly effective: ComputedRef<EffectiveTheme>;
  readonly accent: Readonly<Ref<string>>;
  readonly scheme: ComputedRef<ColorScheme>;
  readonly presets: readonly { readonly id: string; readonly hex: string }[];
  readonly reducedMotion: Readonly<Ref<boolean>>;
  readonly setMode: (mode: ThemeMode) => void;
  readonly setAccent: (hex: string) => void;
}

export function useTheme(): UseTheme {
  return {
    mode: readonly(mode),
    effective: effectiveTheme,
    accent: readonly(accent),
    scheme,
    presets: ACCENT_PRESETS,
    reducedMotion: readonly(reducedMotion),
    setMode: setThemeMode,
    setAccent: setAccentColor,
  };
}
