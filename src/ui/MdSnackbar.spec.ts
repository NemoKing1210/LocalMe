// @vitest-environment happy-dom
import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';

import MdSnackbar from './MdSnackbar.vue';

describe('MdSnackbar', () => {
  it('renders nothing while closed', () => {
    const wrapper = mount(MdSnackbar, { props: { open: false, text: 'Saved', action: 'Dismiss' } });

    expect(wrapper.find('.md-snackbar').exists()).toBe(false);
  });

  it('shows the text it is given', () => {
    const wrapper = mount(MdSnackbar, {
      props: { open: true, text: 'Peer connected', action: 'Dismiss' },
    });

    expect(wrapper.get('.md-snackbar__text').text()).toBe('Peer connected');
  });

  it('updates when the text prop changes', async () => {
    const wrapper = mount(MdSnackbar, { props: { open: true, text: 'one', action: 'Dismiss' } });

    await wrapper.setProps({ text: 'two' });

    expect(wrapper.get('.md-snackbar__text').text()).toBe('two');
  });

  it('is an ARIA status region', () => {
    const wrapper = mount(MdSnackbar, { props: { open: true, text: 'x', action: 'Dismiss' } });

    const snackbar = wrapper.get('.md-snackbar');
    expect(snackbar.attributes('role')).toBe('status');
    expect(snackbar.attributes('aria-live')).toBe('polite');
  });

  it('shows the action label it is given and emits dismiss when it is clicked', async () => {
    const wrapper = mount(MdSnackbar, {
      props: { open: true, text: 'x', action: 'Verwerfen' },
    });

    const action = wrapper.get('.md-snackbar__action');
    expect(action.text()).toBe('Verwerfen');
    await action.trigger('click');

    expect(wrapper.emitted('dismiss')).toHaveLength(1);
  });

  it('renders a custom action slot and still emits dismiss', async () => {
    const wrapper = mount(MdSnackbar, {
      props: { open: true, text: 'x', action: 'Dismiss' },
      slots: { action: 'Undo' },
    });

    const action = wrapper.get('.md-snackbar__action');
    expect(action.text()).toBe('Undo');
    await action.trigger('click');

    expect(wrapper.emitted('dismiss')).toHaveLength(1);
  });

  it('switches to the error styling only for the error tone', () => {
    const neutral = mount(MdSnackbar, { props: { open: true, text: 'x', action: 'Dismiss' } });
    const error = mount(MdSnackbar, {
      props: { open: true, text: 'x', tone: 'error', action: 'Dismiss' },
    });

    expect(neutral.get('.md-snackbar').classes()).not.toContain('md-snackbar--error');
    expect(error.get('.md-snackbar').classes()).toContain('md-snackbar--error');
  });

  it('appears when open turns true and disappears when it turns false', async () => {
    const wrapper = mount(MdSnackbar, { props: { open: false, text: 'x', action: 'Dismiss' } });

    await wrapper.setProps({ open: true });
    expect(wrapper.find('.md-snackbar').exists()).toBe(true);

    await wrapper.setProps({ open: false });
    expect(wrapper.find('.md-snackbar').exists()).toBe(false);
  });
});
