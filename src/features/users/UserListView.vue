<script setup lang="ts">
/**
 * The people pane: a search field, a virtualised list, and the two per-person actions that do
 * not open a conversation (mute, forget).
 *
 * The list is virtualised with fixed-size rows rather than measured ones. Every row is exactly
 * `--localme-user-row-height`, so the row's start offset is `index * height` and the virtualiser
 * never has to measure the DOM — which in turn means scrolling cannot invalidate a measurement
 * and there is no reflow between a row entering the viewport and being drawn.
 *
 * Emptying the list has two meanings and they are answered differently: "nobody on the network"
 * explains discovery, while "nobody matches what you typed" names the search that hid everyone.
 */
import { useVirtualizer } from '@tanstack/vue-virtual';
import { computed, ref } from 'vue';

import { useI18n } from '@/i18n';
import * as ipc from '@/ipc';
import type { Peer } from '@/ipc';
import { useChatStore } from '@/stores/chat';
import { usePeerStore } from '@/stores/peers';
import { useUiStore } from '@/stores/ui';
import MdEmptyState from '@/ui/MdEmptyState.vue';
import MdIconButton from '@/ui/MdIconButton.vue';
import MdTextField from '@/ui/MdTextField.vue';
import MdTopAppBar from '@/ui/MdTopAppBar.vue';

import ForgetDialog from './ForgetDialog.vue';
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

const scroll = ref<HTMLElement | null>(null);

const virtualizer = useVirtualizer(
  computed(() => ({
    count: peers.visible.length,
    getScrollElement: () => scroll.value,
    estimateSize: () => ROW_HEIGHT,
    overscan: 6,
  })),
);

const totalSize = computed<number>(() => virtualizer.value.getTotalSize());

/** Only the rows in (and near) the viewport, paired with the peer each one stands for. */
const rows = computed<readonly Row[]>(() =>
  virtualizer.value.getVirtualItems().flatMap((item) => {
    const peer = peers.visible[item.index];
    return peer ? [{ key: peer.deviceId, peer, start: item.start }] : [];
  }),
);

function onQuery(value: string): void {
  peers.query = value;
}

function onActivate(peer: Peer): void {
  peers.select(peer.deviceId);
}

async function onMute(peer: Peer): Promise<void> {
  try {
    await ipc.setPeerMuted(peer.deviceId, !peer.notifyMuted);
  } catch {
    ui.fail('error.internal');
  }
}

const forgetTarget = ref<Peer | null>(null);

async function onForgetConfirmed(deleteHistory: boolean): Promise<void> {
  const peer = forgetTarget.value;
  forgetTarget.value = null;
  if (peer === null) return;

  try {
    await ipc.forgetPeer(peer.deviceId, deleteHistory);
  } catch {
    ui.fail('error.internal');
    return;
  }

  chat.clear(peer.deviceId);
  if (peers.selectedId === peer.deviceId) peers.select(null);
}
</script>

<template>
  <div class="users">
    <MdTopAppBar :title="i18n.t('users.title')">
      <template #trailing>
        <MdIconButton
          icon="settings"
          :label="i18n.t('settings.title')"
          @click="ui.goTo('settings')"
        />
      </template>
      <MdTextField
        icon="search"
        :model-value="peers.query"
        :label="i18n.t('users.searchPlaceholder')"
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
        <li
          v-for="row in rows"
          :key="row.key"
          class="users__row"
          :style="{ top: `${row.start}px` }"
        >
          <UserListItem
            :peer="row.peer"
            :selected="peers.selectedId === row.peer.deviceId"
            @activate="onActivate(row.peer)"
            @mute="onMute(row.peer)"
            @forget="forgetTarget = row.peer"
          />
        </li>
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

/* Rows are absolutely positioned because their order in the DOM is the order of the visible
   slice, not the order of the list. `top` rather than `transform: translateY` places them at
   their offset on purpose: a transform would make every row its own stacking context, and the
   overflow menu opened from a row could then never paint above the rows below it. */
.users__row {
  position: absolute;
  inset-inline: 0;
  top: 0;
  height: var(--localme-user-row-height);
}
</style>
