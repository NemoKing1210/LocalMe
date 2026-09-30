<script setup lang="ts">
/**
 * A shimmering placeholder block.
 *
 * The block is deliberately dumb — a box, a shape and a sweep — because every skeleton in the
 * application is a different *arrangement* of boxes, and the arrangement belongs to the screen
 * that knows what is arriving (see `MessageSkeleton`).
 *
 * The sweep is a background gradient moved with `background-position`, not a transform and not
 * an animated opacity: nothing about the block's box changes, so a column of them cannot cause
 * a reflow, and the whole effect stays on one composited layer.
 *
 * The animation carries its own tempo rather than reading `--md-sys-motion-duration-*`, because
 * the token sheet zeroes those under `prefers-reduced-motion`, and a placeholder whose sweep has
 * stopped is exactly what a reduced-motion user should get — a static block, not a strobe. That
 * override is declared below.
 */
withDefaults(
  defineProps<{
    /** Any CSS length; a percentage of the row is the usual choice. */
    width?: string;
    /** Any CSS length. */
    height?: string;
    /** Which corner radius belongs on this block. */
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
  /* The gradient is a translucent highlight over the container role, so the same rule reads
     correctly in light and dark without a second token pair. */
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
