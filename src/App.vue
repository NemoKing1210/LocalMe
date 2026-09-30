<script setup lang="ts">
import { computed } from 'vue';

import { useI18n } from '@/i18n';
import { useTheme } from '@/theme/useTheme';

const { t } = useI18n();
const { effective } = useTheme();

const themeAttribute = computed(() => effective.value);
</script>

<template>
  <div class="shell" :data-theme="themeAttribute">
    <main class="shell__body">
      <h1 class="md-typescale-headline-small">{{ t('app.name') }}</h1>
      <p class="md-typescale-body-medium shell__tagline">{{ t('app.tagline') }}</p>
    </main>
  </div>
</template>

<style scoped>
.shell {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--md-sys-color-surface);
  color: var(--md-sys-color-on-surface);
}

.shell__body {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 8px;
  align-items: center;
  justify-content: center;
  padding: 24px;
  text-align: center;
}

.shell__tagline {
  color: var(--md-sys-color-on-surface-variant);
}
</style>
