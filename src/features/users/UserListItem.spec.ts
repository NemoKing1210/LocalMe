// @vitest-environment happy-dom
import type { VueWrapper } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';

import type { Peer } from '@/ipc';
import { makePeer } from '@/test/factories';
import { flushPromises, mountView } from '@/test/mount';

import UserListItem from './UserListItem.vue';

async function mountItem(peer: Peer = makePeer(), selected = false): Promise<VueWrapper> {
  return mountView(UserListItem, { props: { peer, selected } });
}

describe('UserListItem', () => {
  it('uses the nickname as the headline', async () => {
    const wrapper = await mountItem(makePeer({ nickname: 'Bea' }));

    expect(wrapper.get('.md-list-item__headline').text()).toBe('Bea');
  });

  describe('the supporting line', () => {
    it('shows presence when there is no message preview', async () => {
      const wrapper = await mountItem(makePeer({ online: true, lastMessage: null }));

      expect(wrapper.get('.md-list-item__supporting').text()).toBe('online');
    });

    it('says a peer has never been online when it is offline with no last-seen time', async () => {
      const wrapper = await mountItem(
        makePeer({ online: false, lastSeenMs: null, lastMessage: null }),
      );

      expect(wrapper.get('.md-list-item__supporting').text()).toBe('never online');
    });

    it('describes when an offline peer was last seen', async () => {
      const wrapper = await mountItem(
        makePeer({ online: false, lastSeenMs: Date.now() - 3_600_000, lastMessage: null }),
      );

      expect(wrapper.get('.md-list-item__supporting').text()).toMatch(/^last seen /);
    });

    it('shows an incoming preview as its body', async () => {
      const wrapper = await mountItem(
        makePeer({ lastMessage: { direction: 'incoming', body: 'did you see this?' } }),
      );

      expect(wrapper.get('.md-list-item__supporting').text()).toBe('did you see this?');
    });

    it('prefixes an outgoing preview so it reads as the reader’s own words', async () => {
      const wrapper = await mountItem(
        makePeer({ lastMessage: { direction: 'outgoing', body: 'on my way' } }),
      );

      expect(wrapper.get('.md-list-item__supporting').text()).toBe('You: on my way');
    });

    it('appends the muted marker only when notifications are muted', async () => {
      const muted = await mountItem(
        makePeer({ online: true, notifyMuted: true, lastMessage: null }),
      );
      const loud = await mountItem(
        makePeer({ online: true, notifyMuted: false, lastMessage: null }),
      );

      expect(muted.get('.md-list-item__supporting').text()).toBe('online · Notifications muted');
      expect(loud.get('.md-list-item__supporting').text()).toBe('online');
    });
  });

  it('marks the avatar online or offline and dims the offline one', async () => {
    const online = await mountItem(makePeer({ online: true }));
    expect(online.find('.md-avatar__presence--online').exists()).toBe(true);
    expect(online.find('.md-avatar--dimmed').exists()).toBe(false);

    const offline = await mountItem(makePeer({ online: false }));
    expect(offline.find('.md-avatar__presence--offline').exists()).toBe(true);
    expect(offline.find('.md-avatar--dimmed').exists()).toBe(true);
  });

  it('shows an unread badge only while something is unread', async () => {
    const unread = await mountItem(makePeer({ unread: 3 }));
    expect(unread.get('.md-badge').text()).toBe('3');

    const read = await mountItem(makePeer({ unread: 0 }));
    expect(read.find('.md-badge').exists()).toBe(false);
  });

  it('emits activate when the row is clicked, carrying no identity of its own', async () => {
    const wrapper = await mountItem();
    await wrapper.get('.md-list-item__row').trigger('click');

    // The parent owns the peer identity; the row only reports the gesture.
    expect(wrapper.emitted('activate')).toEqual([[]]);
  });

  it('marks the selected row for assistive technology', async () => {
    const wrapper = await mountItem(makePeer(), true);

    expect(wrapper.get('.md-list-item__row').attributes('aria-current')).toBe('true');
    expect(wrapper.get('.md-list-item').classes()).toContain('md-list-item--selected');
  });

  describe('the row menu', () => {
    it('offers mute and forget, and emits mute when chosen', async () => {
      const wrapper = await mountItem(makePeer({ notifyMuted: false }));
      await wrapper.get('.md-menu__trigger').trigger('click');
      await flushPromises();

      const items = wrapper.findAll('.md-menu__item');
      expect(items.map((item) => item.text())).toEqual(['Mute notifications', 'Forget…']);

      await items[0]!.trigger('click');
      expect(wrapper.emitted('mute')).toEqual([[]]);
      expect(wrapper.emitted('forget')).toBeUndefined();
    });

    it('flips the mute label and emits forget from the danger item', async () => {
      const wrapper = await mountItem(makePeer({ notifyMuted: true }));
      await wrapper.get('.md-menu__trigger').trigger('click');
      await flushPromises();

      const items = wrapper.findAll('.md-menu__item');
      expect(items[0]!.text()).toBe('Unmute notifications');
      expect(items[1]!.classes()).toContain('md-menu__item--danger');

      await items[1]!.trigger('click');
      expect(wrapper.emitted('forget')).toEqual([[]]);
      expect(wrapper.emitted('mute')).toBeUndefined();
    });

    it('opens the menu on a context-menu gesture', async () => {
      const wrapper = await mountItem();
      await wrapper.get('.md-list-item').trigger('contextmenu');
      await flushPromises();

      expect(wrapper.find('.md-menu__surface').exists()).toBe(true);
    });
  });
});
