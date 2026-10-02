import { onScopeDispose, readonly, ref, type Ref } from 'vue';

const DEFAULT_WINDOW_MS = 450;

export function useEntranceWindow(windowMs: number = DEFAULT_WINDOW_MS): Readonly<Ref<boolean>> {
  const entering = ref(true);
  const timer = setTimeout(() => {
    entering.value = false;
  }, windowMs);
  onScopeDispose(() => {
    clearTimeout(timer);
  });
  return readonly(entering);
}
