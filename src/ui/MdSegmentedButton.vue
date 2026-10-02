<script setup lang="ts">
// Selection follows focus, as the M3 spec asks of a single-select segmented button — the opposite
// of the radio-group behaviour elsewhere in this design system.
//
// The segments are unrounded and the container clips them, so two arcs do not meet in a seam.
import { ref } from 'vue';

import MdIcon from './MdIcon.vue';
import type { IconName } from './icons';

const props = defineProps<{
  modelValue: string;
  options: readonly {
    readonly value: string;
    readonly label: string;
    readonly icon?: IconName;
  }[];
}>();

const emit = defineEmits<{ 'update:modelValue': [value: string] }>();

const group = ref<HTMLElement | null>(null);

function focusSegment(value: string): void {
  const segments = group.value?.querySelectorAll<HTMLButtonElement>('[role="radio"]');
  if (!segments) return;
  for (const segment of segments) {
    if (segment.dataset.value === value) segment.focus();
  }
}

function select(value: string): void {
  if (value === props.modelValue) return;
  emit('update:modelValue', value);
}

function onKeydown(event: KeyboardEvent): void {
  const current = props.options.findIndex((option) => option.value === props.modelValue);
  if (current < 0) return;

  let next: number;
  switch (event.key) {
    case 'ArrowRight':
    case 'ArrowDown':
      next = (current + 1) % props.options.length;
      break;
    case 'ArrowLeft':
    case 'ArrowUp':
      next = (current - 1 + props.options.length) % props.options.length;
      break;
    case 'Home':
      next = 0;
      break;
    case 'End':
      next = props.options.length - 1;
      break;
    default:
      return;
  }

  const option = props.options[next];
  if (option === undefined) return;

  event.preventDefault();
  emit('update:modelValue', option.value);
  focusSegment(option.value);
}
</script>

<template>
  <div
    ref="group"
    class="md-segmented md-typescale-label-large"
    role="radiogroup"
    @keydown="onKeydown"
  >
    <button
      v-for="option in options"
      :key="option.value"
      class="md-segmented__segment md-state-layer"
      :class="{ 'md-segmented__segment--selected': option.value === modelValue }"
      type="button"
      role="radio"
      :data-value="option.value"
      :aria-checked="option.value === modelValue"
      :tabindex="option.value === modelValue ? 0 : -1"
      @click="select(option.value)"
    >
      <MdIcon v-if="option.icon" :name="option.icon" :size="18" />
      <span class="md-segmented__label">{{ option.label }}</span>
    </button>
  </div>
</template>

<style scoped>
.md-segmented {
  display: inline-flex;
  overflow: hidden;
  border: 1px solid var(--md-sys-color-outline);
  border-radius: var(--md-sys-shape-corner-full);
}

.md-segmented__segment {
  display: flex;
  gap: 8px;
  align-items: center;
  justify-content: center;
  height: 40px;
  padding: 0 16px;
  color: var(--md-sys-color-on-surface);
  transition: background-color var(--md-sys-motion-duration-short3)
    var(--md-sys-motion-easing-standard);
}

/* The divider between two segments is the outline, drawn on the segment that follows. */
.md-segmented__segment + .md-segmented__segment {
  border-inline-start: 1px solid var(--md-sys-color-outline);
}

.md-segmented__segment--selected {
  background: var(--md-sys-color-secondary-container);
  color: var(--md-sys-color-on-secondary-container);
}

.md-segmented__label {
  white-space: nowrap;
}
</style>
