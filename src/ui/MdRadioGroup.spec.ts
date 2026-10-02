// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest';

import { mountView } from '@/test/mount';
import MdRadioGroup from './MdRadioGroup.vue';

const options = [
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark', description: 'Easier at night' },
] as const;

describe('MdRadioGroup', () => {
  it('marks the group and labels it for assistive tech', async () => {
    const wrapper = await mountView(MdRadioGroup, {
      props: { modelValue: 'light', label: 'Theme', options },
    });

    expect(wrapper.get('[role="radiogroup"]').attributes('aria-label')).toBe('Theme');
  });

  it('renders every option label, and a description only when given', async () => {
    const wrapper = await mountView(MdRadioGroup, {
      props: { modelValue: 'light', label: 'Theme', options },
    });

    const labels = wrapper.findAll('.md-typescale-body-large').map((node) => node.text());
    expect(labels).toEqual(['Light', 'Dark']);
    expect(wrapper.findAll('.md-radio-group__description')).toHaveLength(1);
    expect(wrapper.get('.md-radio-group__description').text()).toBe('Easier at night');
  });

  it('uses real radio inputs that share one generated name', async () => {
    const wrapper = await mountView(MdRadioGroup, {
      props: { modelValue: 'light', label: 'Theme', options },
    });

    const inputs = wrapper.findAll('input');
    expect(inputs).toHaveLength(2);
    expect(inputs.map((input) => input.attributes('type'))).toEqual(['radio', 'radio']);
    expect(inputs.map((input) => input.attributes('value'))).toEqual(['light', 'dark']);
    const names = inputs.map((input) => input.attributes('name'));
    expect(names[0]).toBeTruthy();
    expect(names[1]).toBe(names[0]);
  });

  it('checks only the option matching the model and marks its indicator', async () => {
    const wrapper = await mountView(MdRadioGroup, {
      props: { modelValue: 'dark', label: 'Theme', options },
    });

    const inputs = wrapper.findAll('input');
    const first = inputs[0];
    const second = inputs[1];
    if (first === undefined || second === undefined) throw new Error('expected two radio inputs');

    expect(first.attributes('checked')).toBeUndefined();
    expect(second.attributes('checked')).toBeDefined();
    expect(second.element.checked).toBe(true);
    expect(
      wrapper.get('.md-radio-group__option:nth-child(2) .md-radio-group__indicator').classes(),
    ).toContain('md-radio-group__indicator--selected');
    expect(
      wrapper.get('.md-radio-group__option:nth-child(1) .md-radio-group__indicator').classes(),
    ).not.toContain('md-radio-group__indicator--selected');
  });

  it('emits the chosen value when another radio is selected', async () => {
    const wrapper = await mountView(MdRadioGroup, {
      props: { modelValue: 'light', label: 'Theme', options },
    });

    await wrapper.get('.md-radio-group__option:nth-child(2) input').setValue();

    expect(wrapper.emitted('update:modelValue')).toEqual([['dark']]);
  });

  it('does not emit when the already-selected radio is re-checked', async () => {
    const wrapper = await mountView(MdRadioGroup, {
      props: { modelValue: 'light', label: 'Theme', options },
    });

    await wrapper.get('.md-radio-group__option:nth-child(1) input').setValue();

    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
  });
});
