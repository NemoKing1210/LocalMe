<script setup lang="ts">
/**
 * A Material 3 indeterminate circular progress indicator.
 *
 * Two nested animations make the M3 arc: the whole ring rotates at a constant rate while the
 * dash itself grows and shrinks along the circumference. One animation alone reads as either a
 * spinning static arc (no sense of work being done at an unknown stage) or a pulsing blob (no
 * sense of direction).
 *
 * `pathLength="100"` normalises the circumference, so the dash geometry is expressed in
 * percentages of the ring and is independent of the radius. That keeps the animation correct
 * for every `size`, where hand-computed `2πr` dash values would have to be recalculated — and
 * silently mis-render — the moment the radius changed.
 *
 * Under reduced motion the indicator is slowed rather than stopped: a frozen spinner is
 * indistinguishable from a hung application, and the reduced-motion preference is about
 * vestibular comfort, not about removing the only sign that work is in progress. The token
 * sheet collapses transition durations to zero, which is why this component carries its own
 * tempo instead of reading the motion tokens.
 *
 * The ring is `currentColor`, so a caller colours it by setting `color` — the same mechanism
 * every other component in this directory uses.
 */
import { computed } from 'vue';

const props = withDefaults(
  defineProps<{
    /** Box size in pixels. */
    size?: number;
    /** What is loading; omit only when an adjacent label already says so. */
    label?: string;
  }>(),
  { size: 24, label: undefined },
);

/** The ring is drawn on the icon grid, so it aligns optically with an `MdIcon` of the same box. */
const RADIUS = 10;
const box = computed(() => `${props.size}px`);
</script>

<template>
  <span
    class="md-circular-progress"
    :style="{ width: box, height: box }"
    role="progressbar"
    :aria-label="label"
  >
    <svg class="md-circular-progress__ring" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <circle
        class="md-circular-progress__track"
        cx="12"
        cy="12"
        :r="RADIUS"
        pathLength="100"
        stroke="currentColor"
        stroke-width="2"
      />
      <circle
        class="md-circular-progress__arc"
        cx="12"
        cy="12"
        :r="RADIUS"
        pathLength="100"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
      />
    </svg>
  </span>
</template>

<style scoped>
.md-circular-progress {
  display: inline-block;
  flex: none;
  color: var(--md-sys-color-primary);
}

.md-circular-progress__ring {
  width: 100%;
  height: 100%;
  animation: md-circular-progress-rotate 1400ms linear infinite;
}

/* A faint ring shows the path the arc is travelling along, which makes the motion legible. */
.md-circular-progress__track {
  opacity: 0.24;
}

.md-circular-progress__arc {
  animation: md-circular-progress-dash 1400ms ease-in-out infinite;
}

@keyframes md-circular-progress-rotate {
  to {
    transform: rotate(1turn);
  }
}

@keyframes md-circular-progress-dash {
  0% {
    stroke-dasharray: 1 99;
    stroke-dashoffset: 0;
  }

  50% {
    stroke-dasharray: 70 30;
    stroke-dashoffset: -25;
  }

  100% {
    stroke-dasharray: 70 30;
    stroke-dashoffset: -100;
  }
}

@media (prefers-reduced-motion: reduce) {
  .md-circular-progress__ring,
  .md-circular-progress__arc {
    animation-duration: 5600ms;
  }
}
</style>
