// @vitest-environment happy-dom

import { afterEach, describe, expect, it } from 'vitest';
import { effectScope, type EffectScope, type Ref } from 'vue';

import { setMediaMatches } from '@/test/matchMedia';

import { TWO_PANE_QUERY, useMediaQuery } from './useMediaQuery';

const REDUCED_MOTION_QUERY = '(prefers-reduced-motion: reduce)';

let scopes: EffectScope[] = [];

function createScope(): EffectScope {
  const scope = effectScope();
  scopes.push(scope);
  return scope;
}

afterEach(() => {
  for (const scope of scopes) scope.stop();
  scopes = [];
});

describe('useMediaQuery', () => {
  it('reads the query result when it is created', () => {
    setMediaMatches(TWO_PANE_QUERY, true);
    setMediaMatches(REDUCED_MOTION_QUERY, false);

    const scope = createScope();
    const twoPane = scope.run(() => useMediaQuery(TWO_PANE_QUERY)) as Readonly<Ref<boolean>>;
    const reduced = scope.run(() => useMediaQuery(REDUCED_MOTION_QUERY)) as Readonly<Ref<boolean>>;

    expect(twoPane.value).toBe(true);
    expect(reduced.value).toBe(false);
  });

  it('tracks changes the media query reports', () => {
    const scope = createScope();
    const matches = scope.run(() => useMediaQuery(TWO_PANE_QUERY)) as Readonly<Ref<boolean>>;
    expect(matches.value).toBe(false);

    setMediaMatches(TWO_PANE_QUERY, true);
    expect(matches.value).toBe(true);

    setMediaMatches(TWO_PANE_QUERY, false);
    expect(matches.value).toBe(false);
  });

  it('stops tracking once the scope is disposed', () => {
    const scope = createScope();
    const matches = scope.run(() => useMediaQuery(TWO_PANE_QUERY)) as Readonly<Ref<boolean>>;

    scope.stop();
    setMediaMatches(TWO_PANE_QUERY, true);

    expect(matches.value).toBe(false);
  });
});
