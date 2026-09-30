<script setup lang="ts">
/**
 * One message bubble.
 *
 * Three decisions are visible here. The direction chooses the colour roles and the side of the
 * column. Only the corner nearest the sender is pulled tight — the other three stay `large` —
 * which is what makes a run of messages read as one column instead of a stack of cards. And the
 * body is text with `white-space: pre-wrap`, so a newline is a line break and a `<script>` tag
 * is nine characters, never markup.
 */
import { computed } from 'vue';
import { AnimatePresence, motion } from 'motion-v';

import { useI18n, type MessageKey } from '@/i18n';
import type { Message, MessageStatus } from '@/ipc';
import { useChatStore } from '@/stores/chat';
import MdIcon from '@/ui/MdIcon.vue';
import type { IconName } from '@/ui/icons';

const props = defineProps<{
  /** The message to draw. */
  message: Message;
  /** Whether delivery progress belongs beside the time. History does not need it. */
  showStatus: boolean;
}>();

const i18n = useI18n();
const chat = useChatStore();

/**
 * Whether this bubble is being drawn for the first time.
 *
 * Asked once, in `setup`: a message that arrives animates in, and the same message scrolling
 * back into view is drawn without ceremony. See `chat.consumeEntrance`.
 */
const entrance = chat.consumeEntrance(props.message.id);

/**
 * The glyph and the sentence for each delivery state.
 *
 * A record rather than a `switch`, so a state added to the host's enum is a compile error here
 * instead of a message that silently shows nothing.
 */
const GLYPHS: Record<MessageStatus, { readonly icon: IconName; readonly key: MessageKey } | null> =
  {
    sending: { icon: 'clock', key: 'chat.statusSending' },
    sent: { icon: 'check', key: 'chat.statusSent' },
    delivered: { icon: 'check-all', key: 'chat.statusDelivered' },
    failed: { icon: 'error', key: 'chat.statusFailed' },
    received: null,
  };

const outgoing = computed(() => props.message.direction === 'outgoing');

/**
 * The delivery glyph, with the sentence that explains it.
 *
 * `null` covers two cases that are deliberately the same: a message that is not ours — its
 * delivery is not our business — and `received`, which is the state every incoming message is
 * in and therefore says nothing.
 */
const status = computed<{ readonly icon: IconName; readonly label: string } | null>(() => {
  if (!props.showStatus || props.message.direction !== 'outgoing') return null;
  const glyph = GLYPHS[props.message.status];
  return glyph === null ? null : { icon: glyph.icon, label: i18n.t(glyph.key) };
});
</script>

<template>
  <motion.article
    class="bubble"
    :class="outgoing ? 'bubble--outgoing' : 'bubble--incoming'"
    :initial="entrance ? { opacity: 0, y: 12, scale: 0.98 } : false"
    :animate="{ opacity: 1, y: 0, scale: 1 }"
    :transition="{ type: 'spring', stiffness: 420, damping: 34 }"
  >
    <p class="md-typescale-body-medium bubble__body" data-selectable>{{ message.body }}</p>
    <footer class="md-typescale-label-small bubble__meta">
      <span class="bubble__time">{{ i18n.clock(message.sentAt) }}</span>
      <!-- One glyph at a time, cross-faded: sending → sent → delivered is the one thing on a
           bubble that changes after it is on screen, and a swap with no transition reads as a
           rendering glitch rather than as progress. -->
      <AnimatePresence mode="wait">
        <motion.span
          v-if="status"
          :key="status.icon"
          class="bubble__status"
          :class="{ 'bubble__status--failed': message.status === 'failed' }"
          :initial="{ opacity: 0, scale: 0.6 }"
          :animate="{ opacity: 1, scale: 1 }"
          :exit="{ opacity: 0, scale: 0.6 }"
          :transition="{ duration: 0.14, ease: [0.2, 0, 0, 1] }"
        >
          <MdIcon :name="status.icon" :size="14" :title="status.label" />
        </motion.span>
      </AnimatePresence>
    </footer>
  </motion.article>
</template>

<style scoped>
.bubble {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-width: min(560px, 75%);
  padding: 8px 14px 6px;
  border-radius: var(--md-sys-shape-corner-large);
}

/* `border-end-*-radius` is the corner nearest the sender: the bottom one on their side. */
.bubble--incoming {
  align-self: flex-start;
  border-end-start-radius: var(--md-sys-shape-corner-extra-small);
  background: var(--md-sys-color-surface-container-high);
  color: var(--md-sys-color-on-surface);
}

.bubble--outgoing {
  align-self: flex-end;
  border-end-end-radius: var(--md-sys-shape-corner-extra-small);
  background: var(--md-sys-color-primary-container);
  color: var(--md-sys-color-on-primary-container);
}

.bubble__body {
  /* Newlines are content; a pasted 200-character URL still has to fit the bubble. */
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.bubble__meta {
  display: flex;
  gap: 4px;
  align-items: center;
  align-self: flex-end;
  color: var(--md-sys-color-on-surface-variant);
}

.bubble__time {
  font-variant-numeric: tabular-nums;
}

.bubble__status {
  display: inline-flex;
  align-items: center;
}

.bubble__status--failed {
  color: var(--md-sys-color-error);
}
</style>
