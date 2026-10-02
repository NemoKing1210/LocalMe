import { onScopeDispose, readonly, ref, type Ref } from 'vue';

const TICK_MS = 30_000;

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
