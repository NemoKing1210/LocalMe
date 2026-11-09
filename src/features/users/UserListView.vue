<script setup lang="ts">
/** Rows are fixed-height, so the virtualiser never measures the DOM. */
import { useVirtualizer } from '@tanstack/vue-virtual';
import { computed, onBeforeUnmount, ref } from 'vue';
import { motion } from 'motion-v';
import { useRouter } from 'vue-router';

import { useEntranceWindow } from '@/composables/useEntrance';
import { ROUTE } from '@/app/routes';
import { useI18n } from '@/i18n';
import * as ipc from '@/ipc';
import type { DeviceId, Peer } from '@/ipc';
import { useChatStore } from '@/stores/chat';
import { usePeerStore } from '@/stores/peers';
import { useUiStore } from '@/stores/ui';
import MdEmptyState from '@/ui/MdEmptyState.vue';
import MdIconButton from '@/ui/MdIconButton.vue';
import MdTextField from '@/ui/MdTextField.vue';
import MdTopAppBar from '@/ui/MdTopAppBar.vue';

import ForgetDialog from './ForgetDialog.vue';
import { movePinned, pinnedDropIndex, pinnedIds } from './pin-order';
import UserListItem from './UserListItem.vue';

/** Mirrors `--localme-user-row-height`; the virtualiser needs the number, CSS needs the token. */
const ROW_HEIGHT = 72;

interface Row {
  readonly key: string;
  readonly peer: Peer;
  readonly start: number;
}

const peers = usePeerStore();
const chat = useChatStore();
const ui = useUiStore();
const i18n = useI18n();
const router = useRouter();

const scroll = ref<HTMLElement | null>(null);

const entering = useEntranceWindow();

/** The order the pointer is previewing; `null` means "use the host's order". */
const pinnedOrder = ref<readonly DeviceId[] | null>(null);

/**
 * The rows to draw. With no preview this is exactly `peers.visible`, which the host already
 * sorted; a drag replaces the pinned prefix with the order under the pointer.
 */
const ordered = computed<readonly Peer[]>(() => {
  if (pinnedOrder.value === null) return peers.visible;
  const byId = new Map(peers.visible.map((peer) => [peer.deviceId, peer]));
  const pinned = pinnedOrder.value.flatMap((id) => {
    const peer = byId.get(id);
    return peer === undefined ? [] : [peer];
  });
  return [...pinned, ...peers.visible.filter((peer) => !peer.pinned)];
});

/** A filtered list must never be turned into the persisted pinned order. */
const canReorder = computed<boolean>(() => peers.query.trim() === '');

const virtualizer = useVirtualizer(
  computed(() => ({
    count: ordered.value.length,
    getScrollElement: () => scroll.value,
    estimateSize: () => ROW_HEIGHT,
    overscan: 6,
  })),
);

const totalSize = computed<number>(() => virtualizer.value.getTotalSize());

const rows = computed<readonly Row[]>(() =>
  virtualizer.value.getVirtualItems().flatMap((item) => {
    const peer = ordered.value[item.index];
    return peer ? [{ key: peer.deviceId, peer, start: item.start }] : [];
  }),
);

function onQuery(value: string): void {
  peers.query = value;
}

function onActivate(peer: Peer): void {
  void router.push({ name: ROUTE.chat, params: { deviceId: peer.deviceId } });
}

function openSettings(): void {
  void router.push({ name: ROUTE.settings });
}

async function onMute(peer: Peer): Promise<void> {
  await ui.attempt(
    () => ipc.setPeerMuted(peer.deviceId, !peer.notifyMuted),
    'the conversation could not be muted',
  );
}

async function onPin(peer: Peer): Promise<void> {
  await ui.attempt(
    () => ipc.setPeerPinned(peer.deviceId, !peer.pinned),
    'the conversation could not be pinned',
  );
}

interface DragState {
  readonly deviceId: DeviceId;
  readonly pointerId: number;
  readonly startY: number;
  readonly listTop: number;
  active: boolean;
  lastY: number;
}

