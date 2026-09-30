<script setup lang="ts">
/**
 * The first-run screen: pick a nickname, see the avatar it produces, start messaging.
 *
 * Three decisions are worth spelling out.
 *
 * The validation is a *transcription* of `Nickname::parse` in `localme-core`, not a second
 * rule: trim, then count code points (`Array.from`, not `.length`, so 32 emoji are 32
 * characters and not 64), then reject control characters. The submit button is gated on the
 * same function that produces the error message, so "enabled" and "no error" cannot disagree.
 *
 * The avatar is seeded with `"{deviceId}:{nickname}"` — exactly what the host announces to
 * every peer — so the preview is the avatar the rest of the network will draw, not a local
 * approximation of it.
 *
 * The error under the field stays hidden until the user has typed something: a form that
 * greets the first-run user with "Please enter a name" before they have touched it treats a
 * normal state as a mistake.
 */
import { hostname } from '@tauri-apps/plugin-os';
import { computed, onMounted, ref } from 'vue';

import { useI18n, type MessageKey } from '@/i18n';
import * as ipc from '@/ipc';
import { CommandError } from '@/ipc';
import { useSettingsStore } from '@/stores/settings';
import { useUiStore } from '@/stores/ui';
import MdAvatar from '@/ui/MdAvatar.vue';
import MdButton from '@/ui/MdButton.vue';
import MdTextField from '@/ui/MdTextField.vue';

/** Mirrors `MAX_NICKNAME_CHARS` in the core. */
const NICKNAME_MAX = 32;

const settings = useSettingsStore();
const ui = useUiStore();
const i18n = useI18n();

const nickname = ref('');
const deviceId = ref('');
const touched = ref(false);
const busy = ref(false);
/** A rejection from the host, shown through the field rather than thrown. */
const serverError = ref<string | null>(null);

/**
 * The host's rule, restated.
 *
 * Returns the key of the message to show, or `null` when the value would be accepted.
 *
 * The length is counted in code points (`Array.from` iterates a string by code point, exactly
 * as Rust's `chars()` does), not in UTF-16 units, so 32 emoji are 32 characters and not 64.
 */
function check(value: string): MessageKey | null {
  const trimmed = value.trim();
  const length = Array.from(trimmed).length;
  if (length === 0) return 'onboarding.nickname.errorEmpty';
  if (length > NICKNAME_MAX) return 'onboarding.nickname.errorTooLong';
  if (/[\p{Cc}]/u.test(trimmed)) return 'onboarding.nickname.errorControl';
  return null;
}

const errorKey = computed<MessageKey | null>(() => check(nickname.value));
const valid = computed<boolean>(() => errorKey.value === null);

const errorText = computed<string | undefined>(() => {
  if (serverError.value !== null) return serverError.value;
  const key = errorKey.value;
  if (!touched.value || key === null) return undefined;
  return i18n.t(key, { max: NICKNAME_MAX });
});

/**
 * The field's error binding.
 *
 * Bound as an object because `exactOptionalPropertyTypes` forbids handing an optional prop an
 * explicit `undefined`; omitting the key is the only way to say "there is no error".
 */
const errorBind = computed<{ errorText?: string }>(() => {
  const text = errorText.value;
  return text === undefined ? {} : { errorText: text };
});

const avatarSeed = computed<string>(() => `${deviceId.value}:${nickname.value}`);

function onInput(value: string): void {
  nickname.value = value;
  touched.value = true;
  serverError.value = null;
}

async function onSubmit(): Promise<void> {
  touched.value = true;
  serverError.value = null;
  if (!valid.value || busy.value) return;

  busy.value = true;
  try {
    await ipc.completeOnboarding(nickname.value.trim());
    // The host now reports `onboarded`, and the shell switches to the chat view.
    await settings.load();
  } catch (error) {
    if (error instanceof CommandError) serverError.value = error.message;
    else ui.fail('error.internal');
  } finally {
    busy.value = false;
  }
}

onMounted(async () => {
  try {
    nickname.value = (await hostname()) ?? '';
  } catch {
    nickname.value = '';
  }
  try {
    deviceId.value = (await ipc.ownProfile()).deviceId;
  } catch {
    deviceId.value = '';
  }
});
</script>

<template>
  <div class="onboarding">
    <form class="onboarding__card" @submit.prevent="onSubmit">
      <h1 class="md-typescale-headline-small onboarding__title">
        {{ i18n.t('onboarding.title') }}
      </h1>
      <p class="md-typescale-body-large onboarding__subtitle">
        {{ i18n.t('onboarding.subtitle') }}
      </p>

      <MdTextField
        :model-value="nickname"
        :label="i18n.t('onboarding.nickname.label')"
        :placeholder="i18n.t('onboarding.nickname.placeholder')"
        :supporting-text="i18n.t('onboarding.nickname.help')"
        :autofocus="true"
        v-bind="errorBind"
        @update:model-value="onInput"
        @submit="onSubmit"
      />

      <div class="onboarding__preview">
        <MdAvatar :seed="avatarSeed" :name="nickname" :size="96" />
        <span class="md-typescale-label-large onboarding__preview-label">
          {{ i18n.t('onboarding.avatarPreview') }}
        </span>
      </div>

      <MdButton variant="filled" type="submit" :disabled="!valid" :busy="busy">
        {{ i18n.t('onboarding.submit') }}
      </MdButton>
    </form>
  </div>
</template>

<style scoped>
.onboarding {
  display: grid;
  grid-column: 1 / -1;
  place-items: center;
  height: 100%;
  padding: 24px;
  background: var(--md-sys-color-surface);
}

.onboarding__card {
  display: flex;
  flex-direction: column;
  gap: 20px;
  width: min(440px, 100%);
  padding: 32px;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: var(--md-sys-shape-corner-extra-large);
  background: var(--md-sys-color-surface-container-low);
  box-shadow: var(--md-sys-elevation-level1);
}

.onboarding__title {
  margin: 0;
}

.onboarding__subtitle {
  margin: 0;
  color: var(--md-sys-color-on-surface-variant);
}

.onboarding__preview {
  display: flex;
  gap: 16px;
  align-items: center;
}

.onboarding__preview-label {
  color: var(--md-sys-color-on-surface-variant);
}
</style>
