<script setup lang="ts">
// The options are real `<input type="radio">` sharing one generated `name`, so the browser owns
// grouping, arrow-key traversal and the one-checked-per-group invariant.
import { useId } from 'vue';

defineProps<{
  modelValue: string;
  /** The group's accessible name; not drawn. */
  label: string;
  options: readonly {
    readonly value: string;
    readonly label: string;
    readonly description?: string;
  }[];
}>();

const emit = defineEmits<{ 'update:modelValue': [value: string] }>();

const name = useId();
</script>

<template>
  <div class="md-radio-group" role="radiogroup" :aria-label="label">
    <label
      v-for="option in options"
      :key="option.value"
      class="md-radio-group__option md-state-layer"
    >
      <input
        class="md-radio-group__input md-visually-hidden"
        type="radio"
        :name="name"
        :value="option.value"
        :checked="option.value === modelValue"
        @change="emit('update:modelValue', option.value)"
      />
      <span
        class="md-radio-group__indicator"
        :class="{ 'md-radio-group__indicator--selected': option.value === modelValue }"
        aria-hidden="true"
      />
      <span class="md-radio-group__text">
        <span class="md-typescale-body-large">{{ option.label }}</span>
        <span v-if="option.description" class="md-typescale-body-small md-radio-group__description">
          {{ option.description }}
        </span>
      </span>
    </label>
  </div>
</template>

<style scoped>
.md-radio-group {
  display: flex;
  flex-direction: column;
}

.md-radio-group__option {
  display: flex;
  gap: 12px;
  align-items: center;
  min-height: 48px;
  padding-inline: 8px;
  border-radius: var(--md-sys-shape-corner-extra-small);
  color: var(--md-sys-color-on-surface);
}

/* The focus ring is drawn on the indicator: the input is visually hidden, so its own outline would be invisible. */
.md-radio-group__input:focus-visible + .md-radio-group__indicator {
  outline: 2px solid var(--md-sys-color-primary);
  outline-offset: 2px;
}

.md-radio-group__indicator {
  display: grid;
  flex: none;
  place-items: center;
  width: 20px;
  height: 20px;
  border: 2px solid var(--md-sys-color-on-surface-variant);
  border-radius: var(--md-sys-shape-corner-full);
}

.md-radio-group__indicator::after {
  content: '';
  width: 10px;
  height: 10px;
  border-radius: var(--md-sys-shape-corner-full);
  background: var(--md-sys-color-primary);
  transform: scale(0);
  transition: transform var(--md-sys-motion-duration-short3) var(--md-sys-motion-easing-standard);
}

.md-radio-group__indicator--selected {
  border-color: var(--md-sys-color-primary);
}

.md-radio-group__indicator--selected::after {
  transform: scale(1);
}

.md-radio-group__text {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.md-radio-group__description {
  color: var(--md-sys-color-on-surface-variant);
}
</style>