/** A few pixels of slack so a press that ends in a click never turns into a drag. */
const DRAG_THRESHOLD = 6;

const drag = ref<DragState | null>(null);
let suppressClick = false;

function stopListening(): void {
  window.removeEventListener('pointermove', onPointerMove);
  window.removeEventListener('pointerup', onPointerUp);
  window.removeEventListener('pointercancel', onPointerUp);
}

function onRowPointerDown(peer: Peer, event: PointerEvent): void {
  // Every click is preceded by its own press, so a new press retires the suppression the last
  // drop left behind. The click that ends a drag lands on the list rather than a row, so no row
  // handler sees it; clearing here is what keeps it from eating this press's click.
  suppressClick = false;
  if (event.button !== 0 || !peer.pinned || !canReorder.value) return;

  // The trailing menu button owns its own presses; a drag must not start from it.
  const target = event.target;
  if (target instanceof Element && target.closest('.md-list-item__trailing') !== null) return;

  stopListening();
  drag.value = {
    deviceId: peer.deviceId,
    pointerId: event.pointerId,
    startY: event.clientY,
    listTop: scroll.value?.getBoundingClientRect().top ?? 0,
    active: false,
    lastY: event.clientY,
  };
  window.addEventListener('pointermove', onPointerMove);
  window.addEventListener('pointerup', onPointerUp);
  window.addEventListener('pointercancel', onPointerUp);
}

function onPointerMove(event: PointerEvent): void {
  const state = drag.value;
  if (state === null || event.pointerId !== state.pointerId) return;

  if (!state.active) {
    if (Math.abs(event.clientY - state.startY) < DRAG_THRESHOLD) return;
    state.active = true;
    pinnedOrder.value = pinnedIds(ordered.value);
  }

  const order = pinnedOrder.value;
  if (order === null) return;

  event.preventDefault();
  state.lastY = event.clientY;

  const to = pinnedDropIndex(
    event.clientY,
    state.listTop,
    scroll.value?.scrollTop ?? 0,
    order.length,
    ROW_HEIGHT,
  );
  if (order.indexOf(state.deviceId) !== to) {
    pinnedOrder.value = movePinned(order, state.deviceId, to);
  }
}

function onPointerUp(): void {
  stopListening();
  const state = drag.value;
  drag.value = null;
  if (state === null || !state.active) {
    pinnedOrder.value = null;
    return;
  }

  const order = pinnedOrder.value;
  pinnedOrder.value = null;
  if (order === null) return;

  // The click that follows a drop must not open the chat.
  suppressClick = true;
  void ui.attempt(() => ipc.reorderPinnedPeers(order), 'the chat order could not be saved');
}

function onClickCapture(event: MouseEvent): void {
  if (!suppressClick) return;
  suppressClick = false;
  event.preventDefault();
  event.stopPropagation();
}

/** The dragged row follows the pointer; every other row stays on the virtualiser's `top`. */
function dragOffset(peer: Peer): string | undefined {
  const state = drag.value;
  const order = pinnedOrder.value;
  if (state === null || !state.active || order === null) return undefined;
  if (state.deviceId !== peer.deviceId) return undefined;

  const index = order.indexOf(peer.deviceId);
  if (index === -1) return undefined;
  // The row lives in the scrolled content, so its on-screen centre is the list's top minus the
  // scroll offset: the pointer has to be measured from there, not from the unscrolled top.
  const scrollTop = scroll.value?.scrollTop ?? 0;
  const centre = state.listTop - scrollTop + index * ROW_HEIGHT + ROW_HEIGHT / 2;
  return `translateY(${state.lastY - centre}px)`;
}

function onMove(peer: Peer, delta: number): void {
  if (!peer.pinned || !canReorder.value) return;

  const ids = pinnedIds(ordered.value);
  const from = ids.indexOf(peer.deviceId);
  if (from === -1) return;

  const to = from + delta;
  if (to < 0 || to >= ids.length) return;

  void ui.attempt(
    () => ipc.reorderPinnedPeers(movePinned(ids, peer.deviceId, to)),
    'the chat order could not be saved',
  );
}

