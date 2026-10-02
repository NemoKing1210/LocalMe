<script setup lang="ts">
// The trailing cluster sits beside the row action, not inside it: interactive content nested in a
// button is unreachable by keyboard and activating it would also activate the row.
//
// `--localme-user-row-height` must match the height the virtualised list measures against.
import { computed } from 'vue';

const props = withDefaults(
  // `exactOptionalPropertyTypes` only accepts the explicit `undefined` default when written out.
  defineProps<{
    headline: string;
    supporting?: string | undefined;
    supportingTone?: 'default' | 'muted' | 'error';
    selected?: boolean;
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

/* The wrapper's state layer must be told when its button has focus (see base.css). */
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

.md-list-item--default .md-list-item__supporting {
  color: var(--md-sys-color-on-surface-variant);
}

.md-list-item--muted .md-list-item__supporting {
  color: var(--md-sys-color-outline);
}

.md-list-item--error .md-list-item__supporting {
  color: var(--md-sys-color-error);
}

.md-list-item--selected .md-list-item__supporting {
  color: var(--md-sys-color-on-secondary-container);
  opacity: 0.8;
}
</style>
