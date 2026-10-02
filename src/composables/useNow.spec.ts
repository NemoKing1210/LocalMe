// @vitest-environment happy-dom

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { effectScope, type EffectScope, type Ref } from 'vue';

import { useNow } from './useNow';

const TICK_MS = 30_000;

/** `document.hidden` is read-only; an own accessor is the only way to steer it. */
function setHidden(hidden: boolean): void {
  Object.defineProperty(document, 'hidden', {
    configurable: true,
    get: () => hidden,
  });
}

function flashVisibility(): void {
  document.dispatchEvent(new Event('visibilitychange'));
}

let scopes: EffectScope[] = [];

function createScope(): EffectScope {
  const scope = effectScope();
  scopes.push(scope);
  return scope;
}

describe('useNow', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    for (const scope of scopes) scope.stop();
    scopes = [];
    Reflect.deleteProperty(document, 'hidden');
    vi.useRealTimers();
  });

  it('starts at the current time and refreshes once the interval elapses', () => {
    vi.setSystemTime(1_000);
    const scope = createScope();
    const now = scope.run(() => useNow()) as Readonly<Ref<number>>;

    expect(now.value).toBe(1_000);

    vi.advanceTimersByTime(TICK_MS - 1);
    expect(now.value).toBe(1_000);

    vi.advanceTimersByTime(1);
    expect(now.value).toBe(1_000 + TICK_MS);

    vi.advanceTimersByTime(TICK_MS);
    expect(now.value).toBe(1_000 + 2 * TICK_MS);
  });

  it('does not tick while hidden and refreshes immediately on becoming visible', () => {
    vi.setSystemTime(1_000);
    const scope = createScope();
    const now = scope.run(() => useNow()) as Readonly<Ref<number>>;

    setHidden(true);
    flashVisibility();

    vi.advanceTimersByTime(3 * TICK_MS);
    expect(now.value).toBe(1_000);

    setHidden(false);
    flashVisibility();
    expect(now.value).toBe(1_000 + 3 * TICK_MS);

    vi.advanceTimersByTime(TICK_MS);
    expect(now.value).toBe(1_000 + 4 * TICK_MS);
  });

  it('does not start an interval when created while hidden', () => {
    setHidden(true);
    vi.setSystemTime(5_000);
    const scope = createScope();
    const now = scope.run(() => useNow()) as Readonly<Ref<number>>;

    vi.advanceTimersByTime(TICK_MS);
    expect(now.value).toBe(5_000);

    setHidden(false);
    flashVisibility();
    expect(now.value).toBe(5_000 + TICK_MS);

    vi.advanceTimersByTime(TICK_MS);
    expect(now.value).toBe(5_000 + 2 * TICK_MS);
  });

  it('stops the interval and drops the visibility listener when the scope is disposed', () => {
    vi.setSystemTime(1_000);
    const scope = createScope();
    const now = scope.run(() => useNow()) as Readonly<Ref<number>>;

    vi.advanceTimersByTime(TICK_MS);
    expect(now.value).toBe(1_000 + TICK_MS);

    const frozen = now.value;
    scope.stop();

    vi.advanceTimersByTime(TICK_MS);
    expect(now.value).toBe(frozen);

    // A listener that was not removed would assign `Date.now()` on this dispatch.
    setHidden(true);
    flashVisibility();
    expect(now.value).toBe(frozen);
  });
});
