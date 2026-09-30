<script setup lang="ts">
/**
 * A Material 3 radio group.
 *
 * Each option is a real `<input type="radio">` sharing one generated `name`, visually hidden
 * inside its label. The browser then owns grouping, arrow-key traversal and the "one checked
 * per group" invariant, all of which a `role="radio"` reimplementation would have to reproduce
 * by hand. The indicator is drawn as a sibling of the input because a native radio cannot be
 * restyled to M3's ring-and-dot without `appearance: none` on a control whose focus ring and
 * hit area are then also gone.
 *
 * `label` names the group for assistive technology and is not drawn: every caller in this
 * application already has a visible section heading above the group, and a second one inside
 * the control would be a duplicate line of text.
 */
import { useId } from 'vue';

defineProps<{
  /** The `value` of the selected option. */
  modelValue: string;
  /** The group's accessible name. */
  label: string;
  /** The options, in order. */
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

/*
 * The focus ring lives on the indicator, not on the input: the input is clipped away, so the
 * browser's own outline would be drawn where nobody can see it.
 */
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
