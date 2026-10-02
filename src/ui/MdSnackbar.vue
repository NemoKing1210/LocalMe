<script setup lang="ts">
defineProps<{
  open: boolean;
  text: string;
  tone?: 'neutral' | 'error';
}>();

const emit = defineEmits<{ dismiss: [] }>();
</script>

<template>
  <Transition name="md-snackbar">
    <div
      v-if="open"
      class="md-snackbar"
      :class="{ 'md-snackbar--error': tone === 'error' }"
      role="status"
      aria-live="polite"
    >
      <span class="md-typescale-body-medium md-snackbar__text">{{ text }}</span>
      <button
        class="md-snackbar__action md-typescale-label-large"
        type="button"
        @click="emit('dismiss')"
      >
        <slot name="action">OK</slot>
      </button>
    </div>
  </Transition>
</template>

<style scoped>
.md-snackbar {
  position: fixed;
  inset-block-end: 24px;
  inset-inline-start: 24px;
  z-index: 10;
  display: flex;
  gap: 16px;
  align-items: center;
  max-width: min(480px, calc(100vw - 48px));
  padding: 12px 12px 12px 16px;
  border-radius: var(--md-sys-shape-corner-small);
  background: var(--md-sys-color-inverse-surface);
  color: var(--md-sys-color-inverse-on-surface);
  box-shadow: var(--md-sys-elevation-level3);
}

.md-snackbar--error {
  background: var(--md-sys-color-error-container);
  color: var(--md-sys-color-on-error-container);
}

.md-snackbar__text {
  min-width: 0;
}

.md-snackbar__action {
  flex: none;
  padding: 6px 12px;
  border-radius: var(--md-sys-shape-corner-full);
  color: inherit;
  opacity: 0.9;
}

.md-snackbar__action:hover {
  opacity: 1;
  background: color-mix(in srgb, currentcolor 12%, transparent);
}

.md-snackbar-enter-active,
.md-snackbar-leave-active {
  transition:
    opacity var(--md-sys-motion-duration-medium2) var(--md-sys-motion-easing-emphasized-decelerate),
    transform var(--md-sys-motion-duration-medium2)
      var(--md-sys-motion-easing-emphasized-decelerate);
}

.md-snackbar-enter-from,
.md-snackbar-leave-to {
  opacity: 0;
  transform: translateY(12px);
}
</style>
