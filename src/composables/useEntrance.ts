/**
 * Whether a component is still in the first moments of its life.
 *
 * Both lists in this application are virtualised, which means rows mount as the reader scrolls
 * and unmount again on the way back. A naive entrance animation in a row therefore replays on
 * every flick of the wheel — a flicker, not an animation. This turns the entrance into a
 * property of the *list appearing*: rows that mount while the window is open animate in, and
 * everything after that is simply there.
 *
 * The window is a timer rather than a counter because it is a statement about time on screen,
 * not about how many rows happened to be mounted.
 */
import { onScopeDispose, readonly, ref, type Ref } from 'vue';

/** How long rows keep animating in after the list appears. */
const DEFAULT_WINDOW_MS = 450;

/** A flag that is `true` for the first `windowMs` of the component's life. */
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
