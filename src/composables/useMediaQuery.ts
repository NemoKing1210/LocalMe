import { onScopeDispose, readonly, ref, type Ref } from 'vue';

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
