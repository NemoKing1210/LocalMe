// @vitest-environment happy-dom

import { createPinia, setActivePinia, type Pinia } from 'pinia';
import { defineComponent, nextTick } from 'vue';
import { createMemoryHistory, createRouter, type Router } from 'vue-router';
import { describe, expect, it } from 'vitest';
import type { VueWrapper } from '@vue/test-utils';

import { mountView } from '@/test/mount';
import { setMediaMatches } from '@/test/matchMedia';
import { makePeer } from '@/test/factories';
import { usePeerStore } from '@/stores/peers';

import { ROUTE } from './routes';
import ShellView from './ShellView.vue';

// Two distinctive stand-ins for the real pages, so the assertions are about which route the
// shell rendered rather than about the contents of the chat or settings features.
const ChatMarker = defineComponent({
  name: 'ChatMarker',
  template: '<div class="marker marker--chat">chat page</div>',
});
const SettingsMarker = defineComponent({
  name: 'SettingsMarker',
  template: '<div class="marker marker--settings">settings page</div>',
});

const TWO_PANE = '(min-width: 720px)';

function testRouter(): Router {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', redirect: { name: ROUTE.chat } },
      { path: '/chat/:deviceId?', name: ROUTE.chat, component: ChatMarker },
      { path: '/settings', name: ROUTE.settings, component: SettingsMarker },
    ],
  });
}

function piniaWithPeer(): Pinia {
  const pinia = createPinia();
  setActivePinia(pinia);
  usePeerStore().replace([makePeer({ deviceId: 'device-a', nickname: 'Alice' })]);
  return pinia;
}

async function mountShell(route = '/'): Promise<VueWrapper> {
  return mountView(ShellView, {
    route,
    router: testRouter(),
    pinia: piniaWithPeer(),
    // The list is not the subject and would drag its virtualiser and per-row components into
    // every assertion.
    stubs: { UserListView: true },
  });
}

describe('ShellView', () => {
  it('lays out two panes when the window is wide enough', async () => {
    setMediaMatches(TWO_PANE, true);

    const wrapper = await mountShell('/chat/device-a');

    expect(wrapper.get('.shell').attributes('data-wide')).toBe('true');
    expect(wrapper.get('.shell').attributes('data-detail')).toBe('true');
  });

  it('lays out one pane when the window is narrow', async () => {
    setMediaMatches(TWO_PANE, false);

    const wrapper = await mountShell('/chat/device-a');

    expect(wrapper.get('.shell').attributes('data-wide')).toBe('false');
  });

  it('reacts to the window crossing the breakpoint after mount', async () => {
    setMediaMatches(TWO_PANE, false);
    const wrapper = await mountShell('/chat/device-a');
    expect(wrapper.get('.shell').attributes('data-wide')).toBe('false');

    setMediaMatches(TWO_PANE, true);
    await nextTick();

    expect(wrapper.get('.shell').attributes('data-wide')).toBe('true');
  });

  it('renders the chat route and selects its peer', async () => {
    const pinia = piniaWithPeer();

    const wrapper = await mountView(ShellView, {
      route: '/chat/device-a',
      router: testRouter(),
      pinia,
      stubs: { UserListView: true },
    });

    expect(wrapper.find('.marker--chat').exists()).toBe(true);
    expect(usePeerStore().selectedId).toBe('device-a');
  });

  it('marks the detail pane empty when the chat route names no peer', async () => {
    setMediaMatches(TWO_PANE, true);

    const wrapper = await mountShell('/chat');

    expect(wrapper.find('.marker--chat').exists()).toBe(true);
    expect(wrapper.get('.shell').attributes('data-detail')).toBe('false');
    expect(usePeerStore().selectedId).toBeNull();
  });

  it('renders the settings route with the detail pane', async () => {
    setMediaMatches(TWO_PANE, true);

    const wrapper = await mountShell('/settings');

    expect(wrapper.find('.marker--settings').exists()).toBe(true);
    expect(wrapper.get('.shell').attributes('data-detail')).toBe('true');
  });

  it('drops the selection when navigation leaves the conversation', async () => {
    const router = testRouter();
    const wrapper = await mountView(ShellView, {
      route: '/chat/device-a',
      router,
      pinia: piniaWithPeer(),
      stubs: { UserListView: true },
    });
    expect(usePeerStore().selectedId).toBe('device-a');

    await router.push({ name: ROUTE.settings });
    await nextTick();

    expect(usePeerStore().selectedId).toBeNull();
    expect(wrapper.find('.marker--settings').exists()).toBe(true);
  });
});
