<script setup lang="ts">
/** Rows are fixed-height, so the virtualiser never measures the DOM. */
import { useVirtualizer } from '@tanstack/vue-virtual';
import { computed, ref } from 'vue';
import { motion } from 'motion-v';
import { useRouter } from 'vue-router';

import { useEntranceWindow } from '@/composables/useEntrance';
import { ROUTE } from '@/app/routes';
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
const router = useRouter();

const scroll = ref<HTMLElement | null>(null);

const entering = useEntranceWindow();

const virtualizer = useVirtualizer(
  computed(() => ({
    count: peers.visible.length,
    getScrollElement: () => scroll.value,
    estimateSize: () => ROW_HEIGHT,
    overscan: 6,
  })),
);

const totalSize = computed<number>(() => virtualizer.value.getTotalSize());

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
  void router.push({ name: ROUTE.chat, params: { deviceId: peer.deviceId } });
}

function openSettings(): void {
  void router.push({ name: ROUTE.settings });
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
          :style="{ top: `${row.start}px` }"
          :initial="entering ? { opacity: 0 } : false"
          :animate="{ opacity: 1 }"
          :transition="{
            duration: 0.18,
            delay: Math.min(position, 8) * 0.02,
            ease: [0.2, 0, 0, 1],
          }"
        >
          <UserListItem
            :peer="row.peer"
            :selected="peers.selectedId === row.peer.deviceId"
            @activate="onActivate(row.peer)"
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
   own stacking context, so the overflow menu could not paint above the rows below. */
.users__row {
  position: absolute;
  inset-inline: 0;
  top: 0;
  height: var(--localme-user-row-height);
}
</style>
