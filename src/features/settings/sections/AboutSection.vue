<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';

import { useI18n } from '@/i18n';
import * as ipc from '@/ipc';
import type { Diagnostics } from '@/ipc';
import { useUiStore } from '@/stores/ui';

const ui = useUiStore();
const i18n = useI18n();

const diagnostics = ref<Diagnostics | null>(null);

const versionLine = computed<string | null>(() => {
  const info = diagnostics.value;
  return info === null ? null : i18n.t('settings.aboutVersion', { version: info.version });
});

// Labels are the host's field names, not translated: they must match what the Rust side logs.
const diagnosticsRows = computed<readonly { readonly label: string; readonly value: string }[]>(
  () => {
    const info = diagnostics.value;
    if (info === null) return [];
    return [
      { label: 'protocolVersion', value: `${info.protocolVersion}` },
      { label: 'tcpPort', value: `${info.tcpPort}` },
      { label: 'platform', value: info.platform },
      { label: 'deviceId', value: info.deviceId },
    ];
  },
);

onMounted(async () => {
  const info = await ui.attempt(() => ipc.diagnostics(), 'the diagnostics could not be read');
  if (info.ok) diagnostics.value = info.value;
});
</script>

<template>
  <section class="settings__group">
    <h2 class="md-typescale-title-medium settings__heading">
      {{ i18n.t('settings.aboutTitle') }}
    </h2>
    <div class="settings__card">
      <p v-if="versionLine !== null" class="md-typescale-body-medium">{{ versionLine }}</p>
      <p class="md-typescale-body-medium settings__help">
        {{ i18n.t('settings.aboutArchitecture') }}
      </p>
      <p class="md-typescale-body-medium settings__help">
        {{ i18n.t('settings.aboutLicense') }}
      </p>
    </div>
    <div v-if="diagnosticsRows.length > 0" class="settings__card">
      <h3 class="md-typescale-title-small">{{ i18n.t('settings.aboutDiagnostics') }}</h3>
      <dl class="settings__pairs">
        <template v-for="row in diagnosticsRows" :key="row.label">
          <dt class="md-typescale-label-small settings__help">{{ row.label }}</dt>
          <dd class="md-typescale-body-small settings__mono" data-selectable>
            {{ row.value }}
          </dd>
        </template>
      </dl>
    </div>
  </section>
</template>
