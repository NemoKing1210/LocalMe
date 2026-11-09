<script setup lang="ts">
import { MotionConfig } from 'motion-v';
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { RouterView } from 'vue-router';

import AppTitleBar from '@/app/AppTitleBar.vue';
import { connectCoreEvents } from '@/app/connect';
import { reportError } from '@/app/errors';
import { signalReady } from '@/app/ready';
import { TWO_PANE_QUERY, useMediaQuery } from '@/composables/useMediaQuery';
import { useI18n } from '@/i18n';
import * as ipc from '@/ipc';
import OnboardingView from '@/features/onboarding/OnboardingView.vue';
import { usePeerStore } from '@/stores/peers';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';
import MdSnackbar from '@/ui/MdSnackbar.vue';

const peers = usePeerStore();
const settings = useSettingsStore();
const ui = useUiStore();
const i18n = useI18n();

const loading = ref(true);
const failure = ref(false);
/**
 * Whether the host removed the native frame, so this interface has to draw the title bar.
 * `true` until the answer arrives: the window is only revealed after the bootstrap, so the
 * value never paints on screen, and a failed bootstrap still leaves a bar with a close button.
 */
const customTitlebar = ref(true);

let disconnect: (() => void) | null = null;

const needsOnboarding = computed(() => !settings.onboarded);

const wide = useMediaQuery(TWO_PANE_QUERY);
const showsShell = computed<boolean>(
  () => !loading.value && !failure.value && !needsOnboarding.value,
);

/**
 * In the two-pane shell the bar is drawn inside the detail column, above the page, so the list
 * column on its left runs all the way to the window's top edge. Everywhere else — the loading
 * and error screens, onboarding, and the one-pane layout — the bar is a row of its own.
 */
const barInShell = computed<boolean>(() => customTitlebar.value && showsShell.value && wide.value);
const appTitleBar = computed<boolean>(() => customTitlebar.value && !barInShell.value);

onMounted(async () => {
  try {
    const payload = await ipc.bootstrap();
    customTitlebar.value = payload.customTitlebar;
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
    // Reported by hand: the global handlers never see this one, and a bootstrap that failed is
    // the first thing to look at in the log when the interface shows an error screen.
    reportError({
      scope: 'startup',
      message: 'the host did not answer the bootstrap command',
      cause: error,
    });
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

// The tray menu and a native notification are drawn outside the web view, so the host has to
// be handed the current strings whenever the language changes.
watch(
  () => i18n.locale.value,
  () => {
    void ipc.setUiLabels({
      appName: i18n.t('app.name'),
      open: i18n.t('tray.open'),
      quit: i18n.t('tray.quit'),
      tooltipIdle: i18n.t('tray.tooltipIdle'),
      tooltipUnread: i18n.t('tray.tooltipUnread'),
      newMessage: i18n.t('notification.newMessage'),
      conversations: i18n.t('tray.conversations'),
      allConversations: i18n.t('tray.allConversations'),
      markAllRead: i18n.t('tray.markAllRead'),
      notifications: i18n.t('tray.notifications'),
      closeToTray: i18n.t('tray.closeToTray'),
      autostart: i18n.t('tray.autostart'),
      settings: i18n.t('tray.settings'),
      openLogs: i18n.t('tray.openLogs'),
    });
  },
  { immediate: true },
);
</script>

<template>
  <!-- `reduced-motion="user"` is the one place the operating system's preference is applied to
       every animation in the application, including the ones inside components that never
       mention motion at all. -->
  <MotionConfig reduced-motion="user">
    <div class="app" :data-title-bar="appTitleBar ? 'true' : 'false'">
      <AppTitleBar v-if="appTitleBar" />

      <Transition name="app-swap" mode="out-in">
        <div v-if="loading" key="loading" class="app__centred">
          <span class="app__spinner" aria-hidden="true" />
          <span class="md-typescale-body-medium">{{ i18n.t('common.loading') }}</span>
        </div>

        <div v-else-if="failure" key="failure" class="app__centred">
          <p class="md-typescale-title-medium">{{ i18n.t('error.internal') }}</p>
          <p class="md-typescale-body-medium app__muted">{{ i18n.t('common.retry') }}</p>
        </div>

        <OnboardingView v-else-if="needsOnboarding" key="onboarding" />

        <!-- The shell draws the bar itself when the layout has a list column to reach the top. -->
        <RouterView v-else key="shell" v-slot="{ Component }">
          <component :is="Component" :custom-titlebar="customTitlebar" />
        </RouterView>
      </Transition>

      <MdSnackbar
        :open="ui.notice !== null"
        :text="ui.noticeText"
        :tone="ui.notice?.tone ?? 'neutral'"
        :action="i18n.t('common.dismiss')"
        @dismiss="ui.dismiss"
      />
    </div>
  </MotionConfig>
</template>

<style scoped>
.app {
  display: grid;
  height: 100%;
  background: var(--md-sys-color-surface);
  color: var(--md-sys-color-on-surface);
}

/* The title bar is a row of its own only when it is drawn; without it the page has the whole
   window, and an unconditional `auto 1fr` would leave it auto-sized in an empty second row. */
.app[data-title-bar='true'] {
  grid-template-rows: auto minmax(0, 1fr);
}

.app[data-title-bar='false'] {
  grid-template-rows: minmax(0, 1fr);
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

.app-swap-enter-active,
.app-swap-leave-active {
  transition: opacity var(--md-sys-motion-duration-medium2) var(--md-sys-motion-easing-standard);
}

.app-swap-enter-from,
.app-swap-leave-to {
  opacity: 0;
}

@media (prefers-reduced-motion: reduce) {
  .app__spinner {
    animation-duration: 1600ms;
  }
}
</style>
