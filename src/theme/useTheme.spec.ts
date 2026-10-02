// @vitest-environment happy-dom
import { mount, type VueWrapper } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defineComponent, nextTick } from 'vue';

import { setMediaMatches } from '@/test/matchMedia';

import { DEFAULT_ACCENT, buildColorScheme } from './palette';
import type * as ThemeNamespace from './useTheme';

const DARK_QUERY = '(prefers-color-scheme: dark)';
const REDUCED_MOTION_QUERY = '(prefers-reduced-motion: reduce)';

type ThemeModule = typeof ThemeNamespace;

/**
 * `useTheme` keeps its state in module scope, so a fresh import is the only honest reset. The
 * media-query listeners an earlier module registered are cleared by the harness between tests.
 */
async function loadThemeModule(): Promise<ThemeModule> {
  vi.resetModules();
  return import('./useTheme');
}

async function initAndFlush(theme: ThemeModule): Promise<void> {
  theme.initTheme();
  await nextTick();
}

function rootTheme(): string | undefined {
  return document.documentElement.dataset['theme'];
}

function rootProperty(name: string): string {
  return document.documentElement.style.getPropertyValue(name);
}

beforeEach(() => {
  delete document.documentElement.dataset['theme'];
  document.documentElement.removeAttribute('style');
});

describe('initTheme', () => {
  it('paints the dark scheme when the system prefers dark at call time', async () => {
    setMediaMatches(DARK_QUERY, true);
    const theme = await loadThemeModule();

    await initAndFlush(theme);

    expect(theme.useTheme().effective.value).toBe('dark');
    expect(rootTheme()).toBe('dark');
    expect(rootProperty('--md-sys-color-primary')).toBe(
      buildColorScheme(DEFAULT_ACCENT, true).primary,
    );
  });

  it('paints the light scheme when the system does not prefer dark', async () => {
    const theme = await loadThemeModule();

    await initAndFlush(theme);

    expect(theme.useTheme().effective.value).toBe('light');
    expect(rootTheme()).toBe('light');
    expect(rootProperty('--md-sys-color-primary')).toBe(
      buildColorScheme(DEFAULT_ACCENT, false).primary,
    );
  });

  it('reads the reduced-motion preference', async () => {
    setMediaMatches(REDUCED_MOTION_QUERY, true);
    const theme = await loadThemeModule();

    await initAndFlush(theme);

    expect(theme.useTheme().reducedMotion.value).toBe(true);

    setMediaMatches(REDUCED_MOTION_QUERY, false);
    const other = await loadThemeModule();
    await initAndFlush(other);
    expect(other.useTheme().reducedMotion.value).toBe(false);
  });

  it('writes the whole scheme, not only the accent', async () => {
    const theme = await loadThemeModule();
    const dark = buildColorScheme(DEFAULT_ACCENT, true);
    setMediaMatches(DARK_QUERY, true);
    await initAndFlush(theme);

    for (const [role, color] of Object.entries(dark)) {
      expect(rootProperty(`--md-sys-color-${role}`)).toBe(color);
    }
  });
});

describe('setThemeMode', () => {
  it('overrides a dark system preference with light', async () => {
    setMediaMatches(DARK_QUERY, true);
    const theme = await loadThemeModule();
    await initAndFlush(theme);

    theme.setThemeMode('light');
    await nextTick();

    expect(theme.useTheme().effective.value).toBe('light');
    expect(rootTheme()).toBe('light');
    expect(rootProperty('--md-sys-color-primary')).toBe(
      buildColorScheme(DEFAULT_ACCENT, false).primary,
    );
  });

  it('overrides a light system preference with dark', async () => {
    const theme = await loadThemeModule();
    await initAndFlush(theme);

    theme.setThemeMode('dark');
    await nextTick();

    expect(theme.useTheme().effective.value).toBe('dark');
    expect(rootTheme()).toBe('dark');
  });

  it('returns to following the system preference on system', async () => {
    setMediaMatches(DARK_QUERY, true);
    const theme = await loadThemeModule();
    await initAndFlush(theme);

    theme.setThemeMode('light');
    await nextTick();
    expect(theme.useTheme().effective.value).toBe('light');

    theme.setThemeMode('system');
    await nextTick();

    expect(theme.useTheme().effective.value).toBe('dark');
    expect(rootTheme()).toBe('dark');
  });
});

