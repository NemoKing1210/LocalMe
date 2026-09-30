<script setup lang="ts">
/**
 * The message log.
 *
 * The virtualiser owns measurement and the translate transforms; everything else in this file
 * is the scroll policy, which is the part a chat log needs and a generic list does not:
 *
 *  * the log follows the tail, but only while the reader is already near it. A message that
 *    arrives while they are reading history must not drag the page out from under them;
 *  * "load earlier messages" sits above the log, and a page loaded there must not move the
 *    reader: a prepend shifts every position below it, so the offset is corrected once the list
 *    has grown rather than when it was asked for;
 *  * the day heading is drawn above the first message of its day and pinned to the top of the
 *    viewport while that day is on screen, from the same measurements the rows are placed with.
 */
import { computed, nextTick, onMounted, ref, watch, type ComponentPublicInstance } from 'vue';
import { useVirtualizer } from '@tanstack/vue-virtual';

import { useNow } from '@/composables/useNow';
import { dayKey, useI18n } from '@/i18n';
import type { Message, Peer } from '@/ipc';
import { useChatStore } from '@/stores/chat';
import MdButton from '@/ui/MdButton.vue';
import MdCircularProgress from '@/ui/MdCircularProgress.vue';
import MdIconButton from '@/ui/MdIconButton.vue';
import MessageBubble from './MessageBubble.vue';

defineProps<{
  /** The conversation on screen. */
  peer: Peer;
}>();

const emit = defineEmits<{ loadOlder: [] }>();

const chat = useChatStore();
const i18n = useI18n();
const now = useNow();

/** One row of the log: a message, and where it sits among the days. */
interface LogRow {
  readonly message: Message;
  /** Index into `days`. */
  readonly day: number;
  /** Whether this message opens its day and therefore carries its heading. */
  readonly startsDay: boolean;
}

/** One local calendar day of messages. */
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
/** Whether the log is following the tail, which is what the "jump to latest" button negates. */
const following = ref(true);

/**
 * Timestamp first, identifier second.
 *
 * This is the total order the conversation is stored in and the one the rest of the application
 * speaks about it in, so the log is rendered in it whatever order the messages arrived in.
 */
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

/** The rendered rows paired with their data, so the template never indexes twice. */
const visible = computed(() =>
  virtualItems.value.map((item) => ({ item, row: rows.value[item.index] })),
);

/** Where each rendered row starts, by index: the day heading is positioned from these. */
const rowStarts = computed<ReadonlyMap<number, number>>(() => {
  const starts = new Map<number, number>();
  for (const item of virtualItems.value) starts.set(item.index, item.start);
  return starts;
});

/** The day of the topmost row that is not hidden behind the top strip. */
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

/** The heading sits above its day's first message until that scrolls past, then it pins. */
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

/**
 * Puts the end of the log at the bottom of the viewport.
 *
 * Written as "as far as the content goes" rather than as a computed offset: the tail of the
 * list is exactly the part whose height is still an estimate, so its offset is only knowable
 * from the browser. The call is idempotent, which is what makes it safe to repeat.
 */
function pinToEnd(): void {
  const element = scrollElement.value;
  if (element === null) return;
  element.scrollTop = element.scrollHeight;
}

/**
 * Keeps the reader on the message they were reading after a page is prepended above it.
 *
 * A prepend shifts every offset below it, and the correction can only be made once the DOM has
 * grown: written before that, it is clamped to the old scroll range and the view jumps to the
 * newly loaded page instead of staying put.
 */
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

/** Asks for the page above, remembering where the reader was first. */
function loadOlder(): void {
  const element = scrollElement.value;
  prependMark = element === null ? null : { top: element.scrollTop, height: element.scrollHeight };
  emit('loadOlder');
}

/**
 * Whether the log sits close enough to its end that a change to it should keep it there.
 *
 * Measured on the element rather than from the list's own idea of its height: the two are one
 * render apart while a message is being laid out, and the element is what the reader sees —
 * and what the "step to the end" below leaves exactly at zero.
 */
function atEnd(element: HTMLDivElement): boolean {
  return element.scrollHeight - element.scrollTop - element.clientHeight <= FOLLOW_THRESHOLD_PX;
}

function onScroll(): void {
  const element = scrollElement.value;
  if (element === null) return;
  scrollTop.value = element.scrollTop;
  following.value = atEnd(element);
}

/** Re-engages the tail and goes there; the whole point of the button that calls it. */
function jumpToLatest(): void {
  following.value = true;
  pinToEnd();
}

/** The oldest loaded message, which is how a page loaded at the top is recognised. */
let oldestId: string | null = null;

watch(
  () => chat.messages,
  (next) => {
    /*
     * Where the reader is is read here, before the list has grown to fit the new messages: a
     * new message must follow the tail only if the tail was in view when it arrived, and a
     * scroll event that has not been delivered yet would answer that question too late.
     */
    const element = scrollElement.value;
    if (element !== null) following.value = atEnd(element);

    const old = rows.value[0]?.message.id ?? null;
    const moved = old !== oldestId;
    oldestId = old;

    if (next.length === 0) {
      // The conversation was closed or replaced: the next one starts at its tail.
      following.value = true;
      prependMark = null;
      return;
    }
    // A page that arrived above the reader is the one change that must not move them; a
    // message that arrives below them is handled by following the tail.
    if (moved) restoreAfterPrepend();
  },
);

/*
 * Following the tail is a reaction to the list's height, not to the arrival of a message.
 * Every row that is measured moves the end of the log, so a single scroll when a message
 * arrives lands short of the bottom — and doing this per change is also what keeps the tail in
 * view while the first page is still settling. Nothing happens once the reader has scrolled
 * away, which is the whole point of the flag.
 */
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
          :style="{ transform: `translateY(${entry.item.start}px)` }"
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

    <div v-if="chat.loading && rows.length === 0" class="list__loading">
      <MdCircularProgress :label="i18n.t('common.loading')" />
    </div>

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

.list__loading {
  inset-block: 0;
  align-items: center;
}

.list__latest {
  inset-block-end: 16px;
  justify-content: flex-end;
  padding-inline-end: 16px;
}
</style>
