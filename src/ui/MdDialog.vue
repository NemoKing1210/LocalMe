<script setup lang="ts">
import { onBeforeUnmount, ref, useId, watch } from 'vue';

const props = withDefaults(
  defineProps<{
    open: boolean;
    headline: string;
    wide?: boolean;
  }>(),
  { wide: false },
);

const emit = defineEmits<{ close: [] }>();

const dialog = ref<HTMLDialogElement | null>(null);
const headlineId = useId();

watch(
  () => props.open,
  (open) => {
    const element = dialog.value;
    if (!element) return;
    if (open && !element.open) {
      element.showModal();
    } else if (!open && element.open) {
      element.close();
    }
  },
);

function onClose(): void {
  emit('close');
}

// A dialog left in the DOM while the window is hidden would trap focus invisibly.
onBeforeUnmount(() => {
  dialog.value?.close();
});
</script>

<template>
  <dialog
    ref="dialog"
    class="md-dialog"
    :class="{ 'md-dialog--wide': wide }"
    :aria-labelledby="headlineId"
    @close="onClose"
    @cancel.prevent="emit('close')"
  >
    <div class="md-dialog__surface">
      <h2 :id="headlineId" class="md-typescale-headline-small md-dialog__headline">
        {{ headline }}
      </h2>
      <div class="md-typescale-body-medium md-dialog__content">
        <slot />
      </div>
      <div class="md-dialog__actions">
        <slot name="actions" />
      </div>
    </div>
  </dialog>
</template>

<style scoped>
.md-dialog {
  max-width: min(560px, calc(100vw - 48px));
  padding: 0;
  border: none;
  background: none;
  color: inherit;
}

.md-dialog--wide {
  width: 560px;
}

.md-dialog::backdrop {
  background: color-mix(in srgb, var(--md-sys-color-scrim) 32%, transparent);
}

/* Durations collapse to zero under `prefers-reduced-motion` (tokens.css), so the dialog appears. */
.md-dialog[open]::backdrop {
  animation: md-dialog-scrim var(--md-sys-motion-duration-medium2)
    var(--md-sys-motion-easing-standard);
}

.md-dialog[open] .md-dialog__surface {
  animation: md-dialog-surface var(--md-sys-motion-duration-medium2)
    var(--md-sys-motion-easing-emphasized-decelerate);
}

@keyframes md-dialog-scrim {
  from {
    opacity: 0;
  }
}

@keyframes md-dialog-surface {
  from {
    opacity: 0;
    transform: translateY(12px) scale(0.98);
  }
}

.md-dialog__surface {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 24px;
  border-radius: var(--md-sys-shape-corner-extra-large);
  background: var(--md-sys-color-surface-container-high);
  color: var(--md-sys-color-on-surface);
  box-shadow: var(--md-sys-elevation-level3);
}

.md-dialog__headline {
  margin: 0;
}

.md-dialog__content {
  display: flex;
  flex-direction: column;
  gap: 12px;
  color: var(--md-sys-color-on-surface-variant);
}

.md-dialog__actions {
  display: flex;
  gap: 8px;
  align-items: center;
  justify-content: flex-end;
  margin-top: 8px;
}
</style>
