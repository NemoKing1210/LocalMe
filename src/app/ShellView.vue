<script setup lang="ts">
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

const chatPeerId = computed<string | null>(() => {
  const id = route.params['deviceId'];
  return typeof id === 'string' && id.length > 0 ? id : null;
});

const hasDetail = computed<boolean>(
  () => route.name === ROUTE.settings || chatPeerId.value !== null,
);

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
  /* A grid item's automatic minimum size is its content, so the single `1fr` row would stretch
     to the height of the whole people list — thousands of pixels for a long one — and the list
     column would hang past the window with nothing to scroll. Clipping the column and zeroing
     its minimum is what lets the row stay one window tall and hands the overflow to the
     `overflow-y: auto` inside `UserListView`. `.shell__detail` needs the same treatment for the
     same reason (it also holds a scrolling page). */
  min-height: 0;
  overflow: hidden;
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

.shell__page {
  display: flex;
  flex: 1;
  min-height: 0;
  flex-direction: column;
}
</style>
