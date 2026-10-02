// @vitest-environment happy-dom
import type { VueWrapper } from '@vue/test-utils';
import { createPinia, setActivePinia, type Pinia } from 'pinia';
import { nextTick, type ComputedRef } from 'vue';
import type { Router } from 'vue-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { useVirtualizer as realUseVirtualizer } from '@tanstack/vue-virtual';
import type { Peer } from '@/ipc';
import { makePeer } from '@/test/factories';
import { createTestRouter, flushPromises, mountView } from '@/test/mount';

import ForgetDialog from './ForgetDialog.vue';
import UserListItem from './UserListItem.vue';
import UserListView from './UserListView.vue';

type FakeVirtualizer = {
  getTotalSize: () => number;
  getVirtualItems: () => { index: number; key: number; start: number; size: number }[];
};

/**
 * happy-dom gives a scroll element no layout, so the real virtualiser would measure zero rows.
 * This replacement keeps the component's own mapping — count, index, and start — under test.
 */
vi.mock('@tanstack/vue-virtual', async () => {
  // The factory is hoisted above the imports, so `computed` cannot be a static binding here.
  const { computed } = await import('vue');

  function useVirtualizer(options: {
    readonly value: { readonly count: number; readonly estimateSize: () => number };
  }): ComputedRef<FakeVirtualizer> {
    return computed(() => {
      const count = options.value.count;
      const size = options.value.estimateSize();
      return {
        getTotalSize: (): number => count * size,
        getVirtualItems: () =>
          Array.from({ length: count }, (_, index) => ({
            index,
            key: index,
            start: index * size,
            size,
          })),
      };
    });
  }

  return { useVirtualizer: useVirtualizer as unknown as typeof realUseVirtualizer };
});

vi.mock('@/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/ipc')>()),
  listPeers: vi.fn(),
  setPeerMuted: vi.fn(),
  forgetPeer: vi.fn(),
}));

import * as ipc from '@/ipc';
import { useChatStore } from '@/stores/chat';
import { usePeerStore } from '@/stores/peers';
import { useUiStore } from '@/stores/ui';

interface ListHarness {
  readonly wrapper: VueWrapper;
  readonly router: Router;
  readonly pinia: Pinia;
}

async function mountList(
  peers: readonly Peer[],
  options: { readonly query?: string; readonly route?: string } = {},
): Promise<ListHarness> {
  const pinia = createPinia();
  setActivePinia(pinia);
  const store = usePeerStore(pinia);
  store.replace(peers);
  store.query = options.query ?? '';

  const router = createTestRouter();
  const wrapper = await mountView(
    UserListView,
    options.route === undefined ? { pinia, router } : { pinia, router, route: options.route },
  );
  return { wrapper, router, pinia };
}

beforeEach(() => {
  vi.resetAllMocks();
});

