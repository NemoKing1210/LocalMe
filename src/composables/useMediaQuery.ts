/**
 * Reactive media queries.
 *
 * A listener, not a poll: `matchMedia` fires on change, so a window that is resized or moved to
 * a different display costs nothing until it actually changes. The listener is removed when the
 * component that asked is unmounted.
 */
import { onScopeDispose, readonly, ref, type Ref } from 'vue';

/** A media query result that stays current. */
export function useMediaQuery(query: string): Readonly<Ref<boolean>> {
  const matches = ref(false);
  const media = window.matchMedia(query);
  matches.value = media.matches;

  const listener = (event: MediaQueryListEvent): void => {
    matches.value = event.matches;
  };
  media.addEventListener('change', listener);
  onScopeDispose(() => {
    media.removeEventListener('change', listener);
  });

  return readonly(matches);
}

/** The width below which the layout shows one pane at a time. */
export const TWO_PANE_QUERY = '(min-width: 720px)';
