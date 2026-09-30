<script setup lang="ts">
/**
 * The window shell: the people list beside whatever the current page is.
 *
 * It owns the layout and nothing else. The list is always mounted, because it is the navigation
 * of the application rather than a page; the detail pane is a `RouterView`, so a conversation
 * and the settings page swap in and out of it as pages. On a narrow window only one of the two
 * is visible, and which one is decided by whether the current route *is* a detail route —
 * which is the difference between the old `screen` flag and this: the address is the state.
 *
 * The selected peer is mirrored into the peer store here, from the route, so the list's
 * highlight has one writer and cannot disagree with the address bar.
 */
import { computed, watch } from 'vue';
import { AnimatePresence, motion } from 'motion-v';
import { RouterView, useRoute } from 'vue-router';

import { TWO_PANE_QUERY, useMediaQuery } from '@/composables/useMediaQuery';
import UserListView from '@/features/users/UserListView.vue';
import { ROUTE } from '@/app/routes';
import { usePeerStore } from '@/stores/peers';

const route = useRoute();
const peers = usePeerStore();
const wide = useMediaQuery(TWO_PANE_QUERY);

/** The conversation in the address, if the address names one. */
const chatPeerId = computed<string | null>(() => {
  const id = route.params['deviceId'];
  return typeof id === 'string' && id.length > 0 ? id : null;
});

/** Whether the detail pane is holding something the user asked for. */
const hasDetail = computed<boolean>(
  () => route.name === ROUTE.settings || chatPeerId.value !== null,
);

/**
 * A page's identity for the transition.
 *
 * A conversation is keyed by the peer, because moving between two conversations *is* a page
 * change: the log, the header and the composer all belong to the peer in the address, and the
 * reader should see one arrive as the other leaves. Within a conversation the key is stable, so
 * a message arriving — or the day heading moving — never remounts anything.
 */
const page = computed<string>(() => {
  if (route.name === ROUTE.settings) return 'settings';
  return chatPeerId.value === null ? 'chat' : `chat:${chatPeerId.value}`;
});

watch(
  chatPeerId,
  (deviceId) => {
    peers.select(deviceId);
  },
  { immediate: true },
);
</script>

<template>
  <div
    class="shell"
    :data-wide="wide ? 'true' : 'false'"
    :data-detail="hasDetail ? 'true' : 'false'"
  >
    <div class="shell__list" role="navigation">
      <UserListView />
    </div>

    <main class="shell__detail">
      <RouterView v-slot="{ Component }">
        <AnimatePresence mode="wait">
          <motion.div
            :key="page"
            class="shell__page"
            :initial="{ opacity: 0, x: 16 }"
            :animate="{ opacity: 1, x: 0 }"
            :exit="{ opacity: 0, x: -16 }"
            :transition="{ duration: 0.18, ease: [0.2, 0, 0, 1] }"
          >
            <component :is="Component" />
          </motion.div>
        </AnimatePresence>
      </RouterView>
    </main>
  </div>
</template>

<style scoped>
.shell {
  display: grid;
  /* One row, exactly as tall as the container. Without this the row is `auto`-sized, so the
     tallest pane decides the height and a long conversation stretches the shell past the
     window instead of scrolling inside it. */
  grid-template-rows: 1fr;
  height: 100%;
  min-height: 0;
}

/* One pane at a time until the window is wide enough for two. */
.shell[data-wide='false'] {
  grid-template-columns: 1fr;
}

.shell[data-wide='true'] {
  grid-template-columns: var(--localme-list-width) 1fr;
}

.shell[data-wide='false'][data-detail='true'] .shell__list {
  display: none;
}

.shell[data-wide='false'][data-detail='false'] .shell__detail {
  display: none;
}

.shell__list {
  min-width: 0;
  border-inline-end: 1px solid var(--md-sys-color-outline-variant);
  background: var(--md-sys-color-surface-container-low);
}

.shell__detail {
  position: relative;
  display: flex;
  flex-direction: column;
  min-width: 0;
  background: var(--md-sys-color-surface);
  /* A page slides in from the side, so for the length of the transition part of it sits outside
     the pane. Without this clip that part paints over the list column beside it — the detail
     pane is a later sibling and therefore the one that wins. */
  overflow: hidden;
}

/*
 * The animated wrapper has to fill the pane: the pages inside it are laid out with the pane's
 * height (a message log that scrolls, a settings list that scrolls), so a wrapper that sizes to
 * its content would push the whole layout open instead.
 */
.shell__page {
  display: flex;
  flex: 1;
  min-height: 0;
  flex-direction: column;
}
</style>