onBeforeUnmount(() => {
  stopListening();
  drag.value = null;
  pinnedOrder.value = null;
  suppressClick = false;
});

const forgetTarget = ref<Peer | null>(null);

async function onForgetConfirmed(deleteHistory: boolean): Promise<void> {
  const peer = forgetTarget.value;
  forgetTarget.value = null;
  if (peer === null) return;

  const forgotten = await ui.attempt(
    () => ipc.forgetPeer(peer.deviceId, deleteHistory),
    'the device could not be forgotten',
  );
  if (!forgotten.ok) return;

  chat.clear(peer.deviceId);
  // The address pointed at a conversation that is now gone.
  if (peers.selectedId === peer.deviceId) void router.replace({ name: ROUTE.chat });
}
</script>

<template>
  <div class="users">
    <MdTopAppBar :title="i18n.t('users.title')">
      <template #trailing>
        <MdIconButton icon="tune" :label="i18n.t('settings.title')" @click="openSettings" />
      </template>
      <MdTextField
        icon="search"
        :model-value="peers.query"
        :label="i18n.t('users.searchPlaceholder')"
        :clear-label="i18n.t('users.clearSearch')"
        @update:model-value="onQuery"
      />
    </MdTopAppBar>

    <MdEmptyState
      v-if="peers.peers.length === 0"
      :title="i18n.t('users.emptyTitle')"
      :body="i18n.t('users.emptyBody')"
      icon="people"
    />
    <MdEmptyState
      v-else-if="peers.visible.length === 0"
      :title="i18n.t('users.searchEmpty', { query: peers.query })"
      icon="search"
    />

    <div v-else ref="scroll" class="users__scroll">
      <ul class="users__list" :style="{ height: `${totalSize}px` }">
        <motion.li
          v-for="(row, position) in rows"
          :key="row.key"
          class="users__row"
          :class="{
            'users__row--dragging': drag?.active === true && drag.deviceId === row.peer.deviceId,
          }"
          :style="{ top: `${row.start}px`, transform: dragOffset(row.peer) }"
          :initial="entering ? { opacity: 0 } : false"
          :animate="{ opacity: 1 }"
          :transition="{
            duration: 0.18,
            delay: Math.min(position, 8) * 0.02,
            ease: [0.2, 0, 0, 1],
          }"
          @pointerdown="onRowPointerDown(row.peer, $event)"
          @click.capture="onClickCapture"
          @keydown.alt.up.prevent="onMove(row.peer, -1)"
          @keydown.alt.down.prevent="onMove(row.peer, 1)"
        >
          <UserListItem
            :peer="row.peer"
            :selected="peers.selectedId === row.peer.deviceId"
            @activate="onActivate(row.peer)"
            @pin="onPin(row.peer)"
            @mute="onMute(row.peer)"
            @forget="forgetTarget = row.peer"
          />
        </motion.li>
      </ul>
    </div>

    <ForgetDialog
      :peer="forgetTarget"
      @close="forgetTarget = null"
      @confirmed="onForgetConfirmed"
    />
  </div>
</template>

<style scoped>
.users {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

/* The list column is one continuous surface: its colour comes from `.shell__list` and has to
   run through the app bar too, or a `surface` band would cut it off from the title bar above.
   Nothing scrolls beneath this bar — the rows scroll in the box below it. */
.users :deep(.md-top-app-bar) {
  background: transparent;
}

.users__scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.users__list {
  position: relative;
  margin: 0;
  padding: 0;
  list-style: none;
}

/* `top`, not `transform`, and opacity-only animation: a lingering transform makes each row its
   own stacking context, so the overflow menu could not paint above the rows below. Only the row
   being dragged is translated, and it is lifted with `z-index` for exactly that reason. */
.users__row {
  position: absolute;
  inset-inline: 0;
  top: 0;
  height: var(--localme-user-row-height);
  transition: top 120ms cubic-bezier(0.2, 0, 0, 1);
}

.users__row--dragging {
  z-index: 1;
  transition: none;
  cursor: grabbing;
}
</style>
