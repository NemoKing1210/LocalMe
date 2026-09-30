<script setup lang="ts">
/**
 * A count badge, for unread messages.
 *
 * The badge renders *nothing* rather than an empty pill when the count is zero or absent: an
 * unread indicator that says "0" is worse than no indicator, and `v-if` at the call site is a
 * rule every caller can forget. Above `max` the count is clamped to `max+`, because a badge
 * wide enough for a four-digit number would push the row's layout around to communicate one
 * bit of information.
 */
import { computed } from 'vue';

const props = withDefaults(
  defineProps<{
    /** The count. Nothing is rendered when absent or zero. */
    value?: number;
    /** The largest count shown in full; above it the badge reads `max+`. */
    max?: number;
  }>(),
  { value: undefined, max: 99 },
);

const visible = computed(() => props.value !== undefined && props.value > 0);

const text = computed(() => {
  const value = props.value ?? 0;
  return value > props.max ? `${props.max}+` : `${value}`;
});
</script>

<template>
  <span v-if="visible" class="md-badge md-typescale-label-small">{{ text }}</span>
</template>

<style scoped>
.md-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  border-radius: var(--md-sys-shape-corner-full);
  background: var(--md-sys-color-error);
  color: var(--md-sys-color-on-error);
  font-variant-numeric: tabular-nums;
}
</style>
