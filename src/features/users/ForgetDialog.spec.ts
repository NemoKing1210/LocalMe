// @vitest-environment happy-dom
import type { VueWrapper } from '@vue/test-utils';
import { nextTick } from 'vue';
import { describe, expect, it } from 'vitest';

import { makePeer } from '@/test/factories';
import { mountView } from '@/test/mount';

import ForgetDialog from './ForgetDialog.vue';

/**
 * The dialog is mounted with no target first and then given one, the way the list drives it;
 * that transition is also what runs `<dialog>.showModal`.
 */
async function openDialog(): Promise<VueWrapper> {
  const wrapper = await mountView(ForgetDialog, {
    props: { peer: null },
    attachTo: document.body,
  });
  await wrapper.setProps({ peer: makePeer({ nickname: 'Alice' }) });
  await nextTick();
  return wrapper;
}

describe('ForgetDialog', () => {
  it('stays closed while there is nobody to forget', async () => {
    const wrapper = await mountView(ForgetDialog, { props: { peer: null } });

    expect((wrapper.get('dialog').element as HTMLDialogElement).open).toBe(false);
    expect(wrapper.find('.forget__body').exists()).toBe(false);
    expect(wrapper.find('.md-switch').exists()).toBe(false);
  });

  it('opens on the named peer and asks for confirmation', async () => {
    const wrapper = await openDialog();

    expect((wrapper.get('dialog').element as HTMLDialogElement).open).toBe(true);
    expect(wrapper.get('.md-dialog__headline').text()).toBe('Forget Alice?');
    expect(wrapper.get('.forget__body').text()).toContain('disappear from your list');
  });

  it('confirms without deleting history by default', async () => {
    const wrapper = await openDialog();

    await wrapper.get('.md-button--filled').trigger('click');

    expect(wrapper.emitted('confirmed')).toEqual([[false]]);
  });

  it('carries the delete-history choice once it is switched on', async () => {
    const wrapper = await openDialog();
    const control = wrapper.get('.md-switch__control');
    expect(control.attributes('aria-checked')).toBe('false');

    await control.trigger('click');
    expect(wrapper.get('.md-switch__control').attributes('aria-checked')).toBe('true');

    await wrapper.get('.md-button--filled').trigger('click');
    expect(wrapper.emitted('confirmed')).toEqual([[true]]);
  });

  it('cancels by closing only', async () => {
    const wrapper = await openDialog();

    await wrapper.get('.md-button--text').trigger('click');

    expect(wrapper.emitted('close')).toEqual([[]]);
    expect(wrapper.emitted('confirmed')).toBeUndefined();
  });

  it('resets the choice when the target peer changes', async () => {
    const wrapper = await openDialog();
    await wrapper.get('.md-switch__control').trigger('click');

    await wrapper.setProps({ peer: makePeer({ deviceId: 'device-b', nickname: 'Bob' }) });
    await nextTick();

    expect(wrapper.get('.md-switch__control').attributes('aria-checked')).toBe('false');
    await wrapper.get('.md-button--filled').trigger('click');
    expect(wrapper.emitted('confirmed')).toEqual([[false]]);
  });
});
