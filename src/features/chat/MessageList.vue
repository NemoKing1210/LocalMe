<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch, type ComponentPublicInstance } from 'vue';
import { useVirtualizer } from '@tanstack/vue-virtual';

import { useNow } from '@/composables/useNow';
import { dayKey, useI18n } from '@/i18n';
import type { Message, Peer } from '@/ipc';
import { useChatStore } from '@/stores/chat';
import MdButton from '@/ui/MdButton.vue';
import MdIconButton from '@/ui/MdIconButton.vue';
import MessageBubble from './MessageBubble.vue';
import MessageSkeleton from './MessageSkeleton.vue';

defineProps<{
  peer: Peer;
}>();

const emit = defineEmits<{ loadOlder: [] }>();

const chat = useChatStore();
const i18n = useI18n();
const now = useNow();

interface LogRow {
  readonly message: Message;
  /** Index into `days`. */
  readonly day: number;
  readonly startsDay: boolean;
}

interface LogDay {
  readonly first: number;
  readonly sentAt: number;
}

/** How much room the top of the log keeps for the "load earlier" control. */
const TOP_STRIP_PX = 48;
/** The distance from the bottom within which the log keeps following the tail. */
const FOLLOW_THRESHOLD_PX = 120;
/** A row's height before it has been measured. Close enough to keep the first paint quiet. */
const ROW_ESTIMATE_PX = 64;
/** Rows rendered beyond the window, so a flick of the wheel does not outrun the renderer. */
const OVERSCAN = 8;

const scrollElement = ref<HTMLDivElement | null>(null);
/** The scroll offset, tracked natively: the virtualiser only notifies when its window moves. */
const scrollTop = ref(0);
const following = ref(true);

// Timestamp then id: the total order the conversation is stored in.
function byTime(left: Message, right: Message): number {
  return left.sentAt === right.sentAt
    ? left.id.localeCompare(right.id)
    : left.sentAt - right.sentAt;
}

const layout = computed<{ readonly rows: readonly LogRow[]; readonly days: readonly LogDay[] }>(
  () => {
    const rows: LogRow[] = [];
    const days: LogDay[] = [];
    let current: string | null = null;
    for (const message of [...chat.messages].sort(byTime)) {
      const key = dayKey(message.sentAt);
      const startsDay = key !== current;
      if (startsDay) {
        current = key;
        days.push({ first: rows.length, sentAt: message.sentAt });
      }
      rows.push({ message, day: days.length - 1, startsDay });
    }
    return { rows, days };
  },
);

const rows = computed(() => layout.value.rows);

const virtualizer = useVirtualizer(
  computed(() => ({
    count: rows.value.length,
    getScrollElement: () => scrollElement.value,
    // Keyed by message id, not by index: a page prepended at the top shifts every index, and
    // the anchor that keeps the reader in place is followed by key.
    getItemKey: (index: number) => rows.value[index]?.message.id ?? index,
    estimateSize: () => ROW_ESTIMATE_PX,
    overscan: OVERSCAN,
    paddingStart: TOP_STRIP_PX,
  })),
);

const virtualItems = computed(() => virtualizer.value.getVirtualItems());
const totalSize = computed(() => virtualizer.value.getTotalSize());

const visible = computed(() =>
  virtualItems.value.map((item) => ({ item, row: rows.value[item.index] })),
);

const rowStarts = computed<ReadonlyMap<number, number>>(() => {
  const starts = new Map<number, number>();
  for (const item of virtualItems.value) starts.set(item.index, item.start);
  return starts;
});

const activeDay = computed<LogDay | null>(() => {
  const top = scrollTop.value + TOP_STRIP_PX;
  for (const item of virtualItems.value) {
    if (item.end > top) return layout.value.days[rows.value[item.index]?.day ?? -1] ?? null;
  }
  return null;
});

const dayHeading = computed<string | null>(() => {
  const day = activeDay.value;
  return day === null ? null : i18n.dayHeading(day.sentAt, now.value);
});

// Pins to the top once the day's first message scrolls past.
const dayOffset = computed(() => {
  const day = activeDay.value;
  if (day === null) return 0;
  const start = rowStarts.value.get(day.first);
  const pinned = scrollTop.value + TOP_STRIP_PX;
  return start === undefined ? pinned : Math.max(start, pinned);
});

function measureRow(element: Element | ComponentPublicInstance | null): void {
  if (element instanceof HTMLElement) virtualizer.value.measureElement(element);
}

/** Where the reader was when they asked for an earlier page. */
let prependMark: { readonly top: number; readonly height: number } | null = null;

// "As far as the content goes" rather than a computed offset: the tail's height is still an
// estimate. Idempotent, so it is safe to repeat.
function pinToEnd(): void {
  const element = scrollElement.value;
  if (element === null) return;
  element.scrollTop = element.scrollHeight;
}

// A prepend shifts every offset below it; the correction must wait for the DOM to grow, or it
// is clamped to the old scroll range and the view jumps to the new page.
function restoreAfterPrepend(): void {
  const mark = prependMark;
  prependMark = null;
  if (mark === null) return;
  void nextTick(() => {
    const element = scrollElement.value;
    if (element === null) return;
    element.scrollTop = mark.top + (element.scrollHeight - mark.height);
  });
}

function loadOlder(): void {
  const element = scrollElement.value;
  prependMark = element === null ? null : { top: element.scrollTop, height: element.scrollHeight };
  emit('loadOlder');
}

// Measured on the element, not the list's own height: the two are one render apart while a
// message is being laid out.
function atEnd(element: HTMLDivElement): boolean {
  return element.scrollHeight - element.scrollTop - element.clientHeight <= FOLLOW_THRESHOLD_PX;
}

