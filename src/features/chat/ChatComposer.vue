<script setup lang="ts">
/**
 * The textarea height is measured against `--localme-composer-max-height`; the length limit
 * mirrors the host's `MAX_BODY_CHARS`, which `send_message` enforces.
 */
import { computed, nextTick, onMounted, ref } from 'vue';

import { useI18n } from '@/i18n';
import type { Peer } from '@/ipc';
import MdIconButton from '@/ui/MdIconButton.vue';

const props = defineProps<{
  peer: Peer;
  sending: boolean;
}>();

const emit = defineEmits<{ send: [body: string] }>();

/** `MAX_BODY_CHARS` from `localme-core`'s protocol limits. */
const MAX_BODY_CHARS = 8000;
/** The share of the limit at which the counter appears, to warn before the message is lost. */
const COUNTER_AT = 0.8;

const i18n = useI18n();

const draft = ref('');
const textarea = ref<HTMLTextAreaElement | null>(null);
/** The token's value, read once: it is a fact of the stylesheet, not of this component. */
const maxHeight = ref(200);

const online = computed(() => props.peer.online);
const tooLong = computed(() => draft.value.length > MAX_BODY_CHARS);
const showCounter = computed(() => draft.value.length > MAX_BODY_CHARS * COUNTER_AT);
const canSend = computed(
  () => online.value && !props.sending && !tooLong.value && draft.value.trim().length > 0,
);

const invitation = computed(() =>
  i18n.t('chat.composerPlaceholder', { name: props.peer.nickname }),
);
const placeholder = computed(() =>
  online.value
    ? i18n.t('chat.composerPlaceholder', { name: props.peer.nickname })
    : i18n.t('chat.composerOffline', { name: props.peer.nickname }),
);

function resize(): void {
  const element = textarea.value;
  if (element === null) return;
  // Measured from `auto`, otherwise the current height would be the floor of the measurement.
  element.style.height = 'auto';
  element.style.height = `${Math.min(element.scrollHeight, maxHeight.value)}px`;
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    event.preventDefault();
    draft.value = '';
    void nextTick(resize);
    return;
  }
  // `isComposing` keeps the Enter that commits an IME candidate from sending the message.
  if (event.key !== 'Enter' || event.shiftKey || event.isComposing) return;
  event.preventDefault();
  submit();
}

function submit(): void {
  if (!canSend.value) return;
  emit('send', draft.value);
  draft.value = '';
  void nextTick(resize);
}

onMounted(() => {
  const element = textarea.value;
  if (element === null) return;
  const token = getComputedStyle(element).getPropertyValue('--localme-composer-max-height');
  const parsed = Number.parseFloat(token);
  if (Number.isFinite(parsed) && parsed > 0) maxHeight.value = parsed;
  resize();
});
</script>

<template>
  <footer class="composer">
    <p v-if="!online" class="md-typescale-label-small composer__offline">
      {{ i18n.t('chat.composerOfflineHint', { name: peer.nickname }) }}
    </p>

    <div class="composer__row">
      <textarea
        ref="textarea"
        v-model="draft"
        class="md-typescale-body-medium composer__input"
        rows="1"
        :placeholder="placeholder"
        :aria-label="invitation"
        :disabled="!online"
        @input="resize"
        @keydown="onKeydown"
      />
      <MdIconButton
        icon="send"
        variant="filled"
        :label="i18n.t('chat.send')"
        :disabled="!canSend"
        @click="submit"
      />
    </div>

    <div class="composer__footer md-typescale-label-small">
      <span class="composer__hint">{{ i18n.t('chat.composerHint') }}</span>
      <span v-if="tooLong" class="composer__error">
        {{ i18n.t('chat.tooLong', { max: MAX_BODY_CHARS }) }}
      </span>
      <span v-else-if="showCounter" class="composer__counter">
        {{ draft.length }} / {{ MAX_BODY_CHARS }}
      </span>
    </div>
  </footer>
</template>

<style scoped>
.composer {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 16px 12px;
  border-block-start: 1px solid var(--md-sys-color-outline-variant);
  background: var(--md-sys-color-surface);
}

.composer__offline {
  color: var(--md-sys-color-on-surface-variant);
}

.composer__row {
  display: flex;
  gap: 8px;
  align-items: flex-end;
}

.composer__input {
  flex: 1;
  min-width: 0;
  min-height: 44px;
  max-height: var(--localme-composer-max-height);
  padding: 11px 16px;
  border: 1px solid transparent;
  border-radius: var(--md-sys-shape-corner-extra-large);
  background: var(--md-sys-color-surface-container-highest);
  color: var(--md-sys-color-on-surface);
  resize: none;
  overflow-y: auto;
  transition: border-color var(--md-sys-motion-duration-short3) var(--md-sys-motion-easing-standard);
}

.composer__input::placeholder {
  color: var(--md-sys-color-on-surface-variant);
}

.composer__input:focus-visible {
  border-color: var(--md-sys-color-primary);
  outline: none;
}

.composer__input:disabled {
  background: var(--md-sys-color-surface-container);
  color: var(--md-sys-color-on-surface-variant);
}

.composer__footer {
  display: flex;
  gap: 12px;
  align-items: baseline;
  justify-content: space-between;
  min-height: 16px;
  padding-inline: 4px;
}

.composer__hint,
.composer__counter {
  color: var(--md-sys-color-on-surface-variant);
}

.composer__error {
  color: var(--md-sys-color-error);
}
</style>
