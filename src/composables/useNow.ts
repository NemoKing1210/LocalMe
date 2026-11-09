import { onScopeDispose, readonly, ref, type Ref } from 'vue';

const TICK_MS = 30_000;

/**
 * One clock for the whole interface.
 *
 * A component that renders "online 2 minutes ago" needs to be redrawn as the sentence ages, but
 * a timer per caller means a timer per rendered list row — the same instant recomputed and
 * re-rendered many times a minute. The interval and the visibility listener are shared, and they
 * exist only while at least one caller is alive.
 */
const now = ref(Date.now());
let subscribers = 0;
let timer: ReturnType<typeof setInterval> | null = null;

function start(): void {
  if (timer !== null) return;
  timer = setInterval(() => {
    now.value = Date.now();
  }, TICK_MS);
}

function stop(): void {
  if (timer === null) return;
  clearInterval(timer);
  timer = null;
}

function onVisibility(): void {
  now.value = Date.now();
  if (document.hidden) stop();
  else start();
}

export function useNow(): Readonly<Ref<number>> {
  subscribers += 1;
  if (subscribers === 1) {
    now.value = Date.now();
    document.addEventListener('visibilitychange', onVisibility);
    if (!document.hidden) start();
  }

  onScopeDispose(() => {
    subscribers -= 1;
    if (subscribers > 0) return;
    document.removeEventListener('visibilitychange', onVisibility);
    stop();
  });

  return readonly(now);
}
