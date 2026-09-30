<script setup lang="ts">
/**
 * Renders one icon from the table in `icons.ts`.
 *
 * Sizing is the font size: an icon is designed on a 24-unit grid and stroked at 2 units, which
 * is the Material ratio, so scaling the box scales the stroke with it and the optical weight
 * stays constant across the interface.
 */
import { computed } from 'vue';

import { ICONS, type IconName } from './icons';

const props = withDefaults(
  defineProps<{
    /** Which icon to draw. */
    name: IconName;
    /** Box size in pixels. */
    size?: number;
    /** Stroke width on the 24-unit grid. */
    strokeWidth?: number;
  }>(),
  { size: 24, strokeWidth: 2 },
);

const shapes = computed(() => ICONS[props.name]);
</script>

<template>
  <svg
    class="md-icon"
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    :stroke-width="strokeWidth"
    stroke="currentColor"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    focusable="false"
  >
    <template v-for="(shape, index) in shapes" :key="index">
      <path v-if="shape.kind === 'path'" :d="shape.d" />
      <line
        v-else-if="shape.kind === 'line'"
        :x1="shape.x1"
        :y1="shape.y1"
        :x2="shape.x2"
        :y2="shape.y2"
      />
      <polyline v-else-if="shape.kind === 'polyline'" :points="shape.points" />
      <circle v-else-if="shape.kind === 'circle'" :cx="shape.cx" :cy="shape.cy" :r="shape.r" />
      <rect
        v-else-if="shape.kind === 'rect'"
        :x="shape.x"
        :y="shape.y"
        :width="shape.width"
        :height="shape.height"
        :rx="shape.rx"
      />
      <circle v-else :cx="shape.cx" :cy="shape.cy" :r="shape.r" fill="currentColor" stroke="none" />
    </template>
  </svg>
</template>

<style scoped>
.md-icon {
  display: block;
  flex: none;
}
</style>
