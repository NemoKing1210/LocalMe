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
  setPeerPinned: vi.fn(),
  reorderPinnedPeers: vi.fn(),
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

function pinnedRow(deviceId: string, nickname: string, pinRank: number): Peer {
  return makePeer({ deviceId, nickname, pinned: true, pinRank, unread: 0, online: false });
}

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

  it('toggles the pin through the peer id when the row asks for it', async () => {
    const { wrapper } = await mountList([makePeer({ deviceId: 'device-a', pinned: false })]);

    wrapper.findComponent(UserListItem).vm.$emit('pin');
    await flushPromises();

    expect(ipc.setPeerPinned).toHaveBeenCalledWith('device-a', true);
  });

  it('pins a row while a search is active', async () => {
    const { wrapper } = await mountList([makePeer({ deviceId: 'device-b', nickname: 'Bob' })], {
      query: 'bo',
    });

    wrapper.findComponent(UserListItem).vm.$emit('pin');
    await flushPromises();

    expect(ipc.setPeerPinned).toHaveBeenCalledWith('device-b', true);
  });

  it('reports a failed pin instead of leaving it silent', async () => {
    vi.mocked(ipc.setPeerPinned).mockRejectedValueOnce(new Error('offline'));
    const { wrapper, pinia } = await mountList([makePeer({ deviceId: 'device-a' })]);

    wrapper.findComponent(UserListItem).vm.$emit('pin');
    await flushPromises();

    expect(useUiStore(pinia).notice?.key).toBe('error.internal');
  });

  it('moves a pinned row with Alt+ArrowDown and persists the new order', async () => {
    const { wrapper } = await mountList([
      pinnedRow('device-a', 'Ann', 0),
      pinnedRow('device-b', 'Ben', 1),
      pinnedRow('device-c', 'Cid', 2),
    ]);

    await wrapper.findAll('.users__row')[0]!.get('.md-list-item__row').trigger('keydown', {
      key: 'ArrowDown',
      altKey: true,
    });
    await flushPromises();

    expect(ipc.reorderPinnedPeers).toHaveBeenCalledWith(['device-b', 'device-a', 'device-c']);
  });

  it('does not move the first pinned row upwards or an unpinned row at all', async () => {
    const { wrapper } = await mountList([
      pinnedRow('device-a', 'Ann', 0),
      makePeer({ deviceId: 'device-b', pinned: false }),
    ]);

    await wrapper.findAll('.users__row')[0]!.get('.md-list-item__row').trigger('keydown', {
      key: 'ArrowUp',
      altKey: true,
    });
    await wrapper.findAll('.users__row')[1]!.get('.md-list-item__row').trigger('keydown', {
      key: 'ArrowDown',
      altKey: true,
    });
    await flushPromises();

    expect(ipc.reorderPinnedPeers).not.toHaveBeenCalled();
  });

  it('reorders by dragging a pinned row across another', async () => {
    const { wrapper } = await mountList([
      pinnedRow('device-a', 'Ann', 0),
      pinnedRow('device-b', 'Ben', 1),
    ]);
    const first = wrapper.findAll('.users__row')[0]!.element;

    first.dispatchEvent(
      new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientY: 10 }),
    );
    window.dispatchEvent(new PointerEvent('pointermove', { pointerId: 1, clientY: 90 }));
    window.dispatchEvent(new PointerEvent('pointerup', { pointerId: 1, clientY: 90 }));
    await flushPromises();

    expect(ipc.reorderPinnedPeers).toHaveBeenCalledWith(['device-b', 'device-a']);
  });

  it('keeps the dragged row under the pointer when the list is scrolled', async () => {
    const { wrapper } = await mountList([
      pinnedRow('device-a', 'Ann', 0),
      pinnedRow('device-b', 'Ben', 1),
    ]);
    wrapper.get('.users__scroll').element.scrollTop = 100;
    const first = wrapper.findAll('.users__row')[0]!.element;

    first.dispatchEvent(
      new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientY: 10 }),
    );
    window.dispatchEvent(new PointerEvent('pointermove', { pointerId: 1, clientY: 90 }));
    await flushPromises();

    // Row centres live in content space: listTop - scrollTop + index * 72 + 36. After the move
    // the dragged row sits at index 1, so its centre is 8 and the pointer is 82 pixels below it.
    expect(wrapper.findAll('.users__row')[1]!.attributes('style')).toContain('translateY(82px)');
    wrapper.unmount();
  });

  it('does not swallow the first click after a drag', async () => {
    const { wrapper, router } = await mountList([
      pinnedRow('device-a', 'Ann', 0),
      pinnedRow('device-b', 'Ben', 1),
      makePeer({ deviceId: 'device-c', nickname: 'Cid', pinned: false }),
    ]);
    const rows = wrapper.findAll('.users__row');

    rows[0]!.element.dispatchEvent(
      new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientY: 10 }),
    );
    window.dispatchEvent(new PointerEvent('pointermove', { pointerId: 1, clientY: 90 }));
    window.dispatchEvent(new PointerEvent('pointerup', { pointerId: 1, clientY: 90 }));
    await flushPromises();

    // The click that ends a drag lands on the list, not on a row, so the suppression flag must
    // not survive into the next gesture — every new click is preceded by its own press.
    rows[2]!.element.dispatchEvent(
      new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 2, clientY: 250 }),
    );
    window.dispatchEvent(new PointerEvent('pointerup', { pointerId: 2, clientY: 250 }));
    await rows[2]!.get('.md-list-item__row').trigger('click');
    await flushPromises();

    expect(router.currentRoute.value.name).toBe('chat');
    expect(router.currentRoute.value.params.deviceId).toBe('device-c');
  });

  it('leaves a press without movement as a normal click', async () => {
    const { wrapper, router } = await mountList([pinnedRow('device-a', 'Ann', 0)]);

    await wrapper.get('.md-list-item__row').trigger('click');
    await flushPromises();

    expect(ipc.reorderPinnedPeers).not.toHaveBeenCalled();
    expect(router.currentRoute.value.name).toBe('chat');
  });

  it('does not reorder while a search filter is active', async () => {
    const { wrapper } = await mountList(
      [pinnedRow('device-a', 'Ann', 0), pinnedRow('device-b', 'Ben', 1)],
      { query: 'a' },
    );

    await wrapper.findAll('.users__row')[0]!.get('.md-list-item__row').trigger('keydown', {
      key: 'ArrowDown',
      altKey: true,
    });
    await flushPromises();

    expect(ipc.reorderPinnedPeers).not.toHaveBeenCalled();
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