function onScroll(): void {
  const element = scrollElement.value;
  if (element === null) return;
  scrollTop.value = element.scrollTop;
  following.value = atEnd(element);
}

function jumpToLatest(): void {
  following.value = true;
  pinToEnd();
}

let oldestId: string | null = null;

watch(
  () => chat.messages,
  (next) => {
    // Read before the list grows: a pending scroll event would answer "was the tail in view?"
    // too late.
    const element = scrollElement.value;
    if (element !== null) following.value = atEnd(element);

    const old = rows.value[0]?.message.id ?? null;
    const moved = old !== oldestId;
    oldestId = old;

    if (next.length === 0) {
      following.value = true;
      prependMark = null;
      return;
    }
    // A page prepended above must not move the reader; an arriving message is handled by
    // following the tail.
    if (moved) restoreAfterPrepend();
  },
);

// Follows the tail on every height change, not per message: each measured row moves the end, so
// a single scroll on arrival lands short of the bottom.
watch(
  totalSize,
  () => {
    if (following.value) pinToEnd();
  },
  { flush: 'post' },
);

onMounted(() => {
  if (rows.value.length > 0) void nextTick(pinToEnd);
});
</script>

<template>
  <div class="list">
    <div
      ref="scrollElement"
      class="list__scroll"
      role="log"
      :aria-label="i18n.t('chat.messageList', { name: peer.nickname })"
      @scroll="onScroll"
    >
      <div class="list__content" :style="{ height: `${totalSize}px` }">
        <p v-if="!chat.hasMore && rows.length > 0" class="md-typescale-label-small list__end">
          {{ i18n.t('chat.historyEnd') }}
        </p>

        <div
          v-if="dayHeading !== null"
          class="list__day"
          :style="{ transform: `translateY(${dayOffset}px)` }"
        >
          <span class="md-typescale-label-small list__day-chip">{{ dayHeading }}</span>
        </div>

        <div
          v-for="entry in visible"
          :key="entry.row ? entry.row.message.id : entry.item.index"
          :ref="measureRow"
          :data-index="entry.item.index"
          class="list__row"
          :class="{ 'list__row--day': entry.row?.startsDay === true }"
          :style="{ top: `${entry.item.start}px` }"
        >
          <MessageBubble v-if="entry.row" :message="entry.row.message" :show-status="true" />
        </div>
      </div>
    </div>

    <div v-if="chat.hasMore" class="list__older">
      <MdButton icon="refresh" variant="tonal" :busy="chat.loading" @click="loadOlder">
        {{ i18n.t('chat.loadOlder') }}
      </MdButton>
    </div>

    <MessageSkeleton v-if="chat.loading && rows.length === 0" class="list__loading" />

    <div v-if="!following" class="list__latest">
      <MdIconButton
        icon="chevron-down"
        variant="filled"
        :label="i18n.t('chat.jumpToLatest')"
        @click="jumpToLatest"
      />
    </div>
  </div>
</template>

<style scoped>
.list {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  position: relative;
}

.list__scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  /*
   * The bubbles are absolutely positioned against `.list__content`, so a gutter put on the
   * content itself would sit inside their containing block and change nothing; it has to be on
   * the scroll box. Left and right match the composer's gutter, so a message lines up with the
   * field under it instead of touching the edge.
   */
  padding-inline: 16px;
  /* The list positions itself from its own measurements; the browser's own anchoring would
     fight it, moving the view whenever a row above the fold changes height. */
  overflow-anchor: none;
  /*
   * Size containment: how tall the log is comes from the shell above it, never from the
   * messages inside it. Without this the log's own height feeds back into the pane's, and a
   * long conversation pushes the whole window layout open instead of scrolling in place.
   */
  contain: size;
}

.list__content {
  position: relative;
  width: 100%;
}

.list__end {
  position: absolute;
  inset-block-start: 0;
  inset-inline: 0;
  padding-block: 15px;
  text-align: center;
  color: var(--md-sys-color-on-surface-variant);
}

/*
 * The day heading is positioned from the same measurements as the rows rather than left to
 * `position: sticky`, which cannot escape its row in a virtualised list. It takes no pointer
 * events: it is a label, and the message under it has to stay selectable.
 */
.list__day {
  position: absolute;
  inset-block-start: 0;
  inset-inline: 0;
  z-index: 2;
  display: flex;
  justify-content: center;
  pointer-events: none;
}

.list__day-chip {
  padding: 2px 10px;
  border-radius: var(--md-sys-shape-corner-full);
  background: var(--md-sys-color-surface-container);
  color: var(--md-sys-color-on-surface-variant);
}

/* `top`, not `transform`, as in the people list: a transform would make every row its own
   containing block, so the message's context menu — painted `fixed` — would be placed from the
   row's origin instead of the window's. */
.list__row {
  position: absolute;
  inset-inline: 0;
  top: 0;
  display: flex;
  flex-direction: column;
  padding-block: 2px;
}

.list__row--day {
  padding-block-start: 28px;
}

.list__older,
.list__loading,
.list__latest {
  position: absolute;
  inset-inline: 0;
  z-index: 3;
  display: flex;
  justify-content: center;
}

.list__older {
  inset-block-start: 8px;
}

/* The placeholder is laid out like the log itself — from the top, whole width — rather than
   centred like the spinner it replaced, so the first real page does not move anything. */
.list__loading {
  inset-block-start: 0;
  flex-direction: column;
  overflow: hidden;
}

.list__latest {
  inset-block-end: 16px;
  justify-content: flex-end;
  padding-inline-end: 16px;
}
</style>
