<script setup lang="ts">
import { computed } from 'vue';
import { AnimatePresence, motion } from 'motion-v';

import { useI18n, type MessageKey } from '@/i18n';
import type { Message, MessageStatus } from '@/ipc';
import { useChatStore } from '@/stores/chat';
import MdIcon from '@/ui/MdIcon.vue';
import type { IconName } from '@/ui/icons';

const props = defineProps<{
  message: Message;
  showStatus: boolean;
}>();

const i18n = useI18n();
const chat = useChatStore();

// Asked once: a message animates only on first arrival, not when scrolled back into view.
const entrance = chat.consumeEntrance(props.message.id);

/** A record so a new host status is a compile error here, not a silently blank bubble. */
const GLYPHS: Record<MessageStatus, { readonly icon: IconName; readonly key: MessageKey } | null> =
  {
    sending: { icon: 'clock', key: 'chat.statusSending' },
    sent: { icon: 'check', key: 'chat.statusSent' },
    delivered: { icon: 'check-all', key: 'chat.statusDelivered' },
    failed: { icon: 'error', key: 'chat.statusFailed' },
    received: null,
  };

const outgoing = computed(() => props.message.direction === 'outgoing');

/** `null` for a message that is not ours and for `received`: both say nothing. */
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
