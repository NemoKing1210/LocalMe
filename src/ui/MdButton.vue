<script setup lang="ts">
/**
 * A Material 3 button.
 *
 * Every variant is the same box: 40 units tall, a fully rounded corner, the label style, 24
 * units of horizontal padding, and a state layer drawn over the container. The variants differ
 * only in which colour roles they use, which is why they share one implementation — five
 * near-identical components would be five places for the corner radius to be different.
 */
import { computed } from 'vue';

import MdIcon from './MdIcon.vue';
import type { IconName } from './icons';

const props = withDefaults(
  defineProps<{
    /** Which colour role the button takes. */
    variant?: 'filled' | 'tonal' | 'outlined' | 'text' | 'elevated';
    /** An optional leading icon. */
    icon?: IconName;
    /** Whether the button is unavailable. */
    disabled?: boolean;
    /** Whether the label is replaced by a spinner while an action is running. */
    busy?: boolean;
    /** The native button type; only `submit` behaves differently inside a form. */
    type?: 'button' | 'submit' | 'reset';
  }>(),
  { variant: 'filled', disabled: false, busy: false, type: 'button' },
);

const classes = computed(() => [`md-button--${props.variant}`]);
</script>

<template>
  <button
    class="md-button md-state-layer md-typescale-label-large"
    :class="classes"
    :type="type"
    :disabled="disabled || busy"
    :aria-busy="busy"
  >
    <MdIcon v-if="icon && !busy" :name="icon" :size="18" />
    <span v-if="busy" class="md-button__spinner" aria-hidden="true" />
    <span class="md-button__label"><slot /></span>
  </button>
</template>

<style scoped>
.md-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  height: 40px;
  padding: 0 24px;
  border-radius: var(--md-sys-shape-corner-full);
  transition:
    background-color var(--md-sys-motion-duration-short3) var(--md-sys-motion-easing-standard),
    box-shadow var(--md-sys-motion-duration-short3) var(--md-sys-motion-easing-standard);
}

.md-button:has(.md-button__spinner) {
  padding-inline-start: 16px;
}

.md-button:disabled {
  opacity: var(--md-sys-state-disabled-opacity);
}

.md-button__label {
  white-space: nowrap;
}

.md-button__spinner {
  width: 18px;
  height: 18px;
  border: 2px solid currentcolor;
  border-top-color: transparent;
  border-radius: var(--md-sys-shape-corner-full);
  animation: md-button-spin 700ms linear infinite;
}

@keyframes md-button-spin {
  to {
    transform: rotate(1turn);
  }
}

.md-button--filled {
  background: var(--md-sys-color-primary);
  color: var(--md-sys-color-on-primary);
}

.md-button--filled:hover {
  box-shadow: var(--md-sys-elevation-level1);
}

.md-button--tonal {
  background: var(--md-sys-color-secondary-container);
  color: var(--md-sys-color-on-secondary-container);
}

.md-button--elevated {
  background: var(--md-sys-color-surface-container-low);
  color: var(--md-sys-color-primary);
  box-shadow: var(--md-sys-elevation-level1);
}

.md-button--elevated:hover {
  box-shadow: var(--md-sys-elevation-level2);
}

.md-button--outlined {
  border: 1px solid var(--md-sys-color-outline);
  color: var(--md-sys-color-primary);
}

.md-button--text {
  padding: 0 12px;
  color: var(--md-sys-color-primary);
}
</style>
