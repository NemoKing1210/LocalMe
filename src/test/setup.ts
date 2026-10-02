import { enableAutoUnmount } from '@vue/test-utils';
import { afterEach, beforeEach } from 'vitest';

import { setLocale } from '@/i18n';
import { installMatchMedia, resetMediaMatches } from './matchMedia';

/**
 * The parts of a browser the interface uses that a headless DOM either lacks or cannot be
 * steered through. Everything here is a no-op or a deterministic stub: a test that needs a fact
 * from one of them must set it itself.
 */

class NoopObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
  takeRecords(): [] {
    return [];
  }
}

function installObservers(): void {
  const globalWithObservers = globalThis as {
    ResizeObserver?: unknown;
    IntersectionObserver?: unknown;
  };
  globalWithObservers.ResizeObserver ??= NoopObserver;
  globalWithObservers.IntersectionObserver ??= NoopObserver;
}

function installScroll(): void {
  Element.prototype.scrollTo = function scrollTo(): void {};
  Element.prototype.scrollIntoView = function scrollIntoView(): void {};
  Element.prototype.animate = function animate(): Animation {
    return {
      cancel(): void {},
      finish(): void {},
      play(): void {},
      pause(): void {},
      reverse(): void {},
      addEventListener(): void {},
      removeEventListener(): void {},
      finished: Promise.resolve(),
      onfinish: null,
      playState: 'idle',
    } as unknown as Animation;
  };
}

if (typeof window !== 'undefined') {
  installObservers();
  installScroll();
  installMatchMedia();
  // Unmount whatever a test mounted, so the next test starts with an empty document.
  enableAutoUnmount(afterEach);
}

// Module-level state the interface shares across a session, restored between tests so one
// file cannot decide what the next one renders.
beforeEach(() => {
  resetMediaMatches();
  setLocale('en');
});
