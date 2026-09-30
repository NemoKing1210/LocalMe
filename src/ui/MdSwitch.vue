<script setup lang="ts">
/**
 * A Material 3 switch, with its label and supporting text.
 *
 * Only the label and the track are clickable, not the whole settings row: a row that also holds
 * a button would otherwise make the switch impossible to hit without triggering the other
 * control.
 */
import { useId } from 'vue';

withDefaults(
  defineProps<{
    /** Whether the switch is on. */
    modelValue: boolean;
    /** The label. */
    label: string;
    /** A sentence explaining the consequence of the setting. */
    supportingText?: string;
    /** Whether the switch is unavailable. */
    disabled?: boolean;
  }>(),
  { disabled: false },
);

const emit = defineEmits<{ 'update:modelValue': [value: boolean] }>();

const controlId = useId();
</script>

<template>
  <div class="md-switch" :class="{ 'md-switch--disabled': disabled }">
    <div class="md-switch__text">
      <span class="md-typescale-body-large">{{ label }}</span>
      <span v-if="supportingText" class="md-typescale-body-medium md-switch__supporting">
        {{ supportingText }}
      </span>
    </div>
    <button
      :id="controlId"
      class="md-switch__control"
      type="button"
      role="switch"
      :aria-checked="modelValue"
      :aria-label="label"
      :disabled="disabled"
      @click="emit('update:modelValue', !modelValue)"
    >
      <span class="md-switch__track" :class="{ 'md-switch__track--on': modelValue }">
        <span class="md-switch__thumb" :class="{ 'md-switch__thumb--on': modelValue }" />
      </span>
    </button>
  </div>
</template>

<style scoped>
.md-switch {
  display: flex;
  gap: 16px;
  align-items: center;
  justify-content: space-between;
  min-height: 56px;
}

.md-switch--disabled {
  opacity: var(--md-sys-state-disabled-opacity);
}

.md-switch__text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.md-switch__supporting {
  color: var(--md-sys-color-on-surface-variant);
}

.md-switch__control {
  flex: none;
  padding: 4px;
  border-radius: var(--md-sys-shape-corner-full);
}

.md-switch__track {
  display: block;
  position: relative;
  width: 52px;
  height: 32px;
  border: 2px solid var(--md-sys-color-outline);
  border-radius: var(--md-sys-shape-corner-full);
  background: var(--md-sys-color-surface-container-highest);
  transition:
    background-color var(--md-sys-motion-duration-short3) var(--md-sys-motion-easing-standard),
    border-color var(--md-sys-motion-duration-short3) var(--md-sys-motion-easing-standard);
}

.md-switch__track--on {
  border-color: var(--md-sys-color-primary);
  background: var(--md-sys-color-primary);
}

.md-switch__thumb {
  position: absolute;
  top: 50%;
  inset-inline-start: 6px;
  width: 16px;
  height: 16px;
  border-radius: var(--md-sys-shape-corner-full);
  background: var(--md-sys-color-outline);
  transform: translateY(-50%);
  transition:
    inset-inline-start var(--md-sys-motion-duration-medium1) var(--md-sys-motion-easing-emphasized),
    width var(--md-sys-motion-duration-medium1) var(--md-sys-motion-easing-emphasized),
    height var(--md-sys-motion-duration-medium1) var(--md-sys-motion-easing-emphasized),
    background-color var(--md-sys-motion-duration-short3) var(--md-sys-motion-easing-standard);
}

.md-switch__thumb--on {
  inset-inline-start: 22px;
  width: 24px;
  height: 24px;
  background: var(--md-sys-color-on-primary);
}
</style>
