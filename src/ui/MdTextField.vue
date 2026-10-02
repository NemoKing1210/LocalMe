<script setup lang="ts">
// Focus is tracked in the component rather than left to `:focus-within` because the same condition
// also gates the placeholder, and reading the value keeps the label up for programmatic fills.
import { computed, ref, useId } from 'vue';

import MdIcon from './MdIcon.vue';
import MdIconButton from './MdIconButton.vue';
import type { IconName } from './icons';

const props = withDefaults(
  defineProps<{
    modelValue: string;
    label: string;
    placeholder?: string;
    supportingText?: string;
    errorText?: string;
    icon?: IconName;
    disabled?: boolean;
    autofocus?: boolean;
    maxlength?: number;
    showCounter?: boolean;
    /** Setting this (the clear button's accessible name) offers to empty the field. */
    clearLabel?: string;
  }>(),
  { disabled: false, autofocus: false, showCounter: false },
);

const emit = defineEmits<{
  'update:modelValue': [value: string];
  submit: [];
}>();

const fieldId = useId();
const input = ref<HTMLInputElement | null>(null);
const focused = ref(false);

const floated = computed(() => focused.value || props.modelValue.length > 0);
const error = computed(() => props.errorText ?? null);
const counter = computed(() =>
  props.showCounter && props.maxlength !== undefined
    ? `${props.modelValue.length} / ${props.maxlength}`
    : null,
);

const showClear = computed(
  () => !props.disabled && error.value === null && props.modelValue.length > 0,
);

function focus(): void {
  input.value?.focus();
  input.value?.select();
}

function clear(): void {
  emit('update:modelValue', '');
  focus();
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
          @focus="focused = true"
          @blur="focused = false"
          @input="onInput"
          @keydown.enter.prevent="emit('submit')"
        />
      </div>
      <MdIcon v-if="error !== null" name="error" :size="20" class="md-field__trailing" />
      <MdIconButton
        v-else-if="clearLabel !== undefined && showClear"
        class="md-field__clear"
        icon="close"
        :label="clearLabel"
        size="small"
        @click="clear"
      />
    </div>
    <div v-if="error !== null || supportingText || counter" class="md-field__footer">
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
  --md-field-line: var(--md-sys-typescale-body-large-line-height);
  --md-field-inset: 10px;
  --md-field-float-gap: 6px;

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
  /* Inset so the ring reads as one 2px indicator rather than a hairline inside the border. */
  box-shadow: inset 0 0 0 2px var(--md-sys-color-primary);
}

.md-field--error .md-field__box {
  border-color: var(--md-sys-color-error);
}

.md-field--error .md-field__box:focus-within {
  box-shadow: inset 0 0 0 2px var(--md-sys-color-error);
}

.md-field__stack {
  position: relative;
  flex: 1;
  min-width: 0;
  height: 100%;
}

.md-field__label {
  position: absolute;
  inset-inline-start: 0;
  /* The resting label is centred in the field, not on the text's line (see `.md-field__input`). */
  top: 50%;
  transform: translateY(-50%);
  transform-origin: left center;
  color: var(--md-sys-color-on-surface-variant);
  pointer-events: none;
  transition:
    top var(--md-sys-motion-duration-short3) var(--md-sys-motion-easing-standard),
    transform var(--md-sys-motion-duration-short3) var(--md-sys-motion-easing-standard);
}

.md-field__label--floating {
  /* Half the label's scaled line box (0.75 × line); `top` positions the label's centre. */
  top: calc(var(--md-field-float-gap) + var(--md-field-line) * 0.375);
  transform: translateY(-50%) scale(0.75);
}

.md-field__input {
  /* Fills the whole box so the entire field is a click target; top padding sets the text on the
     lower line, under the label. */
  position: absolute;
  inset: 0;
  width: 100%;
  padding: 0;
  padding-block-start: var(--md-field-inset);
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

.md-field__clear {
  /* The absolutely positioned input would otherwise paint above this button and swallow the click. */
  position: relative;
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
