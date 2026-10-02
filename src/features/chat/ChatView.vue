<script setup lang="ts">
import { computed, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';

import { ROUTE } from '@/app/routes';
import { TWO_PANE_QUERY, useMediaQuery } from '@/composables/useMediaQuery';
import { useNow } from '@/composables/useNow';
import { useI18n } from '@/i18n';
import * as ipc from '@/ipc';
import { useChatStore } from '@/stores/chat';
import { usePeerStore } from '@/stores/peers';
import { useUiStore } from '@/stores/ui';
import MdAvatar from '@/ui/MdAvatar.vue';
import MdEmptyState from '@/ui/MdEmptyState.vue';
import MdIconButton from '@/ui/MdIconButton.vue';
import MdTopAppBar from '@/ui/MdTopAppBar.vue';

import ChatComposer from './ChatComposer.vue';
import MessageList from './MessageList.vue';

const route = useRoute();
const router = useRouter();
const chat = useChatStore();
const peers = usePeerStore();
const ui = useUiStore();
const i18n = useI18n();
const now = useNow();

const wide = useMediaQuery(TWO_PANE_QUERY);

const peerId = computed<string | null>(() => {
  const id = route.params['deviceId'];
  return typeof id === 'string' && id.length > 0 ? id : null;
});

const peer = computed(() => (peerId.value === null ? null : (peers.get(peerId.value) ?? null)));

const showBack = computed<boolean>(() => !wide.value);

const subtitle = computed<string>(() => {
  const current = peer.value;
  if (current === null) return '';
  if (current.online) return i18n.t('users.online');
  if (current.lastSeenMs === null) return i18n.t('users.neverSeen');
  return i18n.t('users.lastSeen', { relative: i18n.relative(current.lastSeenMs, now.value) });
});

// One watcher for both: `setActiveChat` tells the host which chat is on screen, which must not
// disagree with the history load.
watch(
  peerId,
  (deviceId) => {
    void chat.open(deviceId);
    void ipc.setActiveChat(deviceId);
  },
  { immediate: true },
);

watch(peer, (current) => {
  if (peerId.value !== null && current === null) void router.replace({ name: ROUTE.chat });
});

async function onSend(body: string, files: readonly string[]): Promise<void> {
  try {
    await chat.send(body, files);
  } catch (error) {
    console.error('[localme] the message could not be sent', error);
    ui.fail('error.network');
  }
}

async function onLoadOlder(): Promise<void> {
  try {
    await chat.loadOlder();
  } catch (error) {
    console.error('[localme] the earlier messages could not be read', error);
    ui.fail('error.storage');
  }
}

function back(): void {
  void router.push({ name: ROUTE.chat });
}
</script>

<template>
  <section class="chat">
    <MdEmptyState
      v-if="peer === null"
      class="chat__empty"
      icon="chat"
      :title="i18n.t('chat.emptyTitle')"
      :body="i18n.t('chat.emptyBody')"
    />

    <template v-else>
      <MdTopAppBar :title="peer.nickname" :subtitle="subtitle">
        <template #leading>
          <MdIconButton v-if="showBack" icon="back" :label="i18n.t('common.back')" @click="back" />
          <MdAvatar
            :seed="peer.avatarSeed"
            :name="peer.nickname"
            :size="40"
            :dimmed="!peer.online"
            :presence="peer.online ? 'online' : 'offline'"
            animate="always"
          />
        </template>
      </MdTopAppBar>

      <MessageList :peer="peer" @load-older="onLoadOlder" />
      <ChatComposer :peer="peer" :sending="chat.sending" @send="onSend" />
    </template>
  </section>
</template>

<style scoped>
.chat {
  display: flex;
  flex: 1;
  min-height: 0;
  flex-direction: column;
}

.chat__empty {
  flex: 1;
}
</style>
