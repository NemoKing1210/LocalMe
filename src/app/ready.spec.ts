// @vitest-environment happy-dom

import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/event', () => ({ emit: vi.fn() }));

import { emit } from '@tauri-apps/api/event';

import { MAIN_WINDOW_READY_EVENT, signalReady } from './ready';

const emitMock = vi.mocked(emit);

describe('signalReady', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('emits the ready event under the name the host listens for', async () => {
    emitMock.mockResolvedValue(undefined);

    await signalReady();

    expect(MAIN_WINDOW_READY_EVENT).toBe('localme://ready');
    expect(emitMock).toHaveBeenCalledTimes(1);
    expect(emitMock).toHaveBeenCalledWith(MAIN_WINDOW_READY_EVENT);
  });

  it('swallows and logs an emit the host rejects', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const failure = new Error('no host listening');
    emitMock.mockRejectedValue(failure);

    await expect(signalReady()).resolves.toBeUndefined();

    expect(warn).toHaveBeenCalledTimes(1);
    expect(warn.mock.calls[0]?.[0]).toBe('[localme] host did not acknowledge readiness');
    expect(warn.mock.calls[0]?.[1]).toBe(failure);
  });
});
