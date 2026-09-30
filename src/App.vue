<script setup lang="ts">
/**
 * The application shell.
 *
 * It owns three things and nothing else: the first-run decision, the two-pane layout, and the
 * handful of host calls that are about the window rather than about data (which conversation is
 * open, what language the tray should speak, whether the window is wide enough for two panes).
 * Every screen it shows is a feature component that reads the stores itself.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';

import { connectCoreEvents } from '@/app/connect';
import { signalReady } from '@/app/ready';
import { TWO_PANE_QUERY, useMediaQuery } from '@/composables/useMediaQuery';
import { useI18n } from '@/i18n';
import * as ipc from '@/ipc';
import ChatView from '@/features/chat/ChatView.vue';
import OnboardingView from '@/features/onboarding/OnboardingView.vue';
import SettingsView from '@/features/settings/SettingsView.vue';
import UserListView from '@/features/users/UserListView.vue';
import { useChatStore } from '@/stores/chat';
import { usePeerStore } from '@/stores/peers';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';
import MdSnackbar from '@/ui/MdSnackbar.vue';

const peers = usePeerStore();
const chat = useChatStore();
const settings = useSettingsStore();
const ui = useUiStore();
const i18n = useI18n();

const wide = useMediaQuery(TWO_PANE_QUERY);
const loading = ref(true);
const failure = ref(false);

let disconnect: (() => void) | null = null;

/** Whether the first-run screen should be showing. */
const needsOnboarding = computed(() => !settings.onboarded);

/** Whether the detail pane is showing a conversation rather than the list. */
const showDetail = computed(() => !wide.value && peers.selectedId !== null);

onMounted(async () => {
  try {
    const payload = await ipc.bootstrap();
    settings.apply(payload.settings);
    peers.replace(payload.peers);
    if (payload.storageRecovered !== null) {
      ui.notify('error.databaseRecovered', { path: payload.storageRecovered });
    }
    if (payload.discoveryProblem !== null) {
      ui.notify('error.discovery');
    }
    disconnect = await connectCoreEvents();
  } catch (error) {
    console.error('[localme] the host did not answer the bootstrap command', error);
    failure.value = true;
  } finally {
    loading.value = false;
  }
  // The host reveals the window when this arrives, so it must come after the first paint.
  await signalReady();
});

onBeforeUnmount(() => {
  disconnect?.();
});

// Selecting a person is three things at once: load the conversation, tell the host which chat is
// open — that is the difference between "you are already reading this" and "show a notification"
// — and drop the notification reminder for it. One watcher, so the three cannot disagree.
watch(
  () => peers.selectedId,
  (deviceId) => {
    void chat.open(deviceId);
    void ipc.setActiveChat(deviceId);
  },
  { immediate: true },
);

// The tray menu and a native notification are drawn outside the web view, so the host has to
// be handed the current strings whenever the language changes.
watch(
  () => i18n.locale.value,
  () => {
    void ipc.setUiLabels({
      appName: i18n.t('app.name'),
      open: i18n.t('tray.open'),
      mute: i18n.t('tray.mute'),
      unmute: i18n.t('tray.unmute'),
      quit: i18n.t('tray.quit'),
      tooltipIdle: i18n.t('tray.tooltipIdle'),
      tooltipUnread: i18n.t('tray.tooltipUnread'),
      newMessage: i18n.t('notification.newMessage'),
    });
  },
  { immediate: true },
);

/** Leaves a conversation on a narrow window, back to the list. */
function backToList(): void {
  peers.select(null);
}
</script>

<template>
  <div
    class="app"
    :data-wide="wide ? 'true' : 'false'"
    :data-detail="showDetail ? 'true' : 'false'"
  >
    <div v-if="loading" class="app__centred">
      <span class="app__spinner" aria-hidden="true" />
      <span class="md-typescale-body-medium">{{ i18n.t('common.loading') }}</span>
    </div>

    <div v-else-if="failure" class="app__centred">
      <p class="md-typescale-title-medium">{{ i18n.t('error.internal') }}</p>
      <p class="md-typescale-body-medium app__muted">{{ i18n.t('common.retry') }}</p>
    </div>

    <OnboardingView v-else-if="needsOnboarding" />

    <template v-else>
      <div class="app__list" role="navigation">
        <UserListView />
      </div>
      <main class="app__detail">
        <SettingsView v-if="ui.screen === 'settings'" @close="ui.goTo('chat')" />
        <ChatView v-else :peer="peers.selected" :show-back="!wide" @back="backToList" />
      </main>
    </template>

    <MdSnackbar
      :open="ui.notice !== null"
      :text="ui.noticeText"
      :tone="ui.notice?.tone ?? 'neutral'"
      @dismiss="ui.dismiss"
    />
  </div>
</template>

<style scoped>
.app {
  display: grid;
  /* One row, exactly as tall as the container. Without this the row is `auto`-sized, so the
     tallest pane decides the height and a long conversation stretches the shell past the
     window instead of scrolling inside it. */
  grid-template-rows: 1fr;
  height: 100%;
  background: var(--md-sys-color-surface);
  color: var(--md-sys-color-on-surface);
}

/* One pane at a time until the window is wide enough for two. */
.app[data-wide='false'] {
  grid-template-columns: 1fr;
}

.app[data-wide='true'] {
  grid-template-columns: var(--localme-list-width) 1fr;
}

.app[data-wide='false'][data-detail='true'] .app__list {
  display: none;
}

.app[data-wide='false'][data-detail='false'] .app__detail {
  display: none;
}

.app__list {
  min-width: 0;
  border-inline-end: 1px solid var(--md-sys-color-outline-variant);
  background: var(--md-sys-color-surface-container-low);
}

.app__detail {
  display: flex;
  flex-direction: column;
  min-width: 0;
  background: var(--md-sys-color-surface);
}

.app__centred {
  display: flex;
  grid-column: 1 / -1;
  gap: 12px;
  align-items: center;
  justify-content: center;
  flex-direction: column;
  color: var(--md-sys-color-on-surface-variant);
}

.app__muted {
  color: var(--md-sys-color-on-surface-variant);
}

.app__spinner {
  width: 28px;
  height: 28px;
  border: 3px solid var(--md-sys-color-primary);
  border-top-color: transparent;
  border-radius: var(--md-sys-shape-corner-full);
  animation: app-spin 800ms linear infinite;
}

@keyframes app-spin {
  to {
    transform: rotate(1turn);
  }
}

@media (prefers-reduced-motion: reduce) {
  .app__spinner {
    animation-duration: 1600ms;
  }
}
</style>
