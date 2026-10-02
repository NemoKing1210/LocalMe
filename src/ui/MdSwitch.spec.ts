// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest';

import { mountView } from '@/test/mount';
import MdSwitch from './MdSwitch.vue';

const baseProps = { modelValue: false, label: 'Notifications' };

describe('MdSwitch', () => {
  it('renders the label and an off switch by default', async () => {
    const wrapper = await mountView(MdSwitch, { props: baseProps });

    expect(wrapper.get('.md-typescale-body-large').text()).toBe('Notifications');
    const control = wrapper.get('[role="switch"]');
    expect(control.attributes('type')).toBe('button');
    expect(control.attributes('aria-label')).toBe('Notifications');
    expect(control.attributes('aria-checked')).toBe('false');
    expect(control.attributes('disabled')).toBeUndefined();
    expect(wrapper.find('.md-switch__track--on').exists()).toBe(false);
    expect(wrapper.find('.md-switch__thumb--on').exists()).toBe(false);
  });

  it('shows the on state when the model is true', async () => {
    const wrapper = await mountView(MdSwitch, { props: { ...baseProps, modelValue: true } });

    expect(wrapper.get('[role="switch"]').attributes('aria-checked')).toBe('true');
    expect(wrapper.get('.md-switch__track').classes()).toContain('md-switch__track--on');
    expect(wrapper.get('.md-switch__thumb').classes()).toContain('md-switch__thumb--on');
  });

  it('renders the supporting text only when given', async () => {
    const withSupporting = await mountView(MdSwitch, {
      props: { ...baseProps, supportingText: 'Muted while away' },
    });
    expect(withSupporting.get('.md-switch__supporting').text()).toBe('Muted while away');

    const without = await mountView(MdSwitch, { props: baseProps });
    expect(without.find('.md-switch__supporting').exists()).toBe(false);
  });

  it('emits the toggled value from the control', async () => {
    const off = await mountView(MdSwitch, { props: baseProps });
    await off.get('[role="switch"]').trigger('click');
    expect(off.emitted('update:modelValue')).toEqual([[true]]);

    const on = await mountView(MdSwitch, { props: { ...baseProps, modelValue: true } });
    await on.get('[role="switch"]').trigger('click');
    expect(on.emitted('update:modelValue')).toEqual([[false]]);
  });

  // The source comment says the label is clickable, but the label text sits in a sibling div with
  // no `for`/`id` link (the control's generated id is referenced nowhere) and no click handler, so
  // only the track reacts. Pinned as observed.
  it('leaves the switch alone when the label text is clicked', async () => {
    const wrapper = await mountView(MdSwitch, { props: baseProps });

    await wrapper.get('.md-switch__text').trigger('click');

    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
  });

  it('marks itself disabled and emits nothing while disabled', async () => {
    const wrapper = await mountView(MdSwitch, { props: { ...baseProps, disabled: true } });

    expect(wrapper.classes()).toContain('md-switch--disabled');
    expect(wrapper.get('[role="switch"]').attributes('disabled')).toBeDefined();
    expect(wrapper.get('[role="switch"]').attributes('aria-checked')).toBe('false');

    await wrapper.get('[role="switch"]').trigger('click');
    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
  });
});
