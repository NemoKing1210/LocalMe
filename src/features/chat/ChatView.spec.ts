// @vitest-environment happy-dom
import { createPinia, setActivePinia, type Pinia } from 'pinia';
import { defineComponent, h, nextTick, type PropType } from 'vue';
import type { VueWrapper } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Router } from 'vue-router';

import { ROUTE } from '@/app/routes';
import { TWO_PANE_QUERY } from '@/composables/useMediaQuery';
import { translate, useI18n } from '@/i18n';
import * as ipc from '@/ipc';
import type { Peer } from '@/ipc';
import { useChatStore } from '@/stores/chat';
import { usePeerStore } from '@/stores/peers';
import { useUiStore } from '@/stores/ui';
import { FIXED_NOW, makeMessage, makePeer } from '@/test/factories';
import { setMediaMatches } from '@/test/matchMedia';
import { createTestRouter, flushPromises, mountView } from '@/test/mount';
import MdAvatar from '@/ui/MdAvatar.vue';
import MdEmptyState from '@/ui/MdEmptyState.vue';
import MdIconButton from '@/ui/MdIconButton.vue';
import MdTopAppBar from '@/ui/MdTopAppBar.vue';

vi.mock('@/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/ipc')>()),
  history: vi.fn(),
  sendMessage: vi.fn(),
  setActiveChat: vi.fn(),
}));

import ChatView from './ChatView.vue';

// The conversation and the composer bring their own heavy dependencies (a virtual list over the
// chat store and the window-level drop listener). The view's own job is routing the peer and the
// store calls to them, so both are replaced by recorders.
const MessageListStub = defineComponent({
  name: 'MessageList',
  props: { peer: { type: Object as PropType<Peer>, required: true } },
  emits: ['loadOlder'],
  setup: () => () => h('div', { class: 'message-list-stub' }),
});

const ChatComposerStub = defineComponent({
  name: 'ChatComposer',
  props: {
    peer: { type: Object as PropType<Peer>, required: true },
    sending: { type: Boolean, required: true },
  },
  emits: ['send'],
  setup: () => () => h('div', { class: 'composer-stub' }),
});

const stubs = { MessageList: MessageListStub, ChatComposer: ChatComposerStub };

function backButton(wrapper: VueWrapper): VueWrapper | undefined {
  return wrapper.findAllComponents(MdIconButton).find((button) => button.props('icon') === 'back');
}

function harness(peers: readonly Peer[] = [makePeer()]): Pinia {
  const pinia = createPinia();
  setActivePinia(pinia);
  usePeerStore().replace(peers);
  return pinia;
}

async function open(
  route: string,
  peers: readonly Peer[] = [makePeer()],
): Promise<{ wrapper: VueWrapper; router: Router }> {
  const router = createTestRouter();
  const wrapper = await mountView(ChatView, { route, pinia: harness(peers), router, stubs });
  await flushPromises();
  return { wrapper, router };
}

beforeEach(() => {
  vi.mocked(ipc.history).mockResolvedValue([]);
  vi.mocked(ipc.setActiveChat).mockResolvedValue(undefined);
  vi.mocked(ipc.sendMessage).mockResolvedValue(makeMessage());
});

