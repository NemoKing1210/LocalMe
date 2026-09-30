<script setup lang="ts">
/**
 * A Material 3 list row.
 *
 * The row is a `<div>` holding one `<button>` for the row action and, when the caller supplies
 * one, the trailing cluster *beside* that button rather than inside it. The obvious shape — a
 * single full-width button with everything in it — is invalid as soon as the trailing slot
 * holds a control: interactive content inside a button is unreachable by keyboard and activating
 * it also activates the row. Splitting the two keeps the row's whole text-and-avatar area
 * clickable while an overflow menu or a badge in the trailing position behaves like the separate
 * control it is.
 *
 * The state layer stays on the wrapper, so hovering the trailing cluster tints the row too and a
 * menu does not appear to sit on an untinted island. The wrapper is not interactive: it draws no
 * focus ring and owns no tab stop.
 *
 * The height comes from `--localme-user-row-height` because the virtualised list measures
 * against the same variable. The two must agree, and a literal here would be the first of two
 * places to change when the row height does.
 */
import { computed } from 'vue';

const props = withDefaults(
  // The `| undefined` is what lets the `undefined` default typecheck under
  // `exactOptionalPropertyTypes`; the prop's type is the same either way.
  defineProps<{
    /** The primary line: the peer's nickname. */
    headline: string;
    /** A secondary line, used when the `subtitle` slot is not supplied. */
    supporting?: string | undefined;
    /** How the supporting line is coloured. `muted` is the quieter, greyer look. */
    supportingTone?: 'default' | 'muted' | 'error';
    /** Whether this row is the current selection. */
    selected?: boolean;
    /** The compact row height, for lists that are scanned rather than read. */
    dense?: boolean;
  }>(),
  {
    supporting: undefined,
    supportingTone: 'default',
    selected: false,
    dense: false,
  },
);

const emit = defineEmits<{ activate: [] }>();

const classes = computed(() => [
  `md-list-item--${props.supportingTone}`,
  {
    'md-list-item--selected': props.selected,
    'md-list-item--dense': props.dense,
  },
]);
</script>

<template>
  <div class="md-list-item md-state-layer" :class="classes">
    <button
      class="md-list-item__row"
      type="button"
      :aria-current="selected ? 'true' : undefined"
      @click="emit('activate')"
    >
      <span v-if="$slots.leading" class="md-list-item__leading">
        <slot name="leading" />
      </span>
      <span class="md-list-item__text">
        <span class="md-typescale-body-large md-list-item__headline">{{ headline }}</span>
        <span
          v-if="supporting || $slots.subtitle"
          class="md-typescale-body-medium md-list-item__supporting"
        >
          <slot name="subtitle">{{ supporting }}</slot>
        </span>
      </span>
    </button>
    <div v-if="$slots.trailing" class="md-list-item__trailing">
      <slot name="trailing" />
    </div>
  </div>
</template>

<style scoped>
.md-list-item {
  display: flex;
  gap: 8px;
  align-items: center;
  height: var(--localme-user-row-height);
  padding-inline: 16px;
}

.md-list-item--dense {
  height: 56px;
}

.md-list-item--selected {
  background: var(--md-sys-color-secondary-container);
  color: var(--md-sys-color-on-secondary-container);
}

/*
 * The focus ring of the row action is drawn by `base.css` on the button itself, but the hover
 * and press tints come from the wrapper's state layer, which has to be told when its button has
 * focus.
 */
.md-list-item:has(.md-list-item__row:focus-visible)::after {
  opacity: var(--md-sys-state-focus-opacity);
}

.md-list-item__row {
  display: flex;
  flex: 1;
  gap: 16px;
  align-items: center;
  height: 100%;
  min-width: 0;
  color: inherit;
  text-align: start;
}

.md-list-item__leading,
.md-list-item__trailing {
  display: flex;
  flex: none;
  align-items: center;
}

.md-list-item__text {
  display: flex;
  flex: 1;
  flex-direction: column;
  justify-content: center;
  min-width: 0;
}

.md-list-item__headline,
.md-list-item__supporting {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* The supporting line is quieter than the headline in every tone; the tones differ in how. */
.md-list-item--default .md-list-item__supporting {
  color: var(--md-sys-color-on-surface-variant);
}

.md-list-item--muted .md-list-item__supporting {
  color: var(--md-sys-color-outline);
}

.md-list-item--error .md-list-item__supporting {
  color: var(--md-sys-color-error);
}

/* A selected row already carries a container colour; its own text must stay legible on it. */
.md-list-item--selected .md-list-item__supporting {
  color: var(--md-sys-color-on-secondary-container);
  opacity: 0.8;
}
</style>
