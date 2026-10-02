// @vitest-environment happy-dom
import { motion } from 'motion-v';
import { createPinia, setActivePinia } from 'pinia';
import { describe, expect, it, vi } from 'vitest';

import { useI18n } from '@/i18n';
import type * as IpcModule from '@/ipc';
import type { MessageStatus } from '@/ipc';
import { useChatStore } from '@/stores/chat';
import { useUiStore } from '@/stores/ui';
import { makeAttachment, makeMessage, FIXED_NOW } from '@/test/factories';
import { flushPromises, mountView } from '@/test/mount';
import MdIcon from '@/ui/MdIcon.vue';
import MdMenu from '@/ui/MdMenu.vue';

import MessageBubble from './MessageBubble.vue';

// The bubble renders attachment cards, which run host commands through the IPC boundary; a real
// `invoke` must never fire from a test.
vi.mock('@/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof IpcModule>()),
}));

const i18nApi = useI18n();

describe('MessageBubble', () => {
  it('renders the body as literal text, never as markup', async () => {
    const wrapper = await mountView(MessageBubble, {
      props: { message: makeMessage({ body: '<b>x</b>' }), showStatus: true },
    });

    const body = wrapper.find('.bubble__body');
    expect(body.text()).toBe('<b>x</b>');
    // The angle brackets have to survive as characters, not become an element.
    expect(body.element.querySelector('b')).toBeNull();
  });

  it('omits the body paragraph when the message carries only files', async () => {
    const wrapper = await mountView(MessageBubble, {
      props: { message: makeMessage({ body: null }), showStatus: true },
    });
    expect(wrapper.find('.bubble__body').exists()).toBe(false);
  });

  it('marks the direction with a class on the bubble', async () => {
    const outgoing = await mountView(MessageBubble, {
      props: { message: makeMessage({ direction: 'outgoing' }), showStatus: true },
    });
    expect(outgoing.find('article.bubble').classes()).toContain('bubble--outgoing');
    expect(outgoing.find('article.bubble').classes()).not.toContain('bubble--incoming');

    const incoming = await mountView(MessageBubble, {
      props: { message: makeMessage({ direction: 'incoming' }), showStatus: true },
    });
    expect(incoming.find('article.bubble').classes()).toContain('bubble--incoming');
    expect(incoming.find('article.bubble').classes()).not.toContain('bubble--outgoing');
  });

  it('renders an attachment card for every attachment, in order', async () => {
    const wrapper = await mountView(MessageBubble, {
      props: {
        message: makeMessage({
          body: null,
          attachments: [
            makeAttachment({ id: 'a1', name: 'one.txt' }),
            makeAttachment({ id: 'a2', name: 'two.txt' }),
          ],
        }),
        showStatus: true,
      },
    });

    expect(wrapper.findAll('.attachment')).toHaveLength(2);
    expect(wrapper.findAll('.attachment__name').map((name) => name.text())).toEqual([
      'one.txt',
      'two.txt',
    ]);
  });

  it('shows no file stack when there are no attachments', async () => {
    const wrapper = await mountView(MessageBubble, {
      props: { message: makeMessage({ attachments: [] }), showStatus: true },
    });
    expect(wrapper.find('.bubble__files').exists()).toBe(false);
  });

  it('shows a second time line only for an outgoing message that waited for delivery', async () => {
    const wrapper = await mountView(MessageBubble, {
      props: {
        message: makeMessage({
          direction: 'outgoing',
          sentAt: FIXED_NOW,
          deliveredAt: FIXED_NOW + 30_000,
        }),
        showStatus: true,
      },
    });

    const times = wrapper.findAll('.bubble__time');
    expect(times).toHaveLength(2);
    expect(times[0]?.text()).toBe(i18nApi.clock(FIXED_NOW));
    expect(times[1]?.text()).toBe(i18nApi.clock(FIXED_NOW + 30_000));
    expect(wrapper.find('.bubble__arrow').exists()).toBe(true);
    expect(times[1]?.attributes('title')).toBe(
      i18nApi.t('chat.deliveredAt', { time: i18nApi.clock(FIXED_NOW + 30_000) }),
    );
  });

  it('does not show the delivery line for a quick send, a pending send, or an incoming message', async () => {
    // The threshold is inclusive: exactly five seconds counts as "waited".
    const quick = await mountView(MessageBubble, {
      props: {
        message: makeMessage({
          direction: 'outgoing',
          sentAt: FIXED_NOW,
          deliveredAt: FIXED_NOW + 4_999,
        }),
        showStatus: true,
      },
    });
    expect(quick.findAll('.bubble__time')).toHaveLength(1);

    const pending = await mountView(MessageBubble, {
      props: {
        message: makeMessage({ direction: 'outgoing', deliveredAt: null, status: 'sending' }),
        showStatus: true,
      },
    });
    expect(pending.findAll('.bubble__time')).toHaveLength(1);
    expect(pending.find('.bubble__arrow').exists()).toBe(false);

    // Even with a delayed acknowledgement, a message that is not ours says nothing about it.
    const incoming = await mountView(MessageBubble, {
      props: {
        message: makeMessage({ direction: 'incoming', deliveredAt: FIXED_NOW + 30_000 }),
        showStatus: true,
      },
    });
    expect(incoming.findAll('.bubble__time')).toHaveLength(1);
    expect(incoming.find('.bubble__arrow').exists()).toBe(false);
  });

  it.each<{ status: MessageStatus; icon: string; label: string | null }>([
    { status: 'queued', icon: 'clock', label: 'Waiting to send' },
    { status: 'sending', icon: 'clock', label: 'Sending…' },
    { status: 'delivered', icon: 'check-all', label: 'Delivered' },
    { status: 'received', icon: '', label: null },
  ])('shows the $status glyph as $icon', async ({ status, icon, label }) => {
    const wrapper = await mountView(MessageBubble, {
      props: { message: makeMessage({ direction: 'outgoing', status }), showStatus: true },
    });

    if (label === null) {
      // `received` is not an outgoing state and says nothing.
      expect(wrapper.find('.bubble__status').exists()).toBe(false);
      return;
    }
    expect(wrapper.find('.bubble__status').exists()).toBe(true);
    expect(wrapper.findComponent(MdIcon).props('name')).toBe(icon);
    expect(wrapper.find('.bubble__status svg').attributes('title')).toBe(label);
  });

  it('hides the status glyph when the list asks it not to show one', async () => {
    const wrapper = await mountView(MessageBubble, {
      props: {
        message: makeMessage({ direction: 'outgoing', status: 'delivered' }),
        showStatus: false,
      },
    });
    expect(wrapper.find('.bubble__status').exists()).toBe(false);
  });

  it('never shows a status glyph on an incoming message', async () => {
    const wrapper = await mountView(MessageBubble, {
      props: {
        message: makeMessage({ direction: 'incoming', status: 'delivered' }),
        showStatus: true,
      },
    });
    expect(wrapper.find('.bubble__status').exists()).toBe(false);
  });

  it('exposes the localised time as plain clock text', async () => {
    const wrapper = await mountView(MessageBubble, {
      props: { message: makeMessage({ sentAt: FIXED_NOW, deliveredAt: null }), showStatus: true },
    });
    expect(wrapper.find('.bubble__time').text()).toBe(i18nApi.clock(FIXED_NOW));
  });
});

