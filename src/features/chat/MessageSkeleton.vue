<script setup lang="ts">
/** One `role="status"` for the whole group, so a screen reader announces it once. */
import { useI18n } from '@/i18n';
import MdSkeleton from '@/ui/MdSkeleton.vue';

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

.skeleton__row {
  display: flex;
  justify-content: flex-start;
}

.skeleton__row--outgoing {
  justify-content: flex-end;
}
</style>
