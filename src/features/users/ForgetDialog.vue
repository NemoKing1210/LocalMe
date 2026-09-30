<script setup lang="ts">
/**
 * Confirms forgetting a peer, optionally deleting the conversation at the same time.
 *
 * The dialog element stays mounted and is driven by the `open` prop; only its content is
 * conditional on there being a peer. That is deliberate: `<dialog>`'s `showModal` is what
 * makes it the top layer, and calling it from a watcher only works if there is a transition
 * to observe — a dialog that mounts already open would never be promoted.
 *
 * The switch is local state, reset every time a new peer is offered, so "also delete the
 * history" can never carry over from the previous person the user opened it for.
 */
import { computed, ref, watch } from 'vue';

import { useI18n } from '@/i18n';
import type { Peer } from '@/ipc';
import MdButton from '@/ui/MdButton.vue';
import MdDialog from '@/ui/MdDialog.vue';
import MdSwitch from '@/ui/MdSwitch.vue';

const props = defineProps<{
  /** The peer to forget, or `null` while the dialog is closed. */
  peer: Peer | null;
}>();

const emit = defineEmits<{
  /** The dialog was dismissed without confirming. */
  close: [];
  /** Forget was confirmed; the flag says whether to delete the history too. */
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
