// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest';
import { defineComponent, h } from 'vue';

import { mountView } from '@/test/mount';
import MdTopAppBar from './MdTopAppBar.vue';

describe('MdTopAppBar', () => {
  it('renders the title and subtitle in a header', async () => {
    const wrapper = await mountView(MdTopAppBar, {
      props: { title: 'Devices', subtitle: '3 online' },
    });

    expect(wrapper.find('header').element.tagName).toBe('HEADER');
    expect(wrapper.get('.md-top-app-bar__title').text()).toBe('Devices');
    expect(wrapper.get('.md-top-app-bar__subtitle').text()).toBe('3 online');
  });

  it('omits the subtitle when none is given', async () => {
    const wrapper = await mountView(MdTopAppBar, { props: { title: 'Devices' } });

    expect(wrapper.find('.md-top-app-bar__subtitle').exists()).toBe(false);
  });

  it('renders no slot wrappers when no slots are filled', async () => {
    const wrapper = await mountView(MdTopAppBar, { props: { title: 'Devices' } });

    expect(wrapper.find('.md-top-app-bar__leading').exists()).toBe(false);
    expect(wrapper.find('.md-top-app-bar__trailing').exists()).toBe(false);
    expect(wrapper.find('.md-top-app-bar__extra').exists()).toBe(false);
  });

  it('places the leading, trailing and default slots in their own rows', async () => {
    const Host = defineComponent({
      render: () =>
        h(
          MdTopAppBar,
          { title: 'Devices', subtitle: '3 online' },
          {
            leading: () => h('span', { class: 'lead' }, 'Back'),
            trailing: () => h('span', { class: 'trail' }, 'Menu'),
            default: () => h('div', { class: 'extra' }, 'Search'),
          },
        ),
    });
    const wrapper = await mountView(Host);

    expect(wrapper.get('.md-top-app-bar__leading').get('.lead').text()).toBe('Back');
    expect(wrapper.get('.md-top-app-bar__trailing').get('.trail').text()).toBe('Menu');
    expect(wrapper.get('.md-top-app-bar__extra').get('.extra').text()).toBe('Search');
    const row = wrapper.get('.md-top-app-bar__row').element;
    expect(row.contains(wrapper.get('.lead').element)).toBe(true);
    expect(row.contains(wrapper.get('.extra').element)).toBe(false);
  });
});
