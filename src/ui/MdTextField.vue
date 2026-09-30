<script setup lang="ts">
/**
 * A Material 3 text field.
 *
 * The filled variant, with a floating label. The label is a real `<label>` bound to the input
 * by id rather than a `<legend>` or an aria attribute, so the accessible name comes from the
 * browser's own machinery; the floating behaviour is pure CSS driven by `:focus-within` and a
 * class set from the value, which keeps it correct when the field is filled programmatically.
 */
import { computed, ref, useId } from 'vue';

import MdIcon from './MdIcon.vue';
import type { IconName } from './icons';

const props = withDefaults(
  defineProps<{
    /** The field's value. */
    modelValue: string;
    /** The floating label. */
    label: string;
    /** Placeholder text, shown once the label has floated. */
    placeholder?: string;
    /** A hint shown below the field. */
    supportingText?: string;
    /** An error shown below the field, replacing the hint. */
    errorText?: string;
    /** A leading icon. */
    icon?: IconName;
    /** Whether the field is unavailable. */
    disabled?: boolean;
    /** Whether to focus the field when it appears. */
    autofocus?: boolean;
    /** A hard character limit, also used for the counter. */
    maxlength?: number;
    /** Whether to show `used / maxlength` under the field. */
    showCounter?: boolean;
  }>(),
  { disabled: false, autofocus: false, showCounter: false },
);

const emit = defineEmits<{
  'update:modelValue': [value: string];
  /** Enter was pressed without a modifier. */
  submit: [];
}>();

const fieldId = useId();
const input = ref<HTMLInputElement | null>(null);

const floated = computed(() => props.modelValue.length > 0);
const error = computed(() => props.errorText ?? null);
const counter = computed(() =>
  props.showCounter && props.maxlength !== undefined
    ? `${props.modelValue.length} / ${props.maxlength}`
    : null,
);

/** Focuses the field, for the places that open a dialog with a field in it. */
function focus(): void {
  input.value?.focus();
  input.value?.select();
}

function onInput(event: Event): void {
  const target = event.target;
  if (target instanceof HTMLInputElement) emit('update:modelValue', target.value);
}

defineExpose({ focus });
</script>

<template>
  <div class="md-field" :class="{ 'md-field--error': error !== null }">
    <div class="md-field__box" :class="{ 'md-field__box--filled': floated }">
      <MdIcon v-if="icon" :name="icon" :size="20" class="md-field__icon" />
      <div class="md-field__stack">
        <label
          class="md-field__label md-typescale-body-large"
          :class="{ 'md-field__label--floating': floated }"
          :for="fieldId"
        >
          {{ label }}
        </label>
        <input
          :id="fieldId"
          ref="input"
          class="md-field__input md-typescale-body-large"
          type="text"
          :value="modelValue"
          :placeholder="floated ? placeholder : undefined"
          :disabled="disabled"
          :maxlength="maxlength"
          :aria-invalid="error !== null"
          :aria-describedby="error !== null || supportingText ? `${fieldId}-help` : undefined"
          :autofocus="autofocus"
          @input="onInput"
          @keydown.enter.prevent="emit('submit')"
        />
      </div>
      <MdIcon v-if="error !== null" name="error" :size="20" class="md-field__trailing" />
    </div>
    <div class="md-field__footer">
      <span
        v-if="error !== null"
        :id="`${fieldId}-help`"
        class="md-typescale-body-small md-field__help"
      >
        {{ error }}
      </span>
      <span
        v-else-if="supportingText"
        :id="`${fieldId}-help`"
        class="md-typescale-body-small md-field__help"
      >
        {{ supportingText }}
      </span>
      <span v-if="counter" class="md-typescale-body-small md-field__counter">{{ counter }}</span>
    </div>
  </div>
</template>

<style scoped>
.md-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.md-field__box {
  display: flex;
  gap: 12px;
  align-items: center;
  height: 56px;
  padding: 0 16px;
  border: 1px solid var(--md-sys-color-outline);
  border-radius: var(--md-sys-shape-corner-extra-small);
  background: var(--md-sys-color-surface-container-highest);
  transition: border-color var(--md-sys-motion-duration-short3) var(--md-sys-motion-easing-standard);
}

.md-field__box:focus-within {
  border-color: var(--md-sys-color-primary);
  box-shadow: inset 0 0 0 1px var(--md-sys-color-primary);
}

.md-field--error .md-field__box {
  border-color: var(--md-sys-color-error);
}

.md-field__stack {
  position: relative;
  display: flex;
  flex: 1;
  flex-direction: column;
  justify-content: center;
  min-width: 0;
  height: 100%;
}

.md-field__label {
  position: absolute;
  color: var(--md-sys-color-on-surface-variant);
  pointer-events: none;
  transform-origin: left center;
  transition: transform var(--md-sys-motion-duration-short3) var(--md-sys-motion-easing-standard);
}

.md-field__label--floating {
  transform: translateY(-10px) scale(0.75);
}

.md-field__input {
  width: 100%;
  padding: 0;
  border: none;
  background: none;
  outline: none;
  color: var(--md-sys-color-on-surface);
}

.md-field__input::placeholder {
  color: var(--md-sys-color-on-surface-variant);
  opacity: 0.7;
}

.md-field__icon {
  color: var(--md-sys-color-on-surface-variant);
}

.md-field__trailing {
  color: var(--md-sys-color-error);
}

.md-field__footer {
  display: flex;
  gap: 8px;
  justify-content: space-between;
  padding: 0 16px;
  min-height: 16px;
}

.md-field__help {
  color: var(--md-sys-color-on-surface-variant);
}

.md-field--error .md-field__help {
  color: var(--md-sys-color-error);
}

.md-field__counter {
  margin-inline-start: auto;
  color: var(--md-sys-color-on-surface-variant);
}
</style>
