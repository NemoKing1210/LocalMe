// @vitest-environment happy-dom
import { mount } from '@vue/test-utils';
import { nextTick } from 'vue';
import { describe, expect, it } from 'vitest';

import MdBadge from './MdBadge.vue';

describe('MdBadge', () => {
  it('renders nothing without a value', () => {
    const wrapper = mount(MdBadge);

    expect(wrapper.find('.md-badge').exists()).toBe(false);
  });

  it('renders nothing for zero or a negative count', () => {
    for (const value of [0, -1, -100]) {
      const wrapper = mount(MdBadge, { props: { value } });

      expect(wrapper.find('.md-badge').exists(), `value=${value}`).toBe(false);
    }
  });

  it('shows the exact count up to max', () => {
    const wrapper = mount(MdBadge, { props: { value: 7 } });

    expect(wrapper.get('.md-badge').text()).toBe('7');
  });

  it('shows max in full and max+ above it', () => {
    const atMax = mount(MdBadge, { props: { value: 99, max: 99 } });
    const over = mount(MdBadge, { props: { value: 100, max: 99 } });

    expect(atMax.get('.md-badge').text()).toBe('99');
    expect(over.get('.md-badge').text()).toBe('99+');
  });

  it('honours a custom max', () => {
    const atMax = mount(MdBadge, { props: { value: 3, max: 3 } });
    const over = mount(MdBadge, { props: { value: 4, max: 3 } });

    expect(atMax.get('.md-badge').text()).toBe('3');
    expect(over.get('.md-badge').text()).toBe('3+');
  });

  it('dresses the count as a label-scale badge', () => {
    const wrapper = mount(MdBadge, { props: { value: 5 } });

    expect(wrapper.get('.md-badge').classes()).toContain('md-typescale-label-small');
  });

  it('removes the badge when the value drops to zero', async () => {
    const wrapper = mount(MdBadge, { props: { value: 5 } });
    expect(wrapper.find('.md-badge').exists()).toBe(true);

    await wrapper.setProps({ value: 0 });
    await nextTick();

    expect(wrapper.find('.md-badge').exists()).toBe(false);
  });
});
