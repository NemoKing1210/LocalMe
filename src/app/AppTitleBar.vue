<script setup lang="ts">
/**
 * The window's title bar, drawn by the interface instead of the operating system.
 *
 * The host removes the native frame on Windows (see `window::apply_frame`) and says so in the
 * bootstrap payload, so this component is only ever mounted when there is no system title bar
 * to duplicate. Every control is a call into `@/ipc`, which is the one place the window API is
 * spoken to.
 */
import { onBeforeUnmount, onMounted, ref } from 'vue';

import { useI18n } from '@/i18n';
import * as ipc from '@/ipc';
import MdIcon from '@/ui/MdIcon.vue';

/** How long after the last resize event the maximize state is re-read. */
const RESIZE_SETTLE_MS = 80;

const i18n = useI18n();
const maximized = ref(false);

let stopResize: (() => void) | null = null;
let settle: ReturnType<typeof setTimeout> | null = null;
let disposed = false;

async function syncMaximized(): Promise<void> {
  try {
    maximized.value = await ipc.isWindowMaximized();
  } catch {
    // There is no host to ask in a browser preview; the frame is already square there.
  }
}

/** A drag-resize fires continuously, so the state is read once the window has settled. */
function scheduleSync(): void {
  if (settle !== null) clearTimeout(settle);
  settle = setTimeout(() => {
    settle = null;
    void syncMaximized();
  }, RESIZE_SETTLE_MS);
}

/**
 * The window operations cannot fail in a way the user could act on, and a browser has none.
 * `then` also catches the synchronous throw `getCurrentWindow` makes when there is no host.
 */
function run(action: () => Promise<void>): void {
  void Promise.resolve()
    .then(action)
    .catch(() => {});
}

onMounted(async () => {
  await syncMaximized();
  try {
    const stop = await ipc.onWindowResized(scheduleSync);
    if (disposed) stop();
    else stopResize = stop;
  } catch {
    // No host: the bar is drawn but the controls are inert.
  }
});

onBeforeUnmount(() => {
  disposed = true;
  stopResize?.();
  if (settle !== null) clearTimeout(settle);
});

function minimize(): void {
  run(ipc.minimizeWindow);
}

function toggleMaximize(): void {
  run(ipc.toggleMaximizeWindow);
}

function close(): void {
  run(ipc.closeWindow);
}
</script>

<template>
  <!-- `deep` makes the whole strip draggable; the buttons stop that on their own, because
       Tauri's drag-region script treats every `button` as interactive. Double-clicks on it are
       the platform's maximise/restore gesture, handled by the same script. -->
  <header class="title-bar" data-tauri-drag-region="deep">
    <div class="title-bar__controls">
      <button
        class="title-bar__button"
        type="button"
        :aria-label="i18n.t('window.minimize')"
        :title="i18n.t('window.minimize')"
        @click="minimize"
      >
        <MdIcon name="minimize" :size="16" :stroke-width="1.6" />
      </button>
      <button
        class="title-bar__button"
        type="button"
        :aria-label="i18n.t(maximized ? 'window.restore' : 'window.maximize')"
        :title="i18n.t(maximized ? 'window.restore' : 'window.maximize')"
        @click="toggleMaximize"
      >
        <MdIcon :name="maximized ? 'restore' : 'maximize'" :size="16" :stroke-width="1.6" />
      </button>
      <button
        class="title-bar__button title-bar__button--close"
        type="button"
        :aria-label="i18n.t('window.close')"
        :title="i18n.t('window.close')"
        @click="close"
      >
        <MdIcon name="close" :size="16" :stroke-width="1.6" />
      </button>
    </div>
  </header>
</template>

<style scoped>
.title-bar {
  display: flex;
  flex: none;
  align-items: center;
  height: var(--localme-title-bar-height);
  background: var(--md-sys-color-surface);
  color: var(--md-sys-color-on-surface);
}

.title-bar__controls {
  display: flex;
  flex: none;
  gap: 2px;
  align-items: center;
  margin-inline-start: auto;
  padding-inline: 4px;
}

.title-bar__button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border-radius: var(--md-sys-shape-corner-full);
  color: var(--md-sys-color-on-surface-variant);
  transition:
    background-color var(--md-sys-motion-duration-short2) var(--md-sys-motion-easing-standard),
    color var(--md-sys-motion-duration-short2) var(--md-sys-motion-easing-standard);
}

.title-bar__button:hover {
  background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
  color: var(--md-sys-color-on-surface);
}

.title-bar__button:active {
  background: color-mix(in srgb, var(--md-sys-color-on-surface) 12%, transparent);
}

.title-bar__button--close:hover {
  background: var(--md-sys-color-error);
  color: var(--md-sys-color-on-error);
}

.title-bar__button--close:active {
  background: color-mix(in srgb, var(--md-sys-color-error) 88%, var(--md-sys-color-on-error));
}
</style>