describe('MessageBubble context menu, clipboard and entrance', () => {
  it('offers the selection and message items, and opens the menu at the pointer', async () => {
    const wrapper = await mountView(MessageBubble, {
      props: { message: makeMessage({ body: 'hello world' }), showStatus: true },
    });
    const article = wrapper.find('article.bubble');
    const node = wrapper.find('.bubble__body').element;
    vi.spyOn(window, 'getSelection').mockReturnValue({
      isCollapsed: false,
      anchorNode: node,
      focusNode: node,
      toString: () => 'world',
    } as unknown as Selection);
    const menu = wrapper.findComponent(MdMenu);

    await article.trigger('contextmenu', { clientX: 120, clientY: 44 });
    await flushPromises();

    expect(menu.props('items')).toEqual([
      { id: 'selection', label: 'Copy selection', icon: 'copy' },
      { id: 'message', label: 'Copy message', icon: 'copy' },
    ]);
    // `show()` placed the surface at the pointer.
    const surface = menu.find('.md-menu__surface');
    expect(surface.exists()).toBe(true);
    expect(surface.attributes('style')).toContain('left: 120px');
    expect(surface.attributes('style')).toContain('top: 44px');
  });

  it('offers only the message item for a collapsed selection', async () => {
    const wrapper = await mountView(MessageBubble, {
      props: { message: makeMessage({ body: 'hello' }), showStatus: true },
    });
    vi.spyOn(window, 'getSelection').mockReturnValue({
      isCollapsed: true,
      anchorNode: null,
      focusNode: null,
      toString: () => '',
    } as unknown as Selection);
    const menu = wrapper.findComponent(MdMenu);

    await wrapper.find('article.bubble').trigger('contextmenu', { clientX: 1, clientY: 1 });

    expect(menu.props('items')).toEqual([{ id: 'message', label: 'Copy message', icon: 'copy' }]);
  });

  it('ignores a selection that reaches outside the bubble', async () => {
    const wrapper = await mountView(MessageBubble, {
      props: { message: makeMessage({ body: 'hello' }), showStatus: true },
    });
    // Anchor and focus both live in the document body, not in the message.
    vi.spyOn(window, 'getSelection').mockReturnValue({
      isCollapsed: false,
      anchorNode: document.body,
      focusNode: document.body,
      toString: () => 'outside',
    } as unknown as Selection);
    const menu = wrapper.findComponent(MdMenu);

    await wrapper.find('article.bubble').trigger('contextmenu', { clientX: 1, clientY: 1 });

    expect(menu.props('items')).toEqual([{ id: 'message', label: 'Copy message', icon: 'copy' }]);
  });

  it('does not open a menu for a files-only message with nothing selected', async () => {
    const wrapper = await mountView(MessageBubble, {
      props: {
        message: makeMessage({ body: null, attachments: [makeAttachment()] }),
        showStatus: true,
      },
    });
    vi.spyOn(window, 'getSelection').mockReturnValue({
      isCollapsed: true,
      anchorNode: null,
      focusNode: null,
      toString: () => '',
    } as unknown as Selection);
    const menu = wrapper.findComponent(MdMenu);

    await wrapper.find('article.bubble').trigger('contextmenu', { clientX: 3, clientY: 3 });
    await flushPromises();

    expect(menu.props('items')).toEqual([]);
    expect(menu.find('.md-menu__surface').exists()).toBe(false);
  });

  it('offers only the selection item for a files-only message when text is selected', async () => {
    const wrapper = await mountView(MessageBubble, {
      props: {
        message: makeMessage({ body: null, attachments: [makeAttachment()] }),
        showStatus: true,
      },
    });
    const node = wrapper.find('.attachment').element;
    vi.spyOn(window, 'getSelection').mockReturnValue({
      isCollapsed: false,
      anchorNode: node,
      focusNode: node,
      toString: () => 'notes',
    } as unknown as Selection);
    const menu = wrapper.findComponent(MdMenu);

    await wrapper.find('article.bubble').trigger('contextmenu', { clientX: 7, clientY: 7 });
    await flushPromises();

    expect(menu.props('items')).toEqual([
      { id: 'selection', label: 'Copy selection', icon: 'copy' },
    ]);
    // Observed defect: `onContextMenu` sets the selection then calls `menu.show()` in the same
    // tick, but `MdMenu.show()` guards on its `items` prop, which Vue has not re-rendered yet.
    // A files-only message has no message item to keep the list non-empty, so the menu stays shut
    // and the selection cannot be copied from here.
    expect(menu.find('.md-menu__surface').exists()).toBe(false);
  });

  it('copies the whole message and reports success', async () => {
    const writeText = vi.fn<(text: string) => Promise<void>>();
    writeText.mockResolvedValue(undefined);
    Object.defineProperty(window.navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });
    const wrapper = await mountView(MessageBubble, {
      props: { message: makeMessage({ body: 'hello' }), showStatus: true },
    });
    const ui = useUiStore();
    const notify = vi.spyOn(ui, 'notify');

    wrapper.findComponent(MdMenu).vm.$emit('select', 'message');
    await flushPromises();

    expect(writeText).toHaveBeenCalledWith('hello');
    expect(notify).toHaveBeenCalledWith('chat.copied');
  });

  it('copies the captured selection rather than the live one', async () => {
    const writeText = vi.fn<(text: string) => Promise<void>>();
    writeText.mockResolvedValue(undefined);
    Object.defineProperty(window.navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });
    const wrapper = await mountView(MessageBubble, {
      props: { message: makeMessage({ body: 'hello world' }), showStatus: true },
    });
    const node = wrapper.find('.bubble__body').element;
    vi.spyOn(window, 'getSelection').mockReturnValue({
      isCollapsed: false,
      anchorNode: node,
      focusNode: node,
      toString: () => 'world',
    } as unknown as Selection);
    const ui = useUiStore();
    const notify = vi.spyOn(ui, 'notify');
    const menu = wrapper.findComponent(MdMenu);

    // The menu remembers the selection at the moment it was opened.
    await wrapper.find('article.bubble').trigger('contextmenu', { clientX: 2, clientY: 2 });
    menu.vm.$emit('select', 'selection');
    await flushPromises();

    expect(writeText).toHaveBeenCalledWith('world');
    expect(notify).toHaveBeenCalledWith('chat.copied');
  });

  it('reports an internal failure when the clipboard rejects', async () => {
    const writeText = vi.fn<(text: string) => Promise<void>>();
    writeText.mockRejectedValue(new Error('denied'));
    Object.defineProperty(window.navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });
    const wrapper = await mountView(MessageBubble, {
      props: { message: makeMessage({ body: 'hello' }), showStatus: true },
    });
    const ui = useUiStore();
    const fail = vi.spyOn(ui, 'fail');
    const notify = vi.spyOn(ui, 'notify');
    const logged = vi.spyOn(console, 'error').mockImplementation(() => {});

    wrapper.findComponent(MdMenu).vm.$emit('select', 'message');
    await flushPromises();

    expect(fail).toHaveBeenCalledWith('error.internal');
    expect(notify).not.toHaveBeenCalled();
    expect(logged).toHaveBeenCalled();
  });

  it('animates a message whose entrance flag is still undrawn', async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    const chat = useChatStore();
    const message = makeMessage({ id: 'm1', body: 'hi', sentAt: FIXED_NOW });
    chat.peerId = message.peer;
    chat.add(message);
    const consume = vi.spyOn(chat, 'consumeEntrance');

    const wrapper = await mountView(MessageBubble, {
      props: { message, showStatus: true },
      pinia,
    });

    expect(consume).toHaveBeenCalledTimes(1);
    expect(consume).toHaveBeenCalledWith('m1');
    expect(consume).toHaveReturnedWith(true);
    expect(wrapper.findComponent(motion.article).props('initial')).toEqual({
      opacity: 0,
      y: 12,
      scale: 0.98,
    });
  });

  it('does not animate a message the store has already consumed', async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    const chat = useChatStore();
    const consume = vi.spyOn(chat, 'consumeEntrance');

    const wrapper = await mountView(MessageBubble, {
      props: { message: makeMessage({ id: 'm2' }), showStatus: true },
      pinia,
    });

    expect(consume).toHaveReturnedWith(false);
    expect(wrapper.findComponent(motion.article).props('initial')).toBe(false);
  });
});
