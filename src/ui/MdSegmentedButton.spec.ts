// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest';
import { nextTick } from 'vue';

import { mountView } from '@/test/mount';
import MdSegmentedButton from './MdSegmentedButton.vue';

const options = [
  { value: 'a', label: 'Alpha' },
  { value: 'b', label: 'Beta', icon: 'people' as const },
  { value: 'c', label: 'Gamma' },
];

describe('MdSegmentedButton', () => {
  it('renders one radio per option with a roving tabindex', async () => {
    const wrapper = await mountView(MdSegmentedButton, { props: { modelValue: 'b', options } });

    expect(wrapper.get('[role="radiogroup"]').classes()).toContain('md-segmented');
    const segments = wrapper.findAll('[role="radio"]');
    expect(segments.map((segment) => segment.text())).toEqual(['Alpha', 'Beta', 'Gamma']);
    expect(segments.map((segment) => segment.attributes('data-value'))).toEqual(['a', 'b', 'c']);
    expect(segments.map((segment) => segment.attributes('aria-checked'))).toEqual([
      'false',
      'true',
      'false',
    ]);
    expect(segments.map((segment) => segment.attributes('tabindex'))).toEqual(['-1', '0', '-1']);
    expect(wrapper.get('[data-value="b"]').classes()).toContain('md-segmented__segment--selected');
  });

  it('renders an option icon when the option declares one', async () => {
    const wrapper = await mountView(MdSegmentedButton, { props: { modelValue: 'a', options } });

    expect(wrapper.findAll('svg')).toHaveLength(1);
    expect(wrapper.get('[data-value="b"] svg').attributes('width')).toBe('18');
  });

  it('emits the value of a clicked segment, but not of the selected one', async () => {
    const wrapper = await mountView(MdSegmentedButton, { props: { modelValue: 'a', options } });

    await wrapper.get('[data-value="c"]').trigger('click');
    expect(wrapper.emitted('update:modelValue')).toEqual([['c']]);

    await wrapper.get('[data-value="a"]').trigger('click');
    expect(wrapper.emitted('update:modelValue')).toEqual([['c']]);
  });

  it.each([
    ['ArrowRight', 'c'],
    ['ArrowDown', 'c'],
    ['ArrowLeft', 'a'],
    ['ArrowUp', 'a'],
    ['Home', 'a'],
    ['End', 'c'],
  ] as const)('moves with %s from b to %s', async (key, expected) => {
    const wrapper = await mountView(MdSegmentedButton, { props: { modelValue: 'b', options } });

    await wrapper.get('.md-segmented').trigger('keydown', { key });

    expect(wrapper.emitted('update:modelValue')).toEqual([[expected]]);
  });

  it('wraps around at either end', async () => {
    const last = await mountView(MdSegmentedButton, { props: { modelValue: 'c', options } });
    await last.get('.md-segmented').trigger('keydown', { key: 'ArrowRight' });
    expect(last.emitted('update:modelValue')).toEqual([['a']]);

    const first = await mountView(MdSegmentedButton, { props: { modelValue: 'a', options } });
    await first.get('.md-segmented').trigger('keydown', { key: 'ArrowLeft' });
    expect(first.emitted('update:modelValue')).toEqual([['c']]);
  });

  it('prevents the default for a handled key and ignores the others', async () => {
    const wrapper = await mountView(MdSegmentedButton, {
      props: { modelValue: 'b', options },
      attachTo: document.body,
    });
    const root = wrapper.get('.md-segmented').element;

    const handled = new KeyboardEvent('keydown', { key: 'ArrowRight', cancelable: true });
    root.dispatchEvent(handled);
    await nextTick();
    expect(handled.defaultPrevented).toBe(true);

    const ignored = new KeyboardEvent('keydown', { key: 'Enter', cancelable: true });
    root.dispatchEvent(ignored);
    await nextTick();
    expect(ignored.defaultPrevented).toBe(false);
    expect(wrapper.emitted('update:modelValue')).toEqual([['c']]);
  });

  it('moves focus to the newly selected segment', async () => {
    const wrapper = await mountView(MdSegmentedButton, {
      props: { modelValue: 'b', options },
      attachTo: document.body,
    });

    await wrapper.get('.md-segmented').trigger('keydown', { key: 'ArrowRight' });

    expect(document.activeElement).toBe(wrapper.get('[data-value="c"]').element);
  });

  it('ignores keyboard input when the model is not one of the options', async () => {
    const wrapper = await mountView(MdSegmentedButton, {
      props: { modelValue: 'missing', options },
    });

    await wrapper.get('.md-segmented').trigger('keydown', { key: 'ArrowRight' });

    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
  });
});
