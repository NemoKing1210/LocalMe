// @vitest-environment happy-dom
import { mount } from '@vue/test-utils';
import { describe, expect, it, vi } from 'vitest';

import MdDialog from './MdDialog.vue';

const OPEN_PROPS = { open: true, headline: 'Confirm' };

describe('MdDialog', () => {
  it('does not open the native dialog on first render even when open is true', () => {
    // The `watch` on `open` is not immediate, so an initially-open dialog never calls showModal.
    const wrapper = mount(MdDialog, { props: OPEN_PROPS });

    const dialog = wrapper.get<HTMLDialogElement>('dialog').element;
    expect(dialog.open).toBe(false);
    expect(wrapper.attributes('open')).toBeUndefined();
  });

  it('opens natively when open turns true', async () => {
    const wrapper = mount(MdDialog, { props: { open: false, headline: 'Confirm' } });
    const dialog = wrapper.get<HTMLDialogElement>('dialog').element;
    expect(dialog.open).toBe(false);

    await wrapper.setProps({ open: true });

    expect(dialog.open).toBe(true);
    expect(wrapper.attributes('open')).toBe('');
  });

  it('closes natively and emits close when open turns false', async () => {
    const wrapper = mount(MdDialog, { props: { open: false, headline: 'Confirm' } });
    await wrapper.setProps({ open: true });
    const dialog = wrapper.get<HTMLDialogElement>('dialog').element;

    await wrapper.setProps({ open: false });

    expect(dialog.open).toBe(false);
    expect(wrapper.emitted('close')).toHaveLength(1);
  });

  it('renders the headline, slot content and actions inside the dialog', async () => {
    const wrapper = mount(MdDialog, {
      props: { open: false, headline: 'Clear history' },
      slots: { default: '<p class="body">This cannot be undone</p>', actions: '<b>OK</b>' },
    });
    await wrapper.setProps({ open: true });

    expect(wrapper.get('h2').text()).toBe('Clear history');
    expect(wrapper.get('.body').text()).toBe('This cannot be undone');
    expect(wrapper.get('.md-dialog__actions').text()).toBe('OK');
  });

  it('keeps its content in the DOM while closed; the native dialog hides it', () => {
    // The slot is not v-if'd: a closed <dialog> keeps its subtree and is hidden by the browser.
    const wrapper = mount(MdDialog, {
      props: { open: false, headline: 'Confirm' },
      slots: { default: '<p class="body">Body</p>' },
    });

    expect(wrapper.get('.body').text()).toBe('Body');
    expect(wrapper.attributes('open')).toBeUndefined();
  });

  it('labels itself by the id of its own headline', () => {
    const wrapper = mount(MdDialog, { props: OPEN_PROPS });

    const labelledBy = wrapper.attributes('aria-labelledby');
    expect(labelledBy).toBeTruthy();
    expect(wrapper.get('h2').attributes('id')).toBe(labelledBy);
  });

  it('adds the wide modifier only when wide is set', () => {
    const normal = mount(MdDialog, { props: OPEN_PROPS });
    const wide = mount(MdDialog, { props: { ...OPEN_PROPS, wide: true } });

    expect(normal.classes()).not.toContain('md-dialog--wide');
    expect(wide.classes()).toContain('md-dialog--wide');
  });

  it('emits close from the native close event', async () => {
    const wrapper = mount(MdDialog, { props: { open: false, headline: 'Confirm' } });
    await wrapper.setProps({ open: true });

    wrapper.get('dialog').element.dispatchEvent(new Event('close'));

    expect(wrapper.emitted('close')).toHaveLength(1);
  });

  it('preventDefaults native cancel (Escape) and asks the parent to close', async () => {
    const wrapper = mount(MdDialog, { props: { open: false, headline: 'Confirm' } });
    await wrapper.setProps({ open: true });
    const dialog = wrapper.get<HTMLDialogElement>('dialog').element;

    const cancel = new Event('cancel', { cancelable: true });
    dialog.dispatchEvent(cancel);

    expect(cancel.defaultPrevented).toBe(true);
    expect(wrapper.emitted('close')).toHaveLength(1);
    // The dialog itself stays open; the parent decides by flipping `open`.
    expect(dialog.open).toBe(true);
  });

  it('does not close on a backdrop click', async () => {
    const wrapper = mount(MdDialog, { props: { open: false, headline: 'Confirm' } });
    await wrapper.setProps({ open: true });
    const dialog = wrapper.get<HTMLDialogElement>('dialog').element;

    dialog.dispatchEvent(new MouseEvent('click', { bubbles: true }));

    expect(wrapper.emitted('close')).toBeUndefined();
    expect(dialog.open).toBe(true);
  });

  it('closes the open dialog when it is unmounted', async () => {
    const wrapper = mount(MdDialog, { props: { open: false, headline: 'Confirm' } });
    await wrapper.setProps({ open: true });
    const dialog = wrapper.get<HTMLDialogElement>('dialog').element;
    expect(dialog.open).toBe(true);

    wrapper.unmount();

    expect(dialog.open).toBe(false);
  });

  it('never installs a global keydown listener, so none can leak after close', async () => {
    const documentAdd = vi.spyOn(document, 'addEventListener');
    const windowAdd = vi.spyOn(window, 'addEventListener');
    const wrapper = mount(MdDialog, { props: { open: false, headline: 'Confirm' } });

    await wrapper.setProps({ open: true });
    await wrapper.setProps({ open: false });
    wrapper.unmount();

    for (const [type] of documentAdd.mock.calls) {
      expect(type).not.toBe('keydown');
    }
    for (const [type] of windowAdd.mock.calls) {
      expect(type).not.toBe('keydown');
    }
  });
});