describe('the chat view', () => {
  it('shows the placeholder and clears the conversation when the route has no device', async () => {
    const pinia = harness();
    const chat = useChatStore();
    chat.peerId = 'device-a';
    chat.messages = [makeMessage()];

    const router = createTestRouter();
    const wrapper = await mountView(ChatView, { route: '/chat', pinia, router, stubs });
    await flushPromises();

    expect(chat.peerId).toBeNull();
    expect(chat.messages).toEqual([]);
    expect(wrapper.findComponent(MdEmptyState).exists()).toBe(true);
    expect(wrapper.findComponent(MdTopAppBar).exists()).toBe(false);
    expect(ipc.history).not.toHaveBeenCalled();
    expect(ipc.setActiveChat).toHaveBeenLastCalledWith(null);
  });

  it('opens the conversation named by the route and tells the host which chat is on screen', async () => {
    vi.mocked(ipc.history).mockResolvedValue([makeMessage()]);
    await open('/chat/device-a');

    expect(useChatStore().peerId).toBe('device-a');
    expect(useChatStore().messages).toHaveLength(1);
    expect(ipc.history).toHaveBeenCalledWith('device-a', null, 50);
    expect(ipc.setActiveChat).toHaveBeenLastCalledWith('device-a');
  });

  it('renders the peer chrome and hands the peer to the list and the composer', async () => {
    const peer = makePeer({
      deviceId: 'device-a',
      nickname: 'Alice',
      avatarSeed: 'seed-a',
      online: true,
    });
    const { wrapper } = await open('/chat/device-a', [peer]);

    const bar = wrapper.findComponent(MdTopAppBar);
    expect(bar.props('title')).toBe('Alice');
    expect(bar.props('subtitle')).toBe(translate('users.online'));

    const avatar = wrapper.findComponent(MdAvatar);
    expect(avatar.props('seed')).toBe('seed-a');
    expect(avatar.props('name')).toBe('Alice');

    expect(wrapper.findComponent(MessageListStub).props('peer')).toMatchObject({
      deviceId: 'device-a',
    });
    const composer = wrapper.findComponent(ChatComposerStub);
    expect(composer.props('peer')).toMatchObject({ deviceId: 'device-a' });
    expect(composer.props('sending')).toBe(false);
  });

  it('uses the never-seen subtitle for a peer that has not been seen yet', async () => {
    const peer = makePeer({ online: false, lastSeenMs: null });
    const { wrapper } = await open('/chat/device-a', [peer]);

    expect(wrapper.findComponent(MdTopAppBar).props('subtitle')).toBe(translate('users.neverSeen'));
  });

  it('renders the relative last-seen time for a peer that is away', async () => {
    const peer = makePeer({ online: false, lastSeenMs: FIXED_NOW });
    const { wrapper } = await open('/chat/device-a', [peer]);

    const relative = useI18n().relative(FIXED_NOW);
    expect(wrapper.findComponent(MdTopAppBar).props('subtitle')).toBe(
      translate('users.lastSeen', { relative }),
    );
  });

  it('shows the back control in one-pane mode and hides it when the layout is wide', async () => {
    setMediaMatches(TWO_PANE_QUERY, true);
    const { wrapper } = await open('/chat/device-a');
    expect(backButton(wrapper)).toBeUndefined();

    setMediaMatches(TWO_PANE_QUERY, false);
    await nextTick();
    expect(backButton(wrapper)).toBeDefined();

    setMediaMatches(TWO_PANE_QUERY, true);
    await nextTick();
    expect(backButton(wrapper)).toBeUndefined();
  });

  it('navigates back to the chat list', async () => {
    const { wrapper, router } = await open('/chat/device-a');
    const back = backButton(wrapper);
    expect(back).toBeDefined();

    await back?.trigger('click');
    await flushPromises();

    expect(router.currentRoute.value.name).toBe(ROUTE.chat);
    expect(router.currentRoute.value.params['deviceId']).toBeUndefined();
  });

  it('leaves the conversation when the peer disappears under it', async () => {
    const { wrapper, router } = await open('/chat/device-a');
    expect(wrapper.findComponent(MdTopAppBar).exists()).toBe(true);

    usePeerStore().replace([]);
    await flushPromises();
    await nextTick();

    expect(router.currentRoute.value.name).toBe(ROUTE.chat);
    expect(router.currentRoute.value.params['deviceId']).toBeUndefined();
    expect(wrapper.findComponent(MdEmptyState).exists()).toBe(true);
    expect(ipc.setActiveChat).toHaveBeenLastCalledWith(null);
  });

  it('reports a failed send and keeps the view usable', async () => {
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    vi.mocked(ipc.sendMessage).mockRejectedValue(new Error('network down'));
    const { wrapper } = await open('/chat/device-a');

    wrapper.findComponent(ChatComposerStub).vm.$emit('send', 'hello', []);
    await flushPromises();

    const ui = useUiStore();
    expect(ui.notice?.key).toBe('error.network');
    expect(ui.notice?.tone).toBe('error');
    expect(errorSpy).toHaveBeenCalled();
    expect(wrapper.findComponent(MdTopAppBar).exists()).toBe(true);
  });

  it('reports a failed attempt to read older messages', async () => {
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const { wrapper } = await open('/chat/device-a');
    useChatStore().messages = [makeMessage()];
    vi.mocked(ipc.history).mockRejectedValue(new Error('storage gone'));

    wrapper.findComponent(MessageListStub).vm.$emit('loadOlder');
    await flushPromises();

    expect(useUiStore().notice?.key).toBe('error.storage');
    expect(errorSpy).toHaveBeenCalled();
    expect(wrapper.findComponent(ChatComposerStub).exists()).toBe(true);
  });
});
