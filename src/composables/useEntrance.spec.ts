// @vitest-environment happy-dom

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { effectScope, type EffectScope, type Ref } from 'vue';

import { useEntranceWindow } from './useEntrance';

const DEFAULT_WINDOW_MS = 450;

let scopes: EffectScope[] = [];

function createScope(): EffectScope {
  const scope = effectScope();
  scopes.push(scope);
  return scope;
}

describe('useEntranceWindow', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    for (const scope of scopes) scope.stop();
    scopes = [];
    vi.useRealTimers();
  });

  it('is entering at first and leaves the entrance state when the default window elapses', () => {
    const scope = createScope();
    const entering = scope.run(() => useEntranceWindow()) as Readonly<Ref<boolean>>;

    expect(entering.value).toBe(true);

    vi.advanceTimersByTime(DEFAULT_WINDOW_MS - 1);
    expect(entering.value).toBe(true);

    vi.advanceTimersByTime(1);
    expect(entering.value).toBe(false);
  });

  it('honours a custom window', () => {
    const scope = createScope();
    const entering = scope.run(() => useEntranceWindow(1_200)) as Readonly<Ref<boolean>>;

    vi.advanceTimersByTime(1_199);
    expect(entering.value).toBe(true);

    vi.advanceTimersByTime(1);
    expect(entering.value).toBe(false);
  });

  it('clears the timeout when the scope is disposed', () => {
    const scope = createScope();
    const entering = scope.run(() => useEntranceWindow(100)) as Readonly<Ref<boolean>>;

    scope.stop();
    vi.advanceTimersByTime(1_000);

    expect(entering.value).toBe(true);
  });
});
