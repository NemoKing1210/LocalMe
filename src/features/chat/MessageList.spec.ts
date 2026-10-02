// @vitest-environment happy-dom
import { createPinia, setActivePinia, type Pinia } from 'pinia';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { nextTick } from 'vue';

import type { VueWrapper } from '@vue/test-utils';

import { useChatStore } from '@/stores/chat';
import { FIXED_NOW, makeMessage, makePeer } from '@/test/factories';
import { flushPromises, mountView } from '@/test/mount';
import MdButton from '@/ui/MdButton.vue';

import MessageList from './MessageList.vue';
import MessageSkeleton from './MessageSkeleton.vue';

// Real time makes the day headings depend on when the suite runs; a fixed clock does not.
const fixedNow = vi.hoisted(() => ({ value: Date.UTC(2026, 2, 15, 12, 0, 0) }));
vi.mock('@/composables/useNow', () => ({ useNow: () => fixedNow }));

interface VirtualItemStub {
  readonly index: number;
  readonly start: number;
  readonly end: number;
  readonly size: number;
  readonly key: string | number;
  readonly lane: number;
}

/**
 * happy-dom has no layout, so the real virtualiser measures a zero-height scroll box and returns
 * no window at all. Only `useVirtualizer` is replaced: it reports every row as visible, which lets
 * the list's own ordering, day and row-building logic run untouched.
 */
vi.mock('@tanstack/vue-virtual', () => ({
  useVirtualizer: (options: {
    readonly value: {
      readonly count: number;
      readonly getItemKey: (index: number) => string | number;
      readonly estimateSize: () => number;
    };
  }) => {
    const build = (): VirtualItemStub[] => {
      const { count, getItemKey, estimateSize } = options.value;
      const items: VirtualItemStub[] = [];
      let start = 0;
      for (let index = 0; index < count; index += 1) {
        const size = estimateSize();
        items.push({ index, start, end: start + size, size, key: getItemKey(index), lane: 0 });
        start += size;
      }
      return items;
    };
    return {
      value: {
        getVirtualItems: build,
        getTotalSize: () => build().reduce((max, item) => Math.max(max, item.end), 0),
        measureElement: () => undefined,
      },
    };
  },
}));

const PEER = makePeer();

/** One mutable view of the scroll box, since happy-dom refuses to lay it out. */
function setScrollGeometry(wrapper: VueWrapper, height: number, client: number): void {
  const scroll = wrapper.find('.list__scroll').element;
  Object.defineProperty(scroll, 'scrollHeight', { value: height, configurable: true });
  Object.defineProperty(scroll, 'clientHeight', { value: client, configurable: true });
}

let pinia: Pinia;

beforeEach(() => {
  pinia = createPinia();
  setActivePinia(pinia);
});

