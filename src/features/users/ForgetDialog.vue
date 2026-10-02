<script setup lang="ts">
/**
 * Stays mounted, driven by `open`: `<dialog>`'s `showModal` only promotes a dialog that mounts
 * closed and then opens.
 */
import { computed, ref, watch } from 'vue';

import { useI18n } from '@/i18n';
import type { Peer } from '@/ipc';
import MdButton from '@/ui/MdButton.vue';
import MdDialog from '@/ui/MdDialog.vue';
import MdSwitch from '@/ui/MdSwitch.vue';

const props = defineProps<{
  peer: Peer | null;
}>();

const emit = defineEmits<{
  close: [];
  confirmed: [deleteHistory: boolean];
}>();

const i18n = useI18n();
const deleteHistory = ref(false);

watch(
  () => props.peer,
  () => {
    deleteHistory.value = false;
  },
);

const headline = computed<string>(() =>
  props.peer === null ? '' : i18n.t('forget.title', { name: props.peer.nickname }),
);

function confirm(): void {
  emit('confirmed', deleteHistory.value);
}
</script>

<template>
  <MdDialog :open="peer !== null" :headline="headline" @close="emit('close')">
    <p v-if="peer !== null" class="forget__body">{{ i18n.t('forget.body') }}</p>
    <MdSwitch
      v-if="peer !== null"
      v-model="deleteHistory"
      :label="i18n.t('forget.deleteHistory')"
    />

    <template #actions>
      <MdButton variant="text" @click="emit('close')">{{ i18n.t('common.cancel') }}</MdButton>
      <MdButton variant="filled" @click="confirm">{{ i18n.t('forget.confirm') }}</MdButton>
    </template>
  </MdDialog>
</template>

<style scoped>
.forget__body {
  margin: 0;
}
</style>
