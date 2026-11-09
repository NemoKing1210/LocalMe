<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';

import { usePresence } from '@/composables/usePresence';
import { useI18n } from '@/i18n';
import * as ipc from '@/ipc';
import type { KnownDevice } from '@/ipc';
import { useChatStore } from '@/stores/chat';
import { usePeerStore } from '@/stores/peers';
import { useUiStore } from '@/stores/ui';
import MdAvatar from '@/ui/MdAvatar.vue';
import MdBadge from '@/ui/MdBadge.vue';
import MdButton from '@/ui/MdButton.vue';
import MdDialog from '@/ui/MdDialog.vue';
import MdIconButton from '@/ui/MdIconButton.vue';
import MdListItem from '@/ui/MdListItem.vue';

const chat = useChatStore();
const peers = usePeerStore();
const ui = useUiStore();
const i18n = useI18n();
const presenceSentence = usePresence();

const devices = ref<readonly KnownDevice[]>([]);
const confirmClear = ref(false);

const nothingKnown = computed(() => devices.value.length === 0);

function deviceSupporting(device: KnownDevice): string {
  const status = device.forgotten
    ? i18n.t('settings.deviceForgotten')
    : i18n.t('settings.deviceKnown');
  const seen = presenceSentence({ online: false, lastSeenMs: device.lastSeenMs }, Date.now());
  return `${status} · ${seen}`;
}

async function loadDevices(): Promise<void> {
  const known = await ui.attempt(() => ipc.knownDevices(), 'the known devices could not be read');
  if (known.ok) devices.value = known.value;
}

async function restoreDevice(deviceId: string): Promise<void> {
  const restored = await ui.attempt(
    () => ipc.restorePeer(deviceId),
    'the device could not be restored',
  );
  if (restored.ok) await loadDevices();
}

async function clearHistory(): Promise<void> {
  confirmClear.value = false;
  const cleared = await ui.attempt(() => ipc.clearHistory(), 'the history could not be cleared');
  if (!cleared.ok) return;
  // Reload the open conversation: the rows are gone from the database.
  await chat.open(peers.selectedId);
  ui.notify('settings.historyCleared', { count: cleared.value });
  await loadDevices();
}

onMounted(() => {
  void loadDevices();
});
</script>

<template>
  <section class="settings__group">
    <h2 class="md-typescale-title-medium settings__heading">
      {{ i18n.t('settings.groupData') }}
    </h2>
    <div class="settings__card">
      <h3 class="md-typescale-title-small">{{ i18n.t('settings.knownDevices') }}</h3>
      <p v-if="nothingKnown" class="md-typescale-body-small settings__help">
        {{ i18n.t('settings.devicesEmpty') }}
      </p>
      <ul v-else class="settings__devices">
        <li v-for="device in devices" :key="device.deviceId">
          <MdListItem :headline="device.nickname" :supporting="deviceSupporting(device)">
            <template #leading>
              <MdAvatar
                :seed="device.avatarSeed"
                :name="device.nickname"
                :size="40"
                :dimmed="device.forgotten"
                animate="hover"
              />
            </template>
            <template #trailing>
              <MdBadge :value="device.messageCount" />
              <MdIconButton
                v-if="device.forgotten"
                icon="refresh"
                size="small"
                :label="i18n.t('settings.restoreDevice')"
                @click="restoreDevice(device.deviceId)"
              />
            </template>
          </MdListItem>
        </li>
      </ul>
      <div class="settings__danger">
        <p class="md-typescale-body-small settings__help">
          {{ i18n.t('settings.clearHistoryHelp') }}
        </p>
        <MdButton class="settings__clear" variant="text" icon="trash" @click="confirmClear = true">
          {{ i18n.t('settings.clearHistory') }}
        </MdButton>
      </div>
    </div>

    <MdDialog
      :open="confirmClear"
      :headline="i18n.t('settings.clearHistoryConfirm')"
      @close="confirmClear = false"
    >
      <p class="md-typescale-body-medium">{{ i18n.t('settings.clearHistoryHelp') }}</p>
      <template #actions>
        <MdButton variant="text" @click="confirmClear = false">
          {{ i18n.t('common.cancel') }}
        </MdButton>
        <MdButton variant="filled" @click="clearHistory">
          {{ i18n.t('common.delete') }}
        </MdButton>
      </template>
    </MdDialog>
  </section>
</template>
