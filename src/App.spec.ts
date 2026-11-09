// @vitest-environment happy-dom

import { createPinia, setActivePinia, type Pinia } from 'pinia';
import { defineComponent, nextTick } from 'vue';
import type { VueWrapper } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/ipc')>()),
  bootstrap: vi.fn(),
  setUiLabels: vi.fn(),
  logFrontend: vi.fn(),
}));
vi.mock('@/app/connect', () => ({ connectCoreEvents: vi.fn() }));
vi.mock('@/app/ready', () => ({ signalReady: vi.fn() }));

import * as ipc from '@/ipc';
import { connectCoreEvents } from '@/app/connect';
import { signalReady } from '@/app/ready';
import { TWO_PANE_QUERY } from '@/composables/useMediaQuery';
import { setLocale, translate } from '@/i18n';
import { setMediaMatches } from '@/test/matchMedia';
import { usePeerStore } from '@/stores/peers';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';
import { setThemeMode } from '@/theme/useTheme';
import { makeBootstrap, makePeer, makeSettings } from '@/test/factories';
import { flushPromises, mountView } from '@/test/mount';

import App from './App.vue';

/** The real bar has its own spec; here only its presence in the layout is the subject. */
const TitleBarMarker = defineComponent({
  name: 'AppTitleBar',
  template: '<header class="title-bar-marker" />',
});

function freshPinia(): Pinia {
  const pinia = createPinia();
  setActivePinia(pinia);
  return pinia;
}

async function mountApp(pinia: Pinia): Promise<VueWrapper> {
  const wrapper = await mountView(App, {
    pinia,
    stubs: { AppTitleBar: TitleBarMarker },
  });
  await flushPromises();
  return wrapper;
}

beforeEach(() => {
  vi.mocked(ipc.bootstrap).mockResolvedValue(
    makeBootstrap({ settings: makeSettings(), peers: [makePeer({ deviceId: 'device-a' })] }),
  );
  vi.mocked(ipc.setUiLabels).mockResolvedValue(undefined);
  vi.mocked(connectCoreEvents).mockResolvedValue(() => {});
  vi.mocked(signalReady).mockResolvedValue(undefined);
});

afterEach(() => {
  setThemeMode('system');
  setLocale('en');
});

describe('App', () => {
  it('applies the bootstrap payload, connects the events and signals readiness', async () => {
    const pinia = freshPinia();
    const wrapper = await mountApp(pinia);

    const settings = useSettingsStore();
    const peers = usePeerStore();
    expect(settings.document).toEqual(makeSettings());
    expect(peers.peers.map((peer) => peer.deviceId)).toEqual(['device-a']);
    expect(connectCoreEvents).toHaveBeenCalledTimes(1);
    expect(signalReady).toHaveBeenCalledTimes(1);
    expect(wrapper.find('.app__spinner').exists()).toBe(false);

    const connected = vi.mocked(connectCoreEvents).mock.invocationCallOrder[0] ?? 0;
    const ready = vi.mocked(signalReady).mock.invocationCallOrder[0] ?? 0;
    expect(ready).toBeGreaterThan(connected);
  });

  it('raises a snackbar when storage was recovered', async () => {
    vi.mocked(ipc.bootstrap).mockResolvedValue(
      makeBootstrap({ settings: makeSettings(), storageRecovered: '/tmp/localme.recovered' }),
    );

    await mountApp(freshPinia());

    const ui = useUiStore();
    expect(ui.notice?.key).toBe('error.databaseRecovered');
    expect(ui.notice?.params).toEqual({ path: '/tmp/localme.recovered' });
  });

  it('raises a snackbar when discovery could not start', async () => {
    vi.mocked(ipc.bootstrap).mockResolvedValue(
      makeBootstrap({ settings: makeSettings(), discoveryProblem: 'port in use' }),
    );

    await mountApp(freshPinia());

    expect(useUiStore().notice?.key).toBe('error.discovery');
  });

  it('shows the failure state when the host does not answer', async () => {
    const failure = new Error('no host');
    vi.mocked(ipc.bootstrap).mockRejectedValue(failure);
    vi.mocked(ipc.logFrontend).mockResolvedValue(undefined);
    const logged = vi.spyOn(console, 'error').mockImplementation(() => {});

    const wrapper = await mountApp(freshPinia());

    expect(wrapper.text()).toContain(translate('error.internal'));
    expect(wrapper.find('.app__spinner').exists()).toBe(false);
    expect(connectCoreEvents).not.toHaveBeenCalled();
    expect(signalReady).toHaveBeenCalledTimes(1);
    expect(logged).toHaveBeenCalledWith(
      expect.stringContaining('the host did not answer the bootstrap command'),
      failure,
    );
    // The same failure in the host's own log: this path never reaches the global handlers, and
    // an error screen is exactly what a user reports without any detail.
    expect(ipc.logFrontend).toHaveBeenCalledWith(
      'error',
      expect.stringContaining('startup: the host did not answer the bootstrap command'),
      expect.any(String),
    );
  });

  it('pushes the UI labels at once and again on a language change', async () => {
    await mountApp(freshPinia());

    expect(ipc.setUiLabels).toHaveBeenCalledTimes(1);
    const first = vi.mocked(ipc.setUiLabels).mock.calls[0]?.[0];
    expect(first?.open).toBe(translate('tray.open'));

    setLocale('ru');
    await nextTick();
    await flushPromises();

    expect(ipc.setUiLabels).toHaveBeenCalledTimes(2);
    const second = vi.mocked(ipc.setUiLabels).mock.calls[1]?.[0];
    expect(second?.open).toBe(translate('tray.open'));
    expect(second?.open).not.toBe(first?.open);
  });

  it('draws the title bar when the host removed the native frame', async () => {
    const wrapper = await mountApp(freshPinia());

    expect(wrapper.find('.title-bar-marker').exists()).toBe(true);
    expect(wrapper.get('.app').attributes('data-title-bar')).toBe('true');
  });

  it('leaves the bar to the shell when the two-pane layout is on', async () => {
    setMediaMatches(TWO_PANE_QUERY, true);

    const wrapper = await mountApp(freshPinia());

    // The shell places it inside the detail column, so the list column reaches the top.
    expect(wrapper.find('.title-bar-marker').exists()).toBe(false);
    expect(wrapper.get('.app').attributes('data-title-bar')).toBe('false');
  });

  it('leaves the frame to the system when the host kept it', async () => {
    vi.mocked(ipc.bootstrap).mockResolvedValue(
      makeBootstrap({ settings: makeSettings(), customTitlebar: false }),
    );

    const wrapper = await mountApp(freshPinia());

    expect(wrapper.find('.title-bar-marker').exists()).toBe(false);
    expect(wrapper.get('.app').attributes('data-title-bar')).toBe('false');
  });
});
