import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import { useI18n } from '@/i18n';
import type { MessageKey } from '@/i18n';
import { CommandError } from '@/ipc';
import type { ApiError } from '@/ipc';

/**
 * A sentence for each failure the host can name.
 *
 * `CommandError.message` is built from the wire fields and is English; this is what the user
 * actually reads, in their own language. `invalid_input` is absent on purpose — the field's own
 * message is shown next to the field — so it falls back to the caller's key.
 */
const ERROR_KEYS: Partial<Record<ApiError['kind'], MessageKey>> = {
  storage: 'error.storage',
  discovery: 'error.discovery',
  network: 'error.network',
  unknown_peer: 'error.unknownPeer',
  shutting_down: 'error.shuttingDown',
  internal: 'error.internal',
};

const SNACKBAR_TIMEOUT_MS = 6_000;

export interface Notice {
  readonly id: number;
  readonly key: MessageKey;
  readonly params?: Record<string, string | number>;
  readonly tone: 'neutral' | 'error';
}

/** The outcome of one host command run through [`useUiStore`]'s `attempt`. */
export type AttemptResult<T> = { readonly ok: true; readonly value: T } | { readonly ok: false };

export const useUiStore = defineStore('ui', () => {
  const notice = ref<Notice | null>(null);
  let nextId = 1;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const i18n = useI18n();

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

  function notify(key: MessageKey, params?: Record<string, string | number>): void {
    dismiss();
    notice.value = { id: nextId, key, params: params ?? {}, tone: 'neutral' };
    nextId += 1;
    timer = setTimeout(dismiss, SNACKBAR_TIMEOUT_MS);
  }

  function fail(key: MessageKey, params?: Record<string, string | number>): void {
    notify(key, params);
    if (notice.value) notice.value = { ...notice.value, tone: 'error' };
  }

  /**
   * Runs one host command and reports a failure the way every action does: the detail goes to
   * the console, the user gets one sentence.
   *
   * The result is a discriminated value rather than `undefined`, because plenty of commands
   * succeed while returning nothing and the caller must still be able to tell the two apart.
   */
  async function attempt<T>(
    action: () => Promise<T>,
    what: string,
    failure: MessageKey = 'error.internal',
  ): Promise<AttemptResult<T>> {
    try {
      return { ok: true, value: await action() };
    } catch (error) {
      console.error(`[localme] ${what}`, error);
      // The host's own tag picks the sentence: the caller's `failure` is only the fallback for a
      // command that failed in a way the host could not name.
      const named = error instanceof CommandError ? ERROR_KEYS[error.kind] : undefined;
      fail(named ?? failure);
      return { ok: false };
    }
  }

  return { notice, noticeText, notify, fail, attempt, dismiss };
});

/** The store's public shape, for the tests and their helpers that carry one around. */
export type UiStore = ReturnType<typeof useUiStore>;
