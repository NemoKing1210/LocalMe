import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CommandError } from '@/ipc';

import { useUiStore } from './ui';

beforeEach(() => {
  setActivePinia(createPinia());
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('the ui store', () => {
  it('starts with no notice and no text', () => {
    const ui = useUiStore();

    expect(ui.notice).toBeNull();
    expect(ui.noticeText).toBe('');
  });

  it('creates a neutral notice and hands out increasing identifiers', () => {
    const ui = useUiStore();

    ui.notify('chat.copied');
    expect(ui.notice).toMatchObject({ id: 1, key: 'chat.copied', tone: 'neutral' });

    ui.notify('chat.copied');
    expect(ui.notice?.id).toBe(2);
  });

  it('marks the notice as an error through `fail`', () => {
    const ui = useUiStore();

    ui.fail('error.internal');

    expect(ui.notice?.tone).toBe('error');
    expect(ui.notice?.key).toBe('error.internal');
  });

  it('interpolates the notice params into the text', () => {
    const ui = useUiStore();

    ui.notify('chat.tooManyFiles', { max: 3 });

    expect(ui.noticeText).toBe('No more than 3 files in one message');
  });

  it('auto-dismisses after six seconds', () => {
    const ui = useUiStore();
    ui.notify('chat.copied');

    vi.advanceTimersByTime(5_999);
    expect(ui.notice).not.toBeNull();

    vi.advanceTimersByTime(1);
    expect(ui.notice).toBeNull();
    expect(ui.noticeText).toBe('');
  });

  it('dismisses immediately and cancels the pending timer', () => {
    const ui = useUiStore();
    ui.notify('chat.copied');

    ui.dismiss();

    expect(ui.notice).toBeNull();
    // The orphaned timer must not fire into a later notice.
    vi.advanceTimersByTime(10_000);
    expect(ui.notice).toBeNull();
  });

  it('replaces the previous notice and restarts the six-second window', () => {
    const ui = useUiStore();
    ui.notify('chat.copied');

    vi.advanceTimersByTime(5_000);
    ui.notify('chat.copied');

    // Five more seconds would have expired the first notice; the second holds.
    vi.advanceTimersByTime(5_000);
    expect(ui.notice).not.toBeNull();

    vi.advanceTimersByTime(1_000);
    expect(ui.notice).toBeNull();
  });
});

describe('attempt', () => {
  it('returns the value a command resolved with, and shows nothing', async () => {
    const ui = useUiStore();

    const result = await ui.attempt(() => Promise.resolve(42), 'the thing failed');

    expect(result).toEqual({ ok: true, value: 42 });
    expect(ui.notice).toBeNull();
  });

  it('reports the failure with the sentence for the host tag, not the caller fallback', async () => {
    const ui = useUiStore();
    const logged = vi.spyOn(console, 'error').mockImplementation(() => {});

    const result = await ui.attempt(
      () => Promise.reject(new CommandError({ kind: 'network', message: 'no route' })),
      'the thing failed',
      'error.storage',
    );

    expect(result).toEqual({ ok: false });
    expect(ui.notice).toMatchObject({ tone: 'error', key: 'error.network' });
    expect(logged).toHaveBeenCalledWith('[localme] the thing failed', expect.anything());
  });

  it('falls back to the caller key for a failure the host did not name', async () => {
    const ui = useUiStore();
    vi.spyOn(console, 'error').mockImplementation(() => {});

    // A rejected promise with a plain error: the host never tagged it.
    await ui.attempt(() => Promise.reject(new Error('boom')), 'the thing failed', 'error.storage');
    expect(ui.notice?.key).toBe('error.storage');

    // A tagged failure with no sentence of its own (`invalid_input` is shown on its field).
    await ui.attempt(
      () => Promise.reject(new CommandError({ kind: 'invalid_input', field: 'x', message: 'bad' })),
      'the thing failed',
      'error.storage',
    );
    expect(ui.notice?.key).toBe('error.storage');
  });

  it('keeps the value a command resolved with even when it is falsy', async () => {
    const ui = useUiStore();

    expect(await ui.attempt(() => Promise.resolve(0), 'the thing failed')).toEqual({
      ok: true,
      value: 0,
    });
    expect(await ui.attempt(() => Promise.resolve(undefined), 'the thing failed')).toEqual({
      ok: true,
      value: undefined,
    });
  });
});
