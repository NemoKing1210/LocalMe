<script setup lang="ts">
import { computed } from 'vue';

import MdIcon from './MdIcon.vue';
import type { IconName } from './icons';

const props = withDefaults(
  defineProps<{
    icon: IconName;
    label: string;
    variant?: 'standard' | 'filled' | 'tonal';
    disabled?: boolean;
    selected?: boolean;
    size?: 'small' | 'medium';
  }>(),
  { variant: 'standard', disabled: false, selected: false, size: 'medium' },
);

const classes = computed(() => [
  `md-icon-button--${props.variant}`,
  `md-icon-button--${props.size}`,
  { 'md-icon-button--selected': props.selected },
]);

const iconSize = computed(() => (props.size === 'small' ? 18 : 22));
</script>

<template>
  <button
    class="md-icon-button md-state-layer"
    :class="classes"
    type="button"
    :disabled="disabled"
    :aria-label="label"
    :title="label"
    :aria-pressed="selected ? 'true' : undefined"
  >
    <MdIcon :name="icon" :size="iconSize" />
  </button>
</template>

<style scoped>
.md-icon-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--md-sys-shape-corner-full);
  color: var(--md-sys-color-on-surface-variant);
  transition: background-color var(--md-sys-motion-duration-short3)
    var(--md-sys-motion-easing-standard);
}

.md-icon-button--medium {
  width: 40px;
  height: 40px;
}

.md-icon-button--small {
  width: 32px;
  height: 32px;
}

.md-icon-button:disabled {
  opacity: var(--md-sys-state-disabled-opacity);
}

.md-icon-button--filled {
  background: var(--md-sys-color-primary);
  color: var(--md-sys-color-on-primary);
}

.md-icon-button--tonal {
  background: var(--md-sys-color-secondary-container);
  color: var(--md-sys-color-on-secondary-container);
}

.md-icon-button--selected {
  color: var(--md-sys-color-primary);
}
</style>
