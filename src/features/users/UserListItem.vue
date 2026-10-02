<script setup lang="ts">
/**
 * Controls that are not "open" sit outside the row's button, so no interactive element nests in
 * another; the supporting line is derived here because this is the only place with a ticking clock.
 */
import { computed, ref } from 'vue';

import { useI18n } from '@/i18n';
import type { Peer } from '@/ipc';
import { useNow } from '@/composables/useNow';
import type { IconName } from '@/ui/icons';
import MdAvatar from '@/ui/MdAvatar.vue';
import MdBadge from '@/ui/MdBadge.vue';
import MdListItem from '@/ui/MdListItem.vue';
import MdMenu from '@/ui/MdMenu.vue';

const props = defineProps<{
  peer: Peer;
  selected: boolean;
}>();

const emit = defineEmits<{
  activate: [];
  mute: [];
  forget: [];
}>();

const i18n = useI18n();
const now = useNow();

const presence = computed<string>(() => {
  if (props.peer.online) return i18n.t('users.online');
  if (props.peer.lastSeenMs === null) return i18n.t('users.neverSeen');
  return i18n.t('users.lastSeen', {
    relative: i18n.relative(props.peer.lastSeenMs, now.value),
  });
});

const supporting = computed<string>(() => {
  const preview = props.peer.lastMessage;
  const base =
    preview === null
      ? presence.value
      : preview.direction === 'outgoing'
        ? i18n.t('users.youPreview', { body: preview.body })
        : preview.body;
  return props.peer.notifyMuted ? `${base} · ${i18n.t('users.muted')}` : base;
});

const supportingTone = computed<'default' | 'muted'>(() =>
  props.peer.online ? 'default' : 'muted',
);

interface RowMenuAction {
  readonly id: string;
  readonly label: string;
  readonly icon: IconName;
  readonly danger?: boolean;
}

const menuItems = computed<readonly RowMenuAction[]>(() => [
  {
    id: 'mute',
    label: i18n.t(props.peer.notifyMuted ? 'users.unmute' : 'users.mute'),
    icon: props.peer.notifyMuted ? 'bell-off' : 'bell',
  },
  { id: 'forget', label: i18n.t('users.forget'), icon: 'trash', danger: true },
]);

function onMenuSelect(id: string): void {
  if (id === 'mute') emit('mute');
  else if (id === 'forget') emit('forget');
}

const menu = ref<InstanceType<typeof MdMenu> | null>(null);

function openMenu(event: MouseEvent): void {
  const target = event.target;
  // A right-press inside the open menu must not bounce back to the row and reset its focus.
  if (target instanceof Element && target.closest('.md-menu__surface') !== null) return;
  menu.value?.show();
}
</script>

<template>
  <MdListItem
    :headline="peer.nickname"
    :supporting="supporting"
    :supporting-tone="supportingTone"
    :selected="selected"
    @activate="emit('activate')"
    @contextmenu.prevent="openMenu"
  >
    <template #leading>
      <MdAvatar
        :seed="peer.avatarSeed"
        :name="peer.nickname"
        :size="40"
        :presence="peer.online ? 'online' : 'offline'"
        :dimmed="!peer.online"
      />
    </template>

    <template #trailing>
      <MdBadge :value="peer.unread" />
      <MdMenu
        ref="menu"
        :items="menuItems"
        :label="i18n.t('users.actionsFor', { name: peer.nickname })"
        @select="onMenuSelect"
      />
    </template>
  </MdListItem>
</template>

<style scoped>
/*
 * `display: none` drops the trigger's tab stop; `:focus-within` on the row restores it before the
 * next Tab, so it stays reachable by keyboard.
 */
.md-list-item:not(:hover):not(:focus-within) :deep(.md-menu__trigger) {
  display: none;
}
</style>
