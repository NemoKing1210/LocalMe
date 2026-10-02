<script setup lang="ts">
import { computed, ref } from 'vue';
import { AnimatePresence, motion } from 'motion-v';

import { useI18n, type MessageKey } from '@/i18n';
import type { Message, MessageStatus } from '@/ipc';
import { useChatStore } from '@/stores/chat';
import { useUiStore } from '@/stores/ui';
import MdIcon from '@/ui/MdIcon.vue';
import MdMenu from '@/ui/MdMenu.vue';
import type { IconName } from '@/ui/icons';

const props = defineProps<{
  message: Message;
  showStatus: boolean;
}>();

const i18n = useI18n();
const chat = useChatStore();
const ui = useUiStore();

// Asked once: a message animates only on first arrival, not when scrolled back into view.
const entrance = chat.consumeEntrance(props.message.id);

/** A record so a new host status is a compile error here, not a silently blank bubble. */
const GLYPHS: Record<MessageStatus, { readonly icon: IconName; readonly key: MessageKey } | null> =
  {
    queued: { icon: 'clock', key: 'chat.statusQueued' },
    sending: { icon: 'clock', key: 'chat.statusSending' },
    delivered: { icon: 'check-all', key: 'chat.statusDelivered' },
    received: null,
  };

/**
 * A LAN round trip is milliseconds. A message that took longer than this really waited in the
 * outbox, and only then is the second date — when it actually arrived — worth showing.
 */
const DELAYED_AFTER_MS = 5_000;

const outgoing = computed(() => props.message.direction === 'outgoing');

/** `null` for a message that is not ours and for `received`: both say nothing. */
const status = computed<{ readonly icon: IconName; readonly label: string } | null>(() => {
  if (!props.showStatus || props.message.direction !== 'outgoing') return null;
  const glyph = GLYPHS[props.message.status];
  return glyph === null ? null : { icon: glyph.icon, label: i18n.t(glyph.key) };
});

/** The moment the recipient acknowledged this message, when it had to wait for it. */
const deliveredAt = computed<number | null>(() => {
  const at = props.message.deliveredAt;
  if (!outgoing.value || at === null) return null;
  return at - props.message.sentAt >= DELAYED_AFTER_MS ? at : null;
});

const deliveredLabel = computed(() =>
  deliveredAt.value === null
    ? ''
    : i18n.t('chat.deliveredAt', { time: i18n.clock(deliveredAt.value) }),
);

interface BubbleMenuAction {
  readonly id: string;
  readonly label: string;
  readonly icon: IconName;
}

const menu = ref<InstanceType<typeof MdMenu> | null>(null);
/**
 * The selection as it was when the menu was opened. Read from here rather than when the item is
 * chosen: pressing a menu item moves focus, and reading the live selection afterwards would be
 * reading whatever is left of it.
 */
const menuSelection = ref('');

const menuItems = computed<readonly BubbleMenuAction[]>(() => {
  const copy: BubbleMenuAction = {
    id: 'message',
    label: i18n.t('chat.copyMessage'),
    icon: 'copy',
  };
  return menuSelection.value.length === 0
    ? [copy]
    : [{ id: 'selection', label: i18n.t('chat.copySelection'), icon: 'copy' }, copy];
});

/** The current selection, but only when it both starts and ends inside this bubble. */
function selectionWithin(host: EventTarget | null): string {
  const selection = window.getSelection();
  if (selection === null || selection.isCollapsed) return '';
  if (!(host instanceof HTMLElement)) return '';
  const { anchorNode, focusNode } = selection;
  if (anchorNode === null || focusNode === null) return '';
  if (!host.contains(anchorNode) || !host.contains(focusNode)) return '';
  return selection.toString();
}

/**
 * The right button's own default action is prevented in the template: it would move focus to the
 * pressed element *after* this handler runs, taking it back from the item `MdMenu` focused, and
 * it would drop the selection the reader made.
 */
function onContextMenu(event: MouseEvent): void {
  menuSelection.value = selectionWithin(event.currentTarget);
  menu.value?.show({ x: event.clientX, y: event.clientY });
}

async function copy(text: string): Promise<void> {
  try {
    // The webview's own clipboard — the one the Ctrl+C shortcut writes to — so the two paths
    // cannot disagree about what "copied" means.
    await navigator.clipboard.writeText(text);
    ui.notify('chat.copied');
  } catch (error) {
    console.error('[localme] the text could not be copied', error);
    ui.fail('error.internal');
  }
}

function onMenuSelect(id: string): void {
  void copy(id === 'selection' ? menuSelection.value : props.message.body);
}
</script>

<template>
  <motion.article
    class="bubble"
    :class="outgoing ? 'bubble--outgoing' : 'bubble--incoming'"
    :initial="entrance ? { opacity: 0, y: 12, scale: 0.98 } : false"
    :animate="{ opacity: 1, y: 0, scale: 1 }"
    :transition="{ type: 'spring', stiffness: 420, damping: 34 }"
    @mousedown.right.prevent
    @contextmenu.prevent="onContextMenu"
  >
    <p class="md-typescale-body-medium bubble__body" data-selectable>{{ message.body }}</p>
    <footer class="md-typescale-label-small bubble__meta">
      <span class="bubble__time">{{ i18n.clock(message.sentAt) }}</span>
      <template v-if="deliveredAt !== null">
        <span class="bubble__arrow" aria-hidden="true">→</span>
        <span class="bubble__time" :title="deliveredLabel">{{ i18n.clock(deliveredAt) }}</span>
      </template>
      <AnimatePresence mode="wait">
        <motion.span
          v-if="status"
          :key="status.icon"
          class="bubble__status"
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

  <!--
    A sibling of the bubble, not a child: the entrance animation puts a transform on the bubble,
    and a transformed ancestor would place the `fixed` menu from the bubble's origin instead of
    the window's. The wrapper is `display: contents`, so it takes no room in the row.
  -->
  <MdMenu
    ref="menu"
    :trigger="false"
    :items="menuItems"
    :label="i18n.t('chat.messageActions')"
    @select="onMenuSelect"
  />
</template>

<style scoped>
.bubble {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-width: min(560px, 75%);
  padding: 8px 14px 6px;
  border-radius: var(--md-sys-shape-corner-large);
  /*
   * The whole bubble, not only the text: a drag that starts in the 14px padding — the easiest
   * place to aim at — has to begin a selection, and a press inside a `user-select: none` box
   * starts nothing at all. The meta line below opts back out.
   */
  user-select: text;
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
  /* The I-beam is the only thing that tells the reader the text can be selected. */
  cursor: text;
}

/* The time and the delivery glyph are metadata: a drag over them must not copy them out. */
.bubble__meta {
  display: flex;
  gap: 4px;
  align-items: center;
  align-self: flex-end;
  color: var(--md-sys-color-on-surface-variant);
  user-select: none;
}

.bubble__time {
  font-variant-numeric: tabular-nums;
}

.bubble__arrow {
  opacity: 0.65;
}

.bubble__status {
  display: inline-flex;
  align-items: center;
}
</style>