describe('setAccentColor', () => {
  it('normalises before storing', async () => {
    const theme = await loadThemeModule();
    await initAndFlush(theme);

    theme.setAccentColor('6750a4');
    await nextTick();

    expect(theme.useTheme().accent.value).toBe('#6750a4');
    expect(theme.useTheme().scheme.value.primary).toBe(buildColorScheme('#6750a4', false).primary);
    expect(rootProperty('--md-sys-color-primary')).toBe(buildColorScheme('#6750a4', false).primary);
  });

  it('falls back to the default accent for malformed input', async () => {
    const theme = await loadThemeModule();
    await initAndFlush(theme);

    theme.setAccentColor('not-a-colour');
    await nextTick();

    expect(theme.useTheme().accent.value).toBe(DEFAULT_ACCENT);
  });

  it('recomputes the scheme when the mode and the accent both change', async () => {
    const theme = await loadThemeModule();
    await initAndFlush(theme);

    theme.setThemeMode('dark');
    theme.setAccentColor('#B3261E');
    await nextTick();

    expect(theme.useTheme().scheme.value.primary).toBe(buildColorScheme('#B3261E', true).primary);
    expect(rootProperty('--md-sys-color-primary')).toBe(buildColorScheme('#B3261E', true).primary);
  });
});

describe('media change events', () => {
  it('flips the effective theme while the mode follows the system', async () => {
    const theme = await loadThemeModule();
    await initAndFlush(theme);
    expect(theme.useTheme().effective.value).toBe('light');

    setMediaMatches(DARK_QUERY, true);
    await nextTick();
    expect(theme.useTheme().effective.value).toBe('dark');
    expect(rootTheme()).toBe('dark');

    setMediaMatches(DARK_QUERY, false);
    await nextTick();
    expect(theme.useTheme().effective.value).toBe('light');
    expect(rootTheme()).toBe('light');
  });

  it('ignores the change while an explicit mode is set', async () => {
    const theme = await loadThemeModule();
    await initAndFlush(theme);

    theme.setThemeMode('dark');
    await nextTick();

    setMediaMatches(DARK_QUERY, false);
    await nextTick();

    expect(theme.useTheme().effective.value).toBe('dark');
    expect(rootTheme()).toBe('dark');
  });
});

describe('useTheme in a component', () => {
  function mountProbe(theme: ThemeModule): VueWrapper {
    const Probe = defineComponent({
      setup() {
        const { effective, accent } = theme.useTheme();
        return { effective, accent };
      },
      template: `
        <p data-testid="effective">{{ effective }}</p>
        <p data-testid="accent">{{ accent }}</p>
      `,
    });
    return mount(Probe);
  }

  it('sees the shared state and reflects later changes', async () => {
    setMediaMatches(DARK_QUERY, true);
    const theme = await loadThemeModule();
    await initAndFlush(theme);
    const wrapper = mountProbe(theme);

    expect(wrapper.get('[data-testid="effective"]').text()).toBe('dark');
    expect(wrapper.get('[data-testid="accent"]').text()).toBe(DEFAULT_ACCENT);

    theme.setThemeMode('light');
    theme.setAccentColor('#006A6A');
    await nextTick();

    expect(wrapper.get('[data-testid="effective"]').text()).toBe('light');
    expect(wrapper.get('[data-testid="accent"]').text()).toBe('#006A6A');
    expect(rootProperty('--md-sys-color-primary')).toBe(buildColorScheme('#006A6A', false).primary);
  });

  it('routes the component setter through the same state', async () => {
    const theme = await loadThemeModule();
    await initAndFlush(theme);
    const wrapper = mountProbe(theme);

    theme.useTheme().setMode('dark');
    await nextTick();

    expect(wrapper.get('[data-testid="effective"]').text()).toBe('dark');
    expect(rootTheme()).toBe('dark');

    theme.useTheme().setAccent('#8B5000');
    await nextTick();

    expect(wrapper.get('[data-testid="accent"]').text()).toBe('#8B5000');
  });
});
