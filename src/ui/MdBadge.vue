<script setup lang="ts">
import { computed } from 'vue';
import { AnimatePresence, motion } from 'motion-v';

const props = withDefaults(
  // `exactOptionalPropertyTypes` only accepts the explicit `undefined` default when written out.
  defineProps<{
    /** The count. */
    value?: number | undefined;
    /** The largest count shown in full. */
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
  <!-- Keyed by the number so a value change animates; `popLayout` keeps the outgoing badge out of
       the layout so the row does not jump during the cross-fade. -->
  <AnimatePresence mode="popLayout">
    <motion.span
      v-if="visible"
      :key="text"
      class="md-badge md-typescale-label-small"
      :initial="{ opacity: 0, scale: 0.6 }"
      :animate="{ opacity: 1, scale: 1 }"
      :exit="{ opacity: 0, scale: 0.6 }"
      :transition="{ type: 'spring', stiffness: 520, damping: 28 }"
    >
      {{ text }}
    </motion.span>
  </AnimatePresence>
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
