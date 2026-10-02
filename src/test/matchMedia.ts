/**
 * A controllable `window.matchMedia`, so a test can drive the media queries the layout and the
 * motion preference branch on instead of depending on what a headless DOM happens to report.
 *
 * `setMediaMatches` is the only way the result changes, which keeps a test that cares about the
 * dark scheme or the two-pane breakpoint explicit about it.
 */

type ChangeListener = (event: MediaQueryListEvent) => void;

const matches = new Map<string, boolean>();
const listeners = new Map<string, Set<ChangeListener>>();

function listenersFor(query: string): Set<ChangeListener> {
  const existing = listeners.get(query);
  if (existing) return existing;
  const created = new Set<ChangeListener>();
  listeners.set(query, created);
  return created;
}

function createList(query: string): MediaQueryList {
  const list: MediaQueryList = {
    get matches(): boolean {
      return matches.get(query) ?? false;
    },
    media: query,
    onchange: null,
    addEventListener(type: string, listener: EventListenerOrEventListenerObject | null): void {
      if (type !== 'change' || listener === null) return;
      if (typeof listener === 'function') listenersFor(query).add(listener);
    },
    removeEventListener(type: string, listener: EventListenerOrEventListenerObject | null): void {
      if (type !== 'change' || listener === null) return;
      if (typeof listener === 'function') listenersFor(query).delete(listener);
    },
    addListener(listener: ChangeListener | null): void {
      if (listener !== null) listenersFor(query).add(listener);
    },
    removeListener(listener: ChangeListener | null): void {
      if (listener !== null) listenersFor(query).delete(listener);
    },
    dispatchEvent(): boolean {
      return true;
    },
  };
  return list;
}

/** Publishes a new result for `query` and notifies everyone watching it. */
export function setMediaMatches(query: string, value: boolean): void {
  matches.set(query, value);
  for (const listener of listenersFor(query)) {
    listener({ matches: value, media: query } as MediaQueryListEvent);
  }
}

/** Forgets every query registered by the previous test. */
export function resetMediaMatches(): void {
  matches.clear();
  listeners.clear();
}

/** Installs the stub wherever a DOM exists; a plain-node test suite has nothing to install. */
export function installMatchMedia(): void {
  if (typeof window === 'undefined') return;
  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    writable: true,
    value: (query: string): MediaQueryList => createList(query),
  });
}
