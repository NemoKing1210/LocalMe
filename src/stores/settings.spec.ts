import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useI18n } from '@/i18n';
import type { Settings } from '@/ipc';
import { makeSettings } from '@/test/factories';

// The store's job is to push the document into the theme singleton; the singleton itself paints
// the document and is covered by its own happy-dom spec, so this node-environment spec substitutes
// a stateful double and checks that the push happened, as a change of state rather than a call.
const theme = vi.hoisted(() => {
  const state = { mode: 'system', accent: '#6750A4' };
  return {
    state,
    setThemeMode: (mode: string): void => {
      state.mode = mode;
    },
    setAccentColor: (accent: string): void => {
      state.accent = accent;
    },
  };
});

vi.mock('@/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/ipc')>()),
  getSettings: vi.fn(),
  updateSettings: vi.fn(),
}));

vi.mock('@/theme/useTheme', () => ({
  setThemeMode: theme.setThemeMode,
  setAccentColor: theme.setAccentColor,
}));

import * as ipc from '@/ipc';

import { useSettingsStore } from './settings';

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((innerResolve) => {
    resolve = innerResolve;
  });
  return { promise, resolve };
}

beforeEach(() => {
  setActivePinia(createPinia());
  theme.state.mode = 'system';
  theme.state.accent = '#6750A4';
  vi.mocked(ipc.getSettings).mockReset();
  vi.mocked(ipc.updateSettings).mockReset();
});

describe('the settings store', () => {
  it('exposes the documented defaults before any settings load', () => {
    const settings = useSettingsStore();

    expect(settings.document).toBeNull();
    expect(settings.theme).toBe('system');
    expect(settings.accent).toBe('#6750A4');
    expect(settings.locale).toBe('en');
    expect(settings.notifications).toBeNull();
    expect(settings.system).toBeNull();
    expect(settings.onboarded).toBe(false);
  });

  it('stores the document and pushes locale, theme and accent into the live modules', () => {
    const settings = useSettingsStore();

    settings.apply(
      makeSettings({
        appearance: { theme: 'dark', accent: '#ABCDEF' },
        locale: 'ru',
        onboarded: true,
      }),
    );

    expect(settings.document?.locale).toBe('ru');
    expect(settings.theme).toBe('dark');
    expect(settings.accent).toBe('#ABCDEF');
    expect(settings.onboarded).toBe(true);

    expect(useI18n().locale.value).toBe('ru');
    expect(theme.state.mode).toBe('dark');
    expect(theme.state.accent).toBe('#ABCDEF');
  });

  it('loads the settings from the host and applies them', async () => {
    const settings = useSettingsStore();
    vi.mocked(ipc.getSettings).mockResolvedValue(makeSettings({ locale: 'de' }));

    await settings.load();

    expect(ipc.getSettings).toHaveBeenCalledTimes(1);
    expect(settings.document?.locale).toBe('de');
    expect(useI18n().locale.value).toBe('de');
  });

  it('sends the patched document and keeps `saving` true while the host works', async () => {
    const settings = useSettingsStore();
    settings.apply(makeSettings({ locale: 'en' }));

    const request = deferred<Settings>();
    vi.mocked(ipc.updateSettings).mockReturnValue(request.promise);

    const patch = (current: Settings): Settings => ({ ...current, locale: 'fr' });
    const pending = settings.save(patch);

    expect(settings.saving).toBe(true);
    expect(ipc.updateSettings).toHaveBeenCalledWith(patch(settings.document!));

    request.resolve(makeSettings({ locale: 'fr' }));
    await pending;

    expect(settings.saving).toBe(false);
    expect(settings.document?.locale).toBe('fr');
    expect(useI18n().locale.value).toBe('fr');
  });

  it('ignores a second save while the first is still in flight', async () => {
    const settings = useSettingsStore();
    settings.apply(makeSettings());

    const request = deferred<Settings>();
    vi.mocked(ipc.updateSettings).mockReturnValue(request.promise);

    const first = settings.save((current) => ({ ...current, locale: 'es' }));
    const second = settings.save((current) => ({ ...current, locale: 'zh' }));

    expect(ipc.updateSettings).toHaveBeenCalledTimes(1);

    request.resolve(makeSettings({ locale: 'es' }));
    await Promise.all([first, second]);

    expect(ipc.updateSettings).toHaveBeenCalledTimes(1);
    expect(settings.document?.locale).toBe('es');
  });

  it('sends nothing when there is no document to patch', async () => {
    const settings = useSettingsStore();

    await settings.save((current) => current);

    expect(ipc.updateSettings).not.toHaveBeenCalled();
    expect(settings.saving).toBe(false);
  });
});
