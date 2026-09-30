<script setup lang="ts">
/**
 * The "nothing here" panel.
 *
 * One component for every empty surface in the application — no peers yet, no results for a
 * search, no conversation selected — because the alternative is four slightly different
 * centres, four paddings and four tones of grey. The copy and the actions come from the caller;
 * this only decides the shape.
 *
 * The icon is drawn large and at the same `on-surface-variant` colour as the text, with no
 * container behind it: a filled circle here would compete with the empty state's actual job,
 * which is to explain what the user should do next.
 */
import MdIcon from './MdIcon.vue';
import type { IconName } from './icons';

withDefaults(
  // The `| undefined` is what lets the `undefined` defaults typecheck under
  // `exactOptionalPropertyTypes`; the props' types are the same either way.
  defineProps<{
    /** The headline. */
    title: string;
    /** A sentence explaining why the surface is empty, or what to do about it. */
    body?: string | undefined;
    /** A large illustration glyph. */
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
