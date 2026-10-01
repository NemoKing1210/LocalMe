<script setup lang="ts">
/**
 * One row of the people list.
 *
 * The row is a list item with a button body, so the whole thing opens a conversation and the
 * two controls that are not "open" — the unread badge and the overflow menu — sit in the
 * trailing slot, outside that button, instead of nesting one interactive element inside
 * another.
 *
 * The supporting line is derived here rather than in the list, because it is the only place
 * that needs a ticking clock: it is re-rendered by `useNow` and by nothing else, so the list
 * itself stays a pure render of the store.
 */
import { computed } from 'vue';

import { useI18n } from '@/i18n';
import type { Peer } from '@/ipc';
import { useNow } from '@/composables/useNow';
import type { IconName } from '@/ui/icons';
import MdAvatar from '@/ui/MdAvatar.vue';
import MdBadge from '@/ui/MdBadge.vue';
import MdListItem from '@/ui/MdListItem.vue';
import MdMenu from '@/ui/MdMenu.vue';

const props = defineProps<{
  /** The peer this row stands for. */
  peer: Peer;
  /** Whether this peer's conversation is the open one. */
  selected: boolean;
}>();

const emit = defineEmits<{
  /** The row was activated: open the conversation. */
  activate: [];
  /** The mute state should be toggled. */
  mute: [];
  /** The forget dialog should be opened. */
  forget: [];
}>();

const i18n = useI18n();
const now = useNow();

/** "online", or "last seen …" / "never online". */
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

/** Offline text is deliberately quieter than the name above it. */
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
</script>

<template>
  <MdListItem
    :headline="peer.nickname"
    :supporting="supporting"
    :supporting-tone="supportingTone"
    :selected="selected"
    @activate="emit('activate')"
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
        :items="menuItems"
        :label="i18n.t('users.actionsFor', { name: peer.nickname })"
        @select="onMenuSelect"
      />
    </template>
  </MdListItem>
</template>
