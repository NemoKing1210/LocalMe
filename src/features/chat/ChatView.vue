<script setup lang="ts">
/**
 * The conversation pane.
 *
 * The view owns no data: the peer comes from the list, the messages from the chat store, and
 * the pane is assembled from three components that each own one band of it. What is left here
 * is the header — which has to say something true about presence, and presence is three facts,
 * not one — and the two host calls the pane can fail on.
 */
import { computed } from 'vue';

import { useNow } from '@/composables/useNow';
import { useI18n } from '@/i18n';
import type { Peer } from '@/ipc';
import { useChatStore } from '@/stores/chat';
import { useUiStore } from '@/stores/ui';
import MdEmptyState from '@/ui/MdEmptyState.vue';
import MdIconButton from '@/ui/MdIconButton.vue';
import MdTopAppBar from '@/ui/MdTopAppBar.vue';
import ChatComposer from './ChatComposer.vue';
import MessageList from './MessageList.vue';

const props = defineProps<{
  /** The open conversation, or `null` when the list has nothing selected. */
  peer: Peer | null;
  /** Whether a way back to the list is needed, which a one-pane window is. */
  showBack: boolean;
}>();

const emit = defineEmits<{ back: [] }>();

const chat = useChatStore();
const ui = useUiStore();
const i18n = useI18n();
const now = useNow();

/**
 * Presence, as one line.
 *
 * Online is the only state worth reporting positively; off it is the last time we saw them in
 * our own clock, or the fact that we never have.
 */
const subtitle = computed<string>(() => {
  const peer = props.peer;
  if (peer === null) return '';
  if (peer.online) return i18n.t('users.online');
  if (peer.lastSeenMs === null) return i18n.t('users.neverSeen');
  return i18n.t('users.lastSeen', { relative: i18n.relative(peer.lastSeenMs, now.value) });
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
          <MdIconButton icon="back" :label="i18n.t('common.back')" @click="emit('back')" />
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
