<script setup lang="ts">
import MdIcon from './MdIcon.vue';
import type { IconName } from './icons';

withDefaults(
  // `exactOptionalPropertyTypes` only accepts the explicit `undefined` defaults when written out.
  defineProps<{
    title: string;
    body?: string | undefined;
    icon?: IconName | undefined;
  }>(),
  { body: undefined, icon: undefined },
);
</script>

<template>
  <div class="md-empty-state">
    <MdIcon v-if="icon" :name="icon" :size="48" class="md-empty-state__icon" />
    <p class="md-typescale-title-medium md-empty-state__title">{{ title }}</p>
    <p v-if="body" class="md-typescale-body-medium md-empty-state__body">{{ body }}</p>
    <div v-if="$slots.default" class="md-empty-state__actions">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.md-empty-state {
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: center;
  justify-content: center;
  height: 100%;
  padding: 32px 24px;
  color: var(--md-sys-color-on-surface-variant);
  text-align: center;
}

.md-empty-state__icon {
  margin-block-end: 8px;
  opacity: 0.6;
}

.md-empty-state__title {
  color: var(--md-sys-color-on-surface);
}

.md-empty-state__body {
  max-width: 40ch;
}

.md-empty-state__actions {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-block-start: 16px;
}
</style>