describe('MessageList', () => {
  it('renders messages in timestamp then id order, whatever order the store holds them in', async () => {
    const chat = useChatStore();
    chat.messages = [
      makeMessage({ id: 'c', body: 'third', sentAt: FIXED_NOW + 2_000 }),
      makeMessage({ id: 'a', body: 'first', sentAt: FIXED_NOW + 1_000 }),
      makeMessage({ id: 'b', body: 'second', sentAt: FIXED_NOW + 2_000 }),
    ];

    const wrapper = await mountView(MessageList, { props: { peer: PEER }, pinia });

    // `b` and `c` share a timestamp, so the id breaks the tie.
    expect(wrapper.findAll('.bubble__body').map((body) => body.text())).toEqual([
      'first',
      'second',
      'third',
    ]);
  });

  it('marks only the first row of each day and keeps a single floating heading', async () => {
    const chat = useChatStore();
    chat.messages = [
      makeMessage({ id: 'yesterday', body: 'yesterday', sentAt: FIXED_NOW - 86_400_000 }),
      makeMessage({ id: 'today-1', body: 'today 1', sentAt: FIXED_NOW }),
      makeMessage({ id: 'today-2', body: 'today 2', sentAt: FIXED_NOW + 1_000 }),
    ];

    const wrapper = await mountView(MessageList, { props: { peer: PEER }, pinia });

    // Two days, so two spacer rows; one heading element reused across the whole list.
    expect(wrapper.findAll('.list__row--day')).toHaveLength(2);
    expect(wrapper.findAll('.list__day')).toHaveLength(1);
    // The heading follows the topmost visible day.
    expect(wrapper.find('.list__day-chip').text()).toBe('Yesterday');
  });

  it('reuses one heading when every message falls on the same day', async () => {
    const chat = useChatStore();
    chat.messages = [
      makeMessage({ id: 'a', sentAt: FIXED_NOW }),
      makeMessage({ id: 'b', sentAt: FIXED_NOW + 1_000 }),
      makeMessage({ id: 'c', sentAt: FIXED_NOW + 2_000 }),
    ];

    const wrapper = await mountView(MessageList, { props: { peer: PEER }, pinia });

    expect(wrapper.findAll('.list__row--day')).toHaveLength(1);
    expect(wrapper.find('.list__day-chip').text()).toBe('Today');
  });

  it('moves the floating heading to the day at the top as the log scrolls', async () => {
    const chat = useChatStore();
    chat.messages = [
      makeMessage({ id: 'yesterday', sentAt: FIXED_NOW - 86_400_000 }),
      makeMessage({ id: 'today', sentAt: FIXED_NOW }),
    ];

    const wrapper = await mountView(MessageList, { props: { peer: PEER }, pinia });
    expect(wrapper.find('.list__day-chip').text()).toBe('Yesterday');

    const scroll = wrapper.find('.list__scroll');
    setScrollGeometry(wrapper, 500, 200);
    // Past the first day's row, so the second day is the one at the top.
    (scroll.element as HTMLElement).scrollTop = 40;
    await scroll.trigger('scroll');

    expect(wrapper.find('.list__day-chip').text()).toBe('Today');
  });

  it('shows nothing but the (empty) canvas when there are no messages', async () => {
    const chat = useChatStore();
    chat.messages = [];
    chat.hasMore = false;
    chat.loading = false;

    const wrapper = await mountView(MessageList, { props: { peer: PEER }, pinia });

    expect(wrapper.findAll('.list__row')).toHaveLength(0);
    expect(wrapper.find('.list__day').exists()).toBe(false);
    expect(wrapper.find('.list__end').exists()).toBe(false);
    expect(wrapper.find('.list__older').exists()).toBe(false);
    expect(wrapper.findComponent(MessageSkeleton).exists()).toBe(false);
  });

  it('labels the log with the peer it belongs to', async () => {
    const chat = useChatStore();
    chat.messages = [];

    const wrapper = await mountView(MessageList, {
      props: { peer: makePeer({ nickname: 'Alice' }) },
      pinia,
    });

    expect(wrapper.find('[role="log"]').attributes('aria-label')).toBe('Messages with Alice');
  });

  it('announces the start of the history once no page remains', async () => {
    const chat = useChatStore();
    chat.messages = [makeMessage({ sentAt: FIXED_NOW })];
    chat.hasMore = false;

    const wrapper = await mountView(MessageList, { props: { peer: PEER }, pinia });
    expect(wrapper.find('.list__end').text()).toBe('This is the beginning of the conversation');
  });

  it('offers the earlier-messages button only while the store has more, and emits on click', async () => {
    const chat = useChatStore();
    chat.messages = [makeMessage({ sentAt: FIXED_NOW })];
    chat.hasMore = true;
    chat.loading = false;

    const wrapper = await mountView(MessageList, { props: { peer: PEER }, pinia });

    const button = wrapper.findComponent(MdButton);
    expect(button.exists()).toBe(true);
    expect(button.props('busy')).toBe(false);

    await button.trigger('click');
    expect(wrapper.emitted('loadOlder')).toHaveLength(1);
  });

  it('marks the earlier-messages button busy while a page is loading', async () => {
    const chat = useChatStore();
    chat.messages = [makeMessage({ sentAt: FIXED_NOW })];
    chat.hasMore = true;
    chat.loading = true;

    const wrapper = await mountView(MessageList, { props: { peer: PEER }, pinia });
    const button = wrapper.findComponent(MdButton);
    expect(button.props('busy')).toBe(true);
    expect(button.attributes('disabled')).toBeDefined();
  });

  it('shows the skeleton while the first page is loading and no rows are present', async () => {
    const chat = useChatStore();
    chat.messages = [];
    chat.loading = true;

    const wrapper = await mountView(MessageList, { props: { peer: PEER }, pinia });
    expect(wrapper.findComponent(MessageSkeleton).exists()).toBe(true);
  });

  it('replaces the skeleton with rows once messages arrive', async () => {
    const chat = useChatStore();
    chat.messages = [makeMessage({ sentAt: FIXED_NOW })];
    chat.loading = true;

    const wrapper = await mountView(MessageList, { props: { peer: PEER }, pinia });
    expect(wrapper.findComponent(MessageSkeleton).exists()).toBe(false);
    expect(wrapper.findAll('.list__row')).toHaveLength(1);
  });

  it('stops following the tail when scrolled away and offers a jump back', async () => {
    const chat = useChatStore();
    chat.messages = [makeMessage({ id: 'a', sentAt: FIXED_NOW })];

    const wrapper = await mountView(MessageList, { props: { peer: PEER }, pinia });
    setScrollGeometry(wrapper, 1_000, 400);

    const scroll = wrapper.find('.list__scroll');
    // At the top of a tall conversation the tail is out of view, so the log lets go.
    await scroll.trigger('scroll');
    expect(wrapper.find('.list__latest').exists()).toBe(true);

    // Close enough to the bottom and it follows again.
    (scroll.element as HTMLElement).scrollTop = 550;
    await scroll.trigger('scroll');
    expect(wrapper.find('.list__latest').exists()).toBe(false);

    (scroll.element as HTMLElement).scrollTop = 0;
    await scroll.trigger('scroll');
    expect(wrapper.find('.list__latest').exists()).toBe(true);

    // Jumping puts the view back at the end and hides the control again.
    await wrapper.find('.list__latest button').trigger('click');
    expect((scroll.element as HTMLElement).scrollTop).toBe(1_000);
    await nextTick();
    expect(wrapper.find('.list__latest').exists()).toBe(false);
  });

  it('consumes the entrance flag for a message appended after mount', async () => {
    const chat = useChatStore();
    chat.peerId = PEER.deviceId;
    chat.add(makeMessage({ id: 'first', peer: PEER.deviceId, sentAt: FIXED_NOW }));

    await mountView(MessageList, { props: { peer: PEER }, pinia });

    const consume = vi.spyOn(chat, 'consumeEntrance');
    chat.add(makeMessage({ id: 'second', peer: PEER.deviceId, sentAt: FIXED_NOW + 1_000 }));
    await nextTick();

    expect(consume).toHaveBeenCalledWith('second');
    // The store still had it undrawn, so the new bubble really would animate in.
    expect(consume).toHaveReturnedWith(true);
  });

  it('restores the reader place after an earlier page is prepended', async () => {
    const chat = useChatStore();
    chat.messages = [makeMessage({ id: 'm2', sentAt: FIXED_NOW })];
    chat.hasMore = true;

    const wrapper = await mountView(MessageList, { props: { peer: PEER }, pinia });
    const scroll = wrapper.find('.list__scroll');
    // Drain the mount-time pin before giving the box a real size.
    await flushPromises();
    setScrollGeometry(wrapper, 500, 200);
    (scroll.element as HTMLElement).scrollTop = 0;

    // Away from the tail, so the follow watcher will not fight the restore.
    await scroll.trigger('scroll');
    expect(wrapper.find('.list__latest').exists()).toBe(true);
    await wrapper.findComponent(MdButton).trigger('click');

    // The prepend grows the content by exactly the page that was inserted above.
    setScrollGeometry(wrapper, 700, 200);
    chat.messages = [
      makeMessage({ id: 'm1', sentAt: FIXED_NOW - 1_000 }),
      makeMessage({ id: 'm2', sentAt: FIXED_NOW }),
    ];
    await flushPromises();

    expect((scroll.element as HTMLElement).scrollTop).toBe(200);
  });

  it('lets go of the tail when the conversation is cleared', async () => {
    const chat = useChatStore();
    chat.messages = [makeMessage({ id: 'm1', sentAt: FIXED_NOW })];

    const wrapper = await mountView(MessageList, { props: { peer: PEER }, pinia });

    chat.messages = [];
    await flushPromises();

    expect(wrapper.findAll('.list__row')).toHaveLength(0);
    expect(wrapper.find('.list__latest').exists()).toBe(false);
  });

  it('keeps appending later messages without losing the tail', async () => {
    const chat = useChatStore();
    chat.peerId = PEER.deviceId;
    chat.add(makeMessage({ id: 'm1', peer: PEER.deviceId, sentAt: FIXED_NOW }));

    const wrapper = await mountView(MessageList, { props: { peer: PEER }, pinia });

    chat.add(makeMessage({ id: 'm2', peer: PEER.deviceId, sentAt: FIXED_NOW + 1_000 }));
    await flushPromises();
    // The oldest id is unchanged now, so this exercises the "no prepend" side of the watcher.
    chat.add(makeMessage({ id: 'm3', peer: PEER.deviceId, sentAt: FIXED_NOW + 2_000 }));
    await flushPromises();

    expect(wrapper.findAll('.list__row')).toHaveLength(3);
  });
});
