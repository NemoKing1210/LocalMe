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
import MdIconButton from './MdIconButton.vue';
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
    /**
     * Accessible name of the clear button. Setting it is what offers to empty the field: a field
     * cannot be clearable without a name for the control that clears it.
     */
    clearLabel?: string;
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

/**
 * Whether the trailing slot holds the clear button. It appears as soon as there is something to
 * clear and stays while the field is focused or not, which is how a search box is expected to
 * behave. A disabled field never clears itself, and an error keeps the slot for its own icon.
 */
const showClear = computed(
  () => !props.disabled && error.value === null && props.modelValue.length > 0,
);

/**
 * Focuses the field, for the places that open a dialog with a field in it.
 *
 * Also the place the clear button returns the caret to: emptying the field from a button must not
 * cost the user the keyboard, or clearing a search would take two clicks to resume typing.
 */
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
  /* One source of truth for the vertical rhythm inside the box. `line` is the body-large line box
     the text is set on, and it also determines the floating label's scaled height; `inset` is the
     room left under the text, and doubles as the input's top padding — see `.md-field__input`. */
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
  /* 2px, and drawn from the border box inward so 1px sits on the border itself: the ring reads as
     a single 2px indicator instead of the hairline a 1px inset vanishes into. */
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
  /* The resting label is centred in the field, the way a filled field reads before it is touched.
     The text is not on that centre line — see `.md-field__input` — and it does not need to be:
     the label only ever travels from here to its floating position, never through the text. */
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
  /* `top` positions the label's centre, so this is the gap to the field's top edge plus half the
     label's scaled line box (24 x 0.75). */
  top: calc(var(--md-field-float-gap) + var(--md-field-line) * 0.375);
  transform: translateY(-50%) scale(0.75);
}

.md-field__input {
  /* The input fills the whole 56px box, so the entire field — not just the 24px text line — is a
     click and focus target; the top padding sets the text on the lower line, under the label. */
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
  /* The input is absolutely positioned over the whole box, so it paints above every in-flow child
     and would swallow this click. Positioning the button puts it in the same painting order as the
     input, and later in the document, so it receives the click. */
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
