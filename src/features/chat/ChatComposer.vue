<script setup lang="ts">
/**
 * The textarea height is measured against `--localme-composer-max-height`; the length limit
 * mirrors the host's `MAX_BODY_CHARS`, which `send_message` enforces.
 */
import { computed, nextTick, onMounted, ref } from 'vue';
import { AnimatePresence, motion } from 'motion-v';

import { useI18n } from '@/i18n';
import type { Peer } from '@/ipc';
import MdIcon from '@/ui/MdIcon.vue';
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
// Sending to a peer that is away is allowed: the message waits in the outbox and goes out when
// they are back, so the composer is only gated by length and by an in-flight command.
const canSend = computed(() => !props.sending && !tooLong.value && draft.value.trim().length > 0);

const placeholder = computed(() =>
  i18n.t('chat.composerPlaceholder', { name: props.peer.nickname }),
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
    <AnimatePresence>
      <motion.p
        v-if="!online"
        class="md-typescale-label-large composer__offline"
        :initial="{ opacity: 0, y: 6 }"
        :animate="{ opacity: 1, y: 0 }"
        :exit="{ opacity: 0, y: 6 }"
        :transition="{ duration: 0.16, ease: [0.2, 0, 0, 1] }"
      >
        <MdIcon name="offline" :size="18" />
        <span>{{ i18n.t('chat.composerOfflineHint', { name: peer.nickname }) }}</span>
      </motion.p>
    </AnimatePresence>

    <div class="composer__row">
      <textarea
        ref="textarea"
        v-model="draft"
        class="md-typescale-body-medium composer__input"
        rows="1"
        :placeholder="placeholder"
        :aria-label="placeholder"
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

/*
 * A tonal banner rather than a line of grey text: the peer being away changes what sending
 * means, and the composer is where that has to be read. The container colour is the same tonal
 * surface the app's other secondary actions use, so it stands out from the field without
 * shouting like an error.
 */
.composer__offline {
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 8px 12px;
  border-radius: var(--md-sys-shape-corner-medium);
  background: var(--md-sys-color-secondary-container);
  color: var(--md-sys-color-on-secondary-container);
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
