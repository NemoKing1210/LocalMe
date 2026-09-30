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

import { useI18n, type MessageKey } from '@/i18n';
import type { Message, MessageStatus } from '@/ipc';
import MdIcon from '@/ui/MdIcon.vue';
import type { IconName } from '@/ui/icons';

const props = defineProps<{
  /** The message to draw. */
  message: Message;
  /** Whether delivery progress belongs beside the time. History does not need it. */
  showStatus: boolean;
}>();

const i18n = useI18n();

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
  <article class="bubble" :class="outgoing ? 'bubble--outgoing' : 'bubble--incoming'">
    <p class="md-typescale-body-medium bubble__body" data-selectable>{{ message.body }}</p>
    <footer class="md-typescale-label-small bubble__meta">
      <span class="bubble__time">{{ i18n.clock(message.sentAt) }}</span>
      <MdIcon
        v-if="status"
        class="bubble__status"
        :class="{ 'bubble__status--failed': message.status === 'failed' }"
        :name="status.icon"
        :size="14"
        :title="status.label"
      />
    </footer>
  </article>
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

.bubble__status--failed {
  color: var(--md-sys-color-error);
}
</style>
