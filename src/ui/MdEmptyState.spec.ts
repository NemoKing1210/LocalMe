// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest';
import { defineComponent, h } from 'vue';

import { mountView } from '@/test/mount';
import MdEmptyState from './MdEmptyState.vue';

describe('MdEmptyState', () => {
  it('renders the title and the body', async () => {
    const wrapper = await mountView(MdEmptyState, {
      props: { title: 'No devices', body: 'Pair one to start' },
    });

    expect(wrapper.classes()).toContain('md-empty-state');
    expect(wrapper.get('.md-empty-state__title').text()).toBe('No devices');
    expect(wrapper.get('.md-empty-state__body').text()).toBe('Pair one to start');
  });

  it('omits the body when none is given', async () => {
    const wrapper = await mountView(MdEmptyState, { props: { title: 'No devices' } });

    expect(wrapper.find('.md-empty-state__body').exists()).toBe(false);
  });

  it('draws a 48px icon only when one is named', async () => {
    const withIcon = await mountView(MdEmptyState, {
      props: { title: 'No devices', icon: 'people' },
    });
    const icon = withIcon.get('.md-empty-state__icon');
    expect(icon.element.tagName).toBe('svg');
    expect(icon.attributes('width')).toBe('48');

    const without = await mountView(MdEmptyState, { props: { title: 'No devices' } });
    expect(without.find('.md-empty-state__icon').exists()).toBe(false);
  });

  it('wraps the default slot in the actions row only when the slot is filled', async () => {
    const Host = defineComponent({
      render: () =>
        h(MdEmptyState, { title: 'No devices' }, { default: () => h('button', {}, 'Add') }),
    });
    const withActions = await mountView(Host);
    expect(withActions.get('.md-empty-state__actions').get('button').text()).toBe('Add');

    const without = await mountView(MdEmptyState, { props: { title: 'No devices' } });
    expect(without.find('.md-empty-state__actions').exists()).toBe(false);
  });
});
