<script setup lang="ts">
// The sweep is a background gradient moved with `background-position`, not a transform: nothing
// about the block's box changes, so a column of them cannot reflow and the effect stays on one
// composited layer. The duration is hardcoded because the `--md-sys-motion-*` tokens are zeroed
// under `prefers-reduced-motion`, which is the intended static result here (see the media query).
withDefaults(
  defineProps<{
    width?: string;
    height?: string;
    shape?: 'bubble' | 'pill' | 'text';
  }>(),
  { width: '100%', height: '1em', shape: 'text' },
);
</script>

<template>
  <span
    class="md-skeleton"
    :class="`md-skeleton--${shape}`"
    :style="{ width, height }"
    aria-hidden="true"
  />
</template>

<style scoped>
.md-skeleton {
  display: block;
  /* A translucent highlight over the container role, so one rule reads in both light and dark. */
  background-color: var(--md-sys-color-surface-container-highest);
  background-image: linear-gradient(
    90deg,
    transparent 0%,
    color-mix(in srgb, var(--md-sys-color-on-surface) 10%, transparent) 50%,
    transparent 100%
  );
  background-repeat: no-repeat;
  background-size: 200% 100%;
  background-position: 150% 0;
  animation: md-skeleton-sweep 1400ms ease-in-out infinite;
}

.md-skeleton--bubble {
  border-radius: var(--md-sys-shape-corner-large);
}

.md-skeleton--pill {
  border-radius: var(--md-sys-shape-corner-full);
}

.md-skeleton--text {
  border-radius: var(--md-sys-shape-corner-extra-small);
}

@keyframes md-skeleton-sweep {
  to {
    background-position: -50% 0;
  }
}

@media (prefers-reduced-motion: reduce) {
  .md-skeleton {
    animation: none;
  }
}
</style>
