// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest';
import { defineComponent, h } from 'vue';

import { mountView } from '@/test/mount';
import MdListItem from './MdListItem.vue';

describe('MdListItem', () => {
  it('renders the headline and the supporting line', async () => {
    const wrapper = await mountView(MdListItem, {
      props: { headline: 'Alice', supporting: 'Online' },
    });

    expect(wrapper.get('.md-list-item__headline').text()).toBe('Alice');
    expect(wrapper.get('.md-list-item__supporting').text()).toBe('Online');
  });

  it('omits the supporting line when there is neither text nor a subtitle slot', async () => {
    const wrapper = await mountView(MdListItem, { props: { headline: 'Alice' } });

    expect(wrapper.find('.md-list-item__supporting').exists()).toBe(false);
  });

  it('defaults the supporting tone and applies the chosen one', async () => {
    const byDefault = await mountView(MdListItem, { props: { headline: 'Alice' } });
    expect(byDefault.classes()).toContain('md-list-item--default');
    expect(byDefault.classes()).toContain('md-state-layer');

    for (const supportingTone of ['muted', 'error'] as const) {
      const wrapper = await mountView(MdListItem, { props: { headline: 'Alice', supportingTone } });
      expect(wrapper.classes()).toContain(`md-list-item--${supportingTone}`);
    }
  });

  it('marks the selected row both visually and via aria-current', async () => {
    const selected = await mountView(MdListItem, { props: { headline: 'Alice', selected: true } });
    expect(selected.classes()).toContain('md-list-item--selected');
    expect(selected.get('.md-list-item__row').attributes('aria-current')).toBe('true');

    const plain = await mountView(MdListItem, { props: { headline: 'Alice' } });
    expect(plain.classes()).not.toContain('md-list-item--selected');
    expect(plain.get('.md-list-item__row').attributes('aria-current')).toBeUndefined();
  });

  it('adds the dense class only when dense', async () => {
    const dense = await mountView(MdListItem, { props: { headline: 'Alice', dense: true } });
    expect(dense.classes()).toContain('md-list-item--dense');

    const plain = await mountView(MdListItem, { props: { headline: 'Alice' } });
    expect(plain.classes()).not.toContain('md-list-item--dense');
  });

  it('emits activate when the row is clicked', async () => {
    const wrapper = await mountView(MdListItem, { props: { headline: 'Alice' } });

    await wrapper.get('.md-list-item__row').trigger('click');

    expect(wrapper.emitted('activate')).toHaveLength(1);
  });

  it('puts the trailing content outside the row button', async () => {
    const Host = defineComponent({
      render: () =>
        h(
          MdListItem,
          { headline: 'Alice' },
          {
            trailing: () => h('button', { class: 'trail', type: 'button' }, 'Remove'),
          },
        ),
    });
    const wrapper = await mountView(Host);

    const trailing = wrapper.get('.md-list-item__trailing');
    expect(trailing.classes()).toContain('md-list-item__trailing');
    expect(wrapper.get('.md-list-item__row').element.contains(trailing.element)).toBe(false);

    await wrapper.get('.trail').trigger('click');
    expect(wrapper.emitted('activate')).toBeUndefined();
  });

  it('renders the leading slot beside the text', async () => {
    const Host = defineComponent({
      render: () =>
        h(
          MdListItem,
          { headline: 'Alice' },
          {
            leading: () => h('span', { class: 'lead' }, 'A'),
          },
        ),
    });
    const wrapper = await mountView(Host);

    expect(wrapper.get('.md-list-item__leading').get('.lead').text()).toBe('A');
  });

  it('prefers the subtitle slot over the supporting prop', async () => {
    const Host = defineComponent({
      render: () =>
        h(
          MdListItem,
          { headline: 'Alice', supporting: 'from prop' },
          {
            subtitle: () => 'from slot',
          },
        ),
    });
    const wrapper = await mountView(Host);

    expect(wrapper.get('.md-list-item__supporting').text()).toBe('from slot');
  });
});
