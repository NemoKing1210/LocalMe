<script setup lang="ts">
/**
 * The conversation page.
 *
 * The address is the state: `/chat/<device id>` opens that conversation and `/chat` is the
 * placeholder, so this component owns neither the selection nor the way back — it reads the
 * route, and navigating is what changes what is on screen. The peer itself still comes from the
 * store, because the store is where the host's view of the peer lives.
 *
 * What is left here is the header — which has to say something true about presence, and presence
 * is three facts, not one — and the two host calls the pane can fail on.
 */
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

/** The conversation named by the address, if it names one. */
const peerId = computed<string | null>(() => {
  const id = route.params['deviceId'];
  return typeof id === 'string' && id.length > 0 ? id : null;
});

const peer = computed(() => (peerId.value === null ? null : (peers.get(peerId.value) ?? null)));

/** Whether a way back to the list is needed, which a one-pane window is. */
const showBack = computed<boolean>(() => !wide.value);

/**
 * Presence, as one line.
 *
 * Online is the only state worth reporting positively; off it is the last time we saw them in
 * our own clock, or the fact that we never have.
 */
const subtitle = computed<string>(() => {
  const current = peer.value;
  if (current === null) return '';
  if (current.online) return i18n.t('users.online');
  if (current.lastSeenMs === null) return i18n.t('users.neverSeen');
  return i18n.t('users.lastSeen', { relative: i18n.relative(current.lastSeenMs, now.value) });
});

// Opening a conversation is two things at once: load its history, and tell the host which chat
// is on screen — that is the difference between "you are already reading this" and "raise a
// notification". One watcher, so the two cannot disagree.
watch(
  peerId,
  (deviceId) => {
    void chat.open(deviceId);
    void ipc.setActiveChat(deviceId);
  },
  { immediate: true },
);

// A conversation that no longer exists — forgotten, or removed while the window was in the
// tray — sends the user back to the list rather than leaving the placeholder on screen with a
// device id in the address bar.
watch(peer, (current) => {
  if (peerId.value !== null && current === null) void router.replace({ name: ROUTE.chat });
});

async function onSend(body: string): Promise<void> {
  try {
    await chat.send(body);
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
        <template v-if="showBack" #leading>
          <MdIconButton icon="back" :label="i18n.t('common.back')" @click="back" />
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
