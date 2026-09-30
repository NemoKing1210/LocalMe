/**
 * A clock that ticks slowly, for text that says "5 minutes ago".
 *
 * Relative times have to re-render as time passes, but they only change at minute granularity:
 * a one-second interval would redraw the whole user list sixty times a minute to change one
 * word. Thirty seconds keeps the visible text at most half a minute stale.
 *
 * The interval is stopped while the document is hidden: a window in the tray has nobody reading
 * it, and a redraw there is exactly the kind of idle work this application avoids.
 */
import { onScopeDispose, readonly, ref, type Ref } from 'vue';

/** How often the clock advances while the window is visible. */
const TICK_MS = 30_000;

/** A `Date.now()` value that moves forward on its own. */
export function useNow(): Readonly<Ref<number>> {
  const now = ref(Date.now());
  let timer: ReturnType<typeof setInterval> | null = null;

  const start = (): void => {
    if (timer !== null) return;
    timer = setInterval(() => {
      now.value = Date.now();
    }, TICK_MS);
  };

  const stop = (): void => {
    if (timer === null) return;
    clearInterval(timer);
    timer = null;
  };

  const onVisibility = (): void => {
    now.value = Date.now();
    if (document.hidden) stop();
    else start();
  };

  if (!document.hidden) start();
  document.addEventListener('visibilitychange', onVisibility);

  onScopeDispose(() => {
    stop();
    document.removeEventListener('visibilitychange', onVisibility);
  });

  return readonly(now);
}
