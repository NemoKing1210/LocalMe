/**
 * The small amount of state that is about the interface itself rather than about peers or
 * messages: the transient message shown in a snackbar.
 *
 * Deliberately not the current screen: which page is on screen is the router's business, and a
 * second copy of it here is exactly the kind of state that drifts out of step with the address
 * bar.
 */
import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import { useI18n } from '@/i18n';
import type { MessageKey } from '@/i18n';

/** How long a snackbar stays on screen before it is dismissed. */
const SNACKBAR_TIMEOUT_MS = 6_000;

/**
 * Something to tell the user, in their language.
 *
 * A key plus parameters rather than a rendered string, so it is translated at display time and
 * follows a language change instead of freezing the language it was created in.
 */
export interface Notice {
  readonly id: number;
  readonly key: MessageKey;
  readonly params?: Record<string, string | number>;
  readonly tone: 'neutral' | 'error';
}

export const useUiStore = defineStore('ui', () => {
  const notice = ref<Notice | null>(null);
  let nextId = 1;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const i18n = useI18n();

  /** The notice's text, translated now. */
  const noticeText = computed<string>(() =>
    notice.value ? i18n.t(notice.value.key, notice.value.params) : '',
  );

  function dismiss(): void {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
    notice.value = null;
  }

  /**
   * Shows a notice.
   *
   * One at a time on purpose: two snackbars stacked in the corner of a messenger is worse than
   * losing the older one, and the newer message is the one the user just caused.
   */
  function notify(key: MessageKey, params?: Record<string, string | number>): void {
    dismiss();
    notice.value = { id: nextId, key, params: params ?? {}, tone: 'neutral' };
    nextId += 1;
    timer = setTimeout(dismiss, SNACKBAR_TIMEOUT_MS);
  }

  /** Shows a notice in the error tone. */
  function fail(key: MessageKey, params?: Record<string, string | number>): void {
    notify(key, params);
    if (notice.value) notice.value = { ...notice.value, tone: 'error' };
  }

  return { notice, noticeText, notify, fail, dismiss };
});
