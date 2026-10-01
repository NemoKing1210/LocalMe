<script setup lang="ts">
/**
 * The application root.
 *
 * It owns the facts that are about the process rather than about a page — whether the host
 * answered, whether the first-run screen is still owed, and the host calls that are about the
 * window instead of about data: the strings the tray and a notification are drawn with, and the
 * colour of the native frame — and then hands the window to the router. The pages themselves (a
 * conversation, settings) are route components, so nothing here decides which one is on screen;
 * that is the address.
 */
import { MotionConfig } from 'motion-v';
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { RouterView } from 'vue-router';

import { connectCoreEvents } from '@/app/connect';
import { signalReady } from '@/app/ready';
import { useI18n } from '@/i18n';
import * as ipc from '@/ipc';
import OnboardingView from '@/features/onboarding/OnboardingView.vue';
import { usePeerStore } from '@/stores/peers';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';
import { useTheme } from '@/theme/useTheme';
import MdSnackbar from '@/ui/MdSnackbar.vue';

const peers = usePeerStore();
const settings = useSettingsStore();
const ui = useUiStore();
const i18n = useI18n();
const theme = useTheme();

const loading = ref(true);
const failure = ref(false);

let disconnect: (() => void) | null = null;

/** Whether the first-run screen should be showing. */
const needsOnboarding = computed(() => !settings.onboarded);

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

// The native frame is drawn by the operating system, so it cannot read the custom properties the
// theme writes onto the document. The resolved pair is pushed across instead, whenever the
// palette changes — which is the accent, the mode, or a light/dark flip the system made while
// the mode is "system". An immediate run is what colours the frame before the window is shown.
watch(
  () => theme.scheme.value,
  (scheme) => {
    void ipc.setWindowAccent(scheme.primary, scheme['on-primary']);
  },
  { immediate: true },
);
</script>

<template>
  <!-- `reduced-motion="user"` is the one place the operating system's preference is applied to
       every animation in the application, including the ones inside components that never
       mention motion at all. -->
  <MotionConfig reduced-motion="user">
    <div class="app">
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

        <RouterView v-else key="shell" />
      </Transition>

      <MdSnackbar
        :open="ui.notice !== null"
        :text="ui.noticeText"
        :tone="ui.notice?.tone ?? 'neutral'"
        @dismiss="ui.dismiss"
      />
    </div>
  </MotionConfig>
</template>

<style scoped>
.app {
  display: grid;
  /* One row, exactly as tall as the container; the page inside decides how to fill it. */
  grid-template-rows: 1fr;
  height: 100%;
  background: var(--md-sys-color-surface);
  color: var(--md-sys-color-on-surface);
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

/* The first-run screen and the window proper are one page each, so the swap between them is a
   cross-fade with no travel: there is no direction to suggest. */
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
