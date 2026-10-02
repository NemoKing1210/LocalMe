import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

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
