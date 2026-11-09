<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';

import { useI18n } from '@/i18n';
import * as ipc from '@/ipc';
import { CommandError } from '@/ipc';
import type { Profile } from '@/ipc';
import { useUiStore } from '@/stores/ui';
import MdAvatar from '@/ui/MdAvatar.vue';
import MdTextField from '@/ui/MdTextField.vue';

const ui = useUiStore();
const i18n = useI18n();

const profile = ref<Profile | null>(null);
const nickname = ref('');
const nicknameError = ref<string | undefined>(undefined);

// `exactOptionalPropertyTypes`: a possibly-undefined value is a type error for a prop declared
// without `| undefined`.
const nicknameFieldProps = computed(() => {
  const error = nicknameError.value;
  return error === undefined ? {} : { errorText: error };
});

async function loadProfile(): Promise<void> {
  const own = await ui.attempt(() => ipc.ownProfile(), 'the own profile could not be read');
  if (!own.ok) return;
  profile.value = own.value;
  nickname.value = own.value.nickname;
}

// The host validates the nickname; its refusal is shown on the field.
async function commitNickname(): Promise<void> {
  const value = nickname.value.trim();
  const current = profile.value;
  if (current === null || value === current.nickname) {
    nickname.value = current?.nickname ?? nickname.value;
    return;
  }
  try {
    const updated = await ipc.setNickname(value);
    profile.value = updated;
    nickname.value = updated.nickname;
    nicknameError.value = undefined;
  } catch (error) {
    nicknameError.value = error instanceof CommandError ? error.message : i18n.t('error.internal');
  }
}

// The error belongs to the value that was refused; typing a new one makes it stale.
watch(nickname, () => {
  nicknameError.value = undefined;
});

onMounted(() => {
  void loadProfile();
});
</script>

<template>
  <section class="settings__group">
    <h2 class="md-typescale-title-medium settings__heading">
      {{ i18n.t('settings.groupProfile') }}
    </h2>
    <div v-if="profile" class="settings__card">
      <div class="settings__profile">
        <MdAvatar :seed="profile.avatarSeed" :name="profile.nickname" :size="72" animate="always" />
        <div class="settings__profile-field" @focusout="commitNickname">
          <MdTextField
            v-bind="nicknameFieldProps"
            v-model="nickname"
            :label="i18n.t('settings.nickname')"
            :supporting-text="i18n.t('settings.nicknameHelp')"
            @submit="commitNickname"
          />
        </div>
      </div>
    </div>
  </section>
</template>
