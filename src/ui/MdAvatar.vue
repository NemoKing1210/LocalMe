<script setup lang="ts">
// The drawing is generated from the seed the peer announced, never anything derived locally: two
// machines must render the same peer identically. The image is decorative and `aria-hidden`,
// since the nickname always sits next to it.
import { computed } from 'vue';

import { blobatarUri } from 'blobatar/uri';
// The scoped `@blobatar/vue` of the next major is not installed; on 2.x this subpath is the same
// component and is frozen, and every adapter needs this stylesheet to move at all.
import { Blobatar } from 'blobatar/vue';
import 'blobatar/motion.css';

const props = withDefaults(
  defineProps<{
    /** The seed announced by the peer. */
    seed: string;
    name: string;
    size?: number;
    dimmed?: boolean;
    presence?: 'online' | 'offline' | null;
    /**
     * `'never'` (the default) is the static `<img>` a list needs; `'always'` is for the one avatar
     * on screen, and `'hover'` animates a single short-list row at a time.
     */
    animate?: 'never' | 'hover' | 'always';
  }>(),
  { size: 40, dimmed: false, presence: null, animate: 'never' },
);

const CACHE_LIMIT = 400;
const cache = new Map<string, string>();

function avatarUri(seed: string, size: number): string {
  const key = `${seed}|${size}`;
  const memoised = cache.get(key);
  if (memoised !== undefined) return memoised;

  if (cache.size >= CACHE_LIMIT) cache.clear();
  const uri = blobatarUri(seed, { background: 'squircle', size });
  cache.set(key, uri);
  return uri;
}

const uri = computed(() => avatarUri(props.seed, props.size));
/** The adapter has no "off": off is the `<img>` branch, so only the two on-modes reach it. */
const animation = computed<'hover' | 'always'>(() =>
  props.animate === 'always' ? 'always' : 'hover',
);
const box = computed(() => `${props.size}px`);
/** Keeps the presence dot proportional, so a small avatar is not dwarfed by its marker. */
const dot = computed(() => `${Math.max(8, Math.round(props.size / 3.2))}px`);
</script>

<template>
  <span
    class="md-avatar"
    :class="{ 'md-avatar--dimmed': dimmed }"
    :style="{ width: box, height: box }"
    :title="name"
    aria-hidden="true"
  >
    <Blobatar
      v-if="animate !== 'never'"
      class="md-avatar__image"
      :name="seed"
      :size="size"
      background="squircle"
      :animate="animation"
    />
    <img v-else class="md-avatar__image" :src="uri" alt="" />
    <span
      v-if="presence !== null"
      class="md-avatar__presence"
      :class="`md-avatar__presence--${presence}`"
      :style="{ width: dot, height: dot }"
    />
  </span>
</template>

<style scoped>
/* The frame is blobatar's `squircle` backdrop, not a CSS clip — nothing here may clip. */
.md-avatar {
  position: relative;
  display: block;
  flex: none;
}

.md-avatar__image {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.md-avatar--dimmed .md-avatar__image {
  filter: grayscale(1);
  opacity: var(--md-sys-state-disabled-opacity);
}

.md-avatar__presence {
  position: absolute;
  inset-block-end: 0;
  inset-inline-end: 0;
  border: 2px solid var(--md-sys-color-surface);
  border-radius: var(--md-sys-shape-corner-full);
  transition: background-color var(--md-sys-motion-duration-medium1)
    var(--md-sys-motion-easing-standard);
}

.md-avatar__presence--online {
  background: var(--md-sys-color-primary);
}

.md-avatar__presence--offline {
  background: var(--md-sys-color-outline);
}
</style>
