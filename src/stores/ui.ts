import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import { useI18n } from '@/i18n';
import type { MessageKey } from '@/i18n';

const SNACKBAR_TIMEOUT_MS = 6_000;

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

  return { notice, noticeText, notify, fail, dismiss };
});
