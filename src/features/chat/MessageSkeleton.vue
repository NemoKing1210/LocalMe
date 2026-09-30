<script setup lang="ts">
/**
 * What the message log shows while a conversation's first page is being read.
 *
 * Bubbles rather than a spinner: the log is about to be filled with bubbles, and a placeholder
 * that already has their shape, their side and their width says what is coming — and, more
 * practically, the pane does not jump when the real rows replace it. The widths are fixed and
 * deliberately uneven, because equal blocks read as a table rather than as a conversation.
 *
 * The whole group is one `role="status"` carrying the "loading" sentence, so a screen reader
 * announces it once and then never speaks for the decorative blocks inside it.
 */
import { useI18n } from '@/i18n';
import MdSkeleton from '@/ui/MdSkeleton.vue';

/** Alternating sides and lengths, mirroring the two sides a real log is made of. */
const ROWS = [
  { outgoing: false, width: '58%', height: '40px' },
  { outgoing: true, width: '42%', height: '40px' },
  { outgoing: false, width: '34%', height: '34px' },
  { outgoing: true, width: '64%', height: '56px' },
  { outgoing: false, width: '48%', height: '40px' },
] as const;

const i18n = useI18n();
</script>

<template>
  <div class="skeleton" role="status" :aria-label="i18n.t('common.loading')">
    <div
      v-for="(row, index) in ROWS"
      :key="index"
      class="skeleton__row"
      :class="{ 'skeleton__row--outgoing': row.outgoing }"
    >
      <MdSkeleton shape="bubble" :width="row.width" :height="row.height" />
    </div>
  </div>
</template>

<style scoped>
.skeleton {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 16px;
}

/* The same sides the bubbles use, so the placeholder and the content it is standing in for
   agree about where a conversation lives. */
.skeleton__row {
  display: flex;
  justify-content: flex-start;
}

.skeleton__row--outgoing {
  justify-content: flex-end;
}
</style>