describe('UserListView', () => {
  it('renders one row per visible peer at the estimated offsets', async () => {
    const { wrapper } = await mountList([
      makePeer({ deviceId: 'device-a', nickname: 'Alice' }),
      makePeer({ deviceId: 'device-b', nickname: 'Bob' }),
    ]);

    const rows = wrapper.findAll('.users__row');
    expect(rows).toHaveLength(2);
    expect(rows[0]!.text()).toContain('Alice');
    expect(rows[1]!.text()).toContain('Bob');

    // The list is positioned, not laid out: each row sits at the virtualiser's start offset.
    expect(rows[0]!.attributes('style')).toContain('top: 0px');
    expect(rows[1]!.attributes('style')).toContain('top: 72px');
    expect(wrapper.get('.users__list').attributes('style')).toContain('height: 144px');
  });

  it('shows the empty state when no peer has been discovered', async () => {
    const { wrapper } = await mountList([]);

    expect(wrapper.get('.md-empty-state__title').text()).toBe('Nobody here yet');
    expect(wrapper.find('.users__scroll').exists()).toBe(false);
    expect(wrapper.findAll('.users__row')).toHaveLength(0);
  });

  it('shows the no-match state when a search matches nothing', async () => {
    const { wrapper } = await mountList([makePeer({ nickname: 'Alice' })], { query: 'zzz' });

    expect(wrapper.get('.md-empty-state__title').text()).toContain('zzz');
    expect(wrapper.find('.users__scroll').exists()).toBe(false);
  });

  it('writes what is typed into the store query and filters the rows', async () => {
    const { wrapper, pinia } = await mountList([
      makePeer({ deviceId: 'device-a', nickname: 'Alice' }),
      makePeer({ deviceId: 'device-b', nickname: 'Bob' }),
    ]);

    await wrapper.get('.md-field__input').setValue('bo');

    expect(usePeerStore(pinia).query).toBe('bo');
    expect(wrapper.get('.md-list-item__headline').text()).toBe('Bob');
    expect(wrapper.findAll('.users__row')).toHaveLength(1);
  });

  it('navigates to the chat route with the peer id when a row is selected', async () => {
    const { wrapper, router } = await mountList([makePeer({ deviceId: 'device-a' })]);

    await wrapper.get('.md-list-item__row').trigger('click');
    await flushPromises();

    expect(router.currentRoute.value.name).toBe('chat');
    expect(router.currentRoute.value.params.deviceId).toBe('device-a');
  });

  it('reflects each peer’s unread count and presence', async () => {
    const { wrapper } = await mountList([
      makePeer({ deviceId: 'device-on', online: true, unread: 2 }),
      makePeer({ deviceId: 'device-off', online: false, lastSeenMs: null, unread: 0 }),
    ]);

    const badges = wrapper.findAll('.md-badge');
    expect(badges).toHaveLength(1);
    expect(badges[0]!.text()).toBe('2');
    expect(wrapper.findAll('.md-avatar__presence--online')).toHaveLength(1);
    expect(wrapper.findAll('.md-avatar__presence--offline')).toHaveLength(1);
  });

  it('marks only the muted peer’s supporting line', async () => {
    const { wrapper } = await mountList([
      makePeer({ deviceId: 'device-m', online: true, notifyMuted: true, lastMessage: null }),
      makePeer({ deviceId: 'device-n', online: true, notifyMuted: false, lastMessage: null }),
    ]);

    const supporting = wrapper.findAll('.md-list-item__supporting').map((line) => line.text());
    expect(supporting[0]).toBe('online · Notifications muted');
    expect(supporting[1]).toBe('online');
  });

  it('toggles mute through the peer id when the row asks for it', async () => {
    const { wrapper } = await mountList([makePeer({ deviceId: 'device-a', notifyMuted: false })]);

    wrapper.findComponent(UserListItem).vm.$emit('mute');
    await flushPromises();

    expect(ipc.setPeerMuted).toHaveBeenCalledWith('device-a', true);
  });

  it('reports a failed mute instead of leaving it silent', async () => {
    vi.mocked(ipc.setPeerMuted).mockRejectedValueOnce(new Error('offline'));
    const { wrapper, pinia } = await mountList([makePeer({ deviceId: 'device-a' })]);

    wrapper.findComponent(UserListItem).vm.$emit('mute');
    await flushPromises();

    const ui = useUiStore(pinia);
    expect(ui.notice?.key).toBe('error.internal');
    expect(ui.notice?.tone).toBe('error');
  });

  it('forgets the peer, clears the conversation and leaves the address', async () => {
    const { wrapper, router, pinia } = await mountList([makePeer({ deviceId: 'device-a' })], {
      route: '/chat/device-a',
    });
    usePeerStore(pinia).select('device-a');
    const clear = vi.spyOn(useChatStore(pinia), 'clear');

    wrapper.findComponent(UserListItem).vm.$emit('forget');
    await flushPromises();
    wrapper.findComponent(ForgetDialog).vm.$emit('confirmed', true);
    await flushPromises();

    expect(ipc.forgetPeer).toHaveBeenCalledWith('device-a', true);
    expect(clear).toHaveBeenCalledWith('device-a');
    expect(router.currentRoute.value.params.deviceId).toBeUndefined();
  });

  it('keeps the conversation when a failed forget leaves the peer in place', async () => {
    vi.mocked(ipc.forgetPeer).mockRejectedValueOnce(new Error('busy'));
    const { wrapper, pinia } = await mountList([makePeer({ deviceId: 'device-a' })]);
    const clear = vi.spyOn(useChatStore(pinia), 'clear');

    wrapper.findComponent(UserListItem).vm.$emit('forget');
    await nextTick();
    wrapper.findComponent(ForgetDialog).vm.$emit('confirmed', true);
    await flushPromises();

    expect(clear).not.toHaveBeenCalled();
    expect(useUiStore(pinia).notice?.key).toBe('error.internal');
  });
});
