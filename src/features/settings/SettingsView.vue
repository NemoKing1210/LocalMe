<script setup lang="ts">
/**
 * The settings screen is a container: each concern below is its own component, so a change to
 * the logging controls cannot touch the notification ones, and the screen itself only draws the
 * app bar and the scrolling column.
 */
import { useRouter } from 'vue-router';

import { ROUTE } from '@/app/routes';
import { useI18n } from '@/i18n';
import MdIconButton from '@/ui/MdIconButton.vue';
import MdTopAppBar from '@/ui/MdTopAppBar.vue';

import AboutSection from './sections/AboutSection.vue';
import AppearanceSection from './sections/AppearanceSection.vue';
import DataSection from './sections/DataSection.vue';
import LanguageSection from './sections/LanguageSection.vue';
import LoggingSection from './sections/LoggingSection.vue';
import NotificationsSection from './sections/NotificationsSection.vue';
import ProfileSection from './sections/ProfileSection.vue';
import SystemSection from './sections/SystemSection.vue';
// The section components share these rules; they are one stylesheet because a scoped block per
// section would mean copying the card, row and group styling eight times.
import './settings.css';

const i18n = useI18n();
const router = useRouter();

function close(): void {
  // `back()` only when the user arrived from inside the app; a directly-opened address replaces.
  const state = router.options.history.state as Record<string, unknown> | null;
  const previous = state?.['back'];
  if (typeof previous === 'string' && previous.length > 0) router.back();
  else void router.replace({ name: ROUTE.chat });
}
</script>

<template>
  <section class="settings">
    <MdTopAppBar :title="i18n.t('settings.title')">
      <template #leading>
        <MdIconButton icon="back" :label="i18n.t('settings.close')" @click="close" />
      </template>
    </MdTopAppBar>

    <div class="settings__scroll">
      <ProfileSection />
      <AppearanceSection />
      <LanguageSection />
      <NotificationsSection />
      <SystemSection />
      <DataSection />
      <LoggingSection />
      <AboutSection />
    </div>
  </section>
</template>
