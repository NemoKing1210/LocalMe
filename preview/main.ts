import '@/theme/tokens.css';
import '@/theme/base.css';

import { createPinia } from 'pinia';
import { createApp, defineComponent, h } from 'vue';

import OnboardingView from '@/features/onboarding/OnboardingView.vue';
import UserListView from '@/features/users/UserListView.vue';
import { i18n } from '@/i18n';
import type { Peer } from '@/ipc';
import { usePeerStore } from '@/stores/peers';

const now = Date.now();

const sample: readonly Peer[] = [
  {
    deviceId: '11111111-1111-4111-8111-111111111111',
    nickname: 'Аня',
    avatarSeed: '11111111-1111-4111-8111-111111111111:Аня',
    online: true,
    lastSeenMs: now,
    unread: 3,
    notifyMuted: false,
    lastActivityMs: now,
  },
  {
    deviceId: '22222222-2222-4222-8222-222222222222',
    nickname: 'Bartholomew the Extremely Long Nickname',
    avatarSeed: '22222222-2222-4222-8222-222222222222:Bartholomew',
    online: false,
    lastSeenMs: now - 5 * 60_000,
    unread: 0,
    notifyMuted: true,
    lastActivityMs: now - 5 * 60_000,
  },
  {
    deviceId: '33333333-3333-4333-8333-333333333333',
    nickname: 'Case',
    avatarSeed: '33333333-3333-4333-8333-333333333333:Case',
    online: false,
    lastSeenMs: null,
    unread: 120,
    notifyMuted: false,
    lastActivityMs: null,
  },
];

const Host = defineComponent({
  name: 'PreviewHost',
  setup() {
    const peers = usePeerStore();
    peers.replace(sample);
    return () =>
      h(
        'div',
        { style: 'height:100vh;display:grid;grid-template-columns:320px 1fr' },
        [h(UserListView), h(OnboardingView)],
      );
  },
});

const app = createApp(Host);
app.use(createPinia());
app.use(i18n);
app.mount('#app');
