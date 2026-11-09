// @vitest-environment happy-dom

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { DOMWrapper, VueWrapper } from '@vue/test-utils';

vi.mock('@/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/ipc')>()),
  minimizeWindow: vi.fn(),
  toggleMaximizeWindow: vi.fn(),
  closeWindow: vi.fn(),
  isWindowMaximized: vi.fn(),
  onWindowResized: vi.fn(),
}));

import * as ipc from '@/ipc';
import { translate } from '@/i18n';
import { flushPromises, mountView } from '@/test/mount';

import AppTitleBar from './AppTitleBar.vue';

interface BarOptions {
  readonly maximized?: boolean;
}

async function mountBar(options: BarOptions = {}): Promise<VueWrapper> {
  vi.mocked(ipc.isWindowMaximized).mockResolvedValue(options.maximized ?? false);
  vi.mocked(ipc.onWindowResized).mockResolvedValue(() => {});

  const wrapper = await mountView(AppTitleBar);
  await flushPromises();
  return wrapper;
}

function button(wrapper: VueWrapper, label: string): DOMWrapper<Element> {
  return wrapper.find(`button[aria-label="${label}"]`);
}

describe('AppTitleBar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('makes the whole strip a drag region, since the system frame is gone', async () => {
    const wrapper = await mountBar();

    expect(wrapper.get('header').attributes('data-tauri-drag-region')).toBe('deep');
  });

  it('labels every window control', async () => {
    const wrapper = await mountBar();

    expect(button(wrapper, translate('window.minimize')).exists()).toBe(true);
    expect(button(wrapper, translate('window.maximize')).exists()).toBe(true);
    expect(button(wrapper, translate('window.close')).exists()).toBe(true);
  });

  it('minimises, toggles and closes the window through the host boundary', async () => {
    const wrapper = await mountBar();

    await button(wrapper, translate('window.minimize')).trigger('click');
    await button(wrapper, translate('window.maximize')).trigger('click');
    await button(wrapper, translate('window.close')).trigger('click');

    expect(ipc.minimizeWindow).toHaveBeenCalledTimes(1);
    expect(ipc.toggleMaximizeWindow).toHaveBeenCalledTimes(1);
    expect(ipc.closeWindow).toHaveBeenCalledTimes(1);
  });

  it('offers restore instead of maximize while the window is maximized', async () => {
    const wrapper = await mountBar({ maximized: true });

    expect(button(wrapper, translate('window.restore')).exists()).toBe(true);
    expect(button(wrapper, translate('window.maximize')).exists()).toBe(false);
  });

  it('re-reads the maximize state after a resize settles', async () => {
    const wrapper = await mountBar();
    expect(button(wrapper, translate('window.maximize')).exists()).toBe(true);

    vi.useFakeTimers();
    try {
      vi.mocked(ipc.isWindowMaximized).mockResolvedValue(true);
      const onResized = vi.mocked(ipc.onWindowResized).mock.calls[0]?.[0];
      expect(onResized).toBeDefined();
      onResized?.();

      await vi.advanceTimersByTimeAsync(80);
      await flushPromises();
    } finally {
      vi.useRealTimers();
    }

    expect(button(wrapper, translate('window.restore')).exists()).toBe(true);
  });

  it('stops listening for resizes when it goes away', async () => {
    const stop = vi.fn();
    vi.mocked(ipc.onWindowResized).mockResolvedValue(stop);

    const wrapper = await mountView(AppTitleBar);
    await flushPromises();
    wrapper.unmount();

    expect(stop).toHaveBeenCalledTimes(1);
  });

  it('stays inert when there is no host to ask', async () => {
    vi.mocked(ipc.isWindowMaximized).mockRejectedValue(new Error('no host'));
    vi.mocked(ipc.onWindowResized).mockRejectedValue(new Error('no host'));

    const wrapper = await mountView(AppTitleBar);
    await flushPromises();

    expect(wrapper.find('.title-bar').exists()).toBe(true);
    expect(button(wrapper, translate('window.maximize')).exists()).toBe(true);
  });
});
