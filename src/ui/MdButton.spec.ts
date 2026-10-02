// @vitest-environment happy-dom
import { describe, expect, it, vi } from 'vitest';
import { defineComponent, h } from 'vue';

import { mountView } from '@/test/mount';
import MdButton from './MdButton.vue';
import MdIcon from './MdIcon.vue';

describe('MdButton', () => {
  it('renders a filled button by default', async () => {
    const wrapper = await mountView(MdButton);

    expect(wrapper.find('button').element.tagName).toBe('BUTTON');
    expect(wrapper.attributes('type')).toBe('button');
    expect(wrapper.classes()).toContain('md-button');
    expect(wrapper.classes()).toContain('md-typescale-label-large');
    expect(wrapper.classes()).toContain('md-button--filled');
    expect(wrapper.attributes('disabled')).toBeUndefined();
  });

  it.each(['filled', 'tonal', 'outlined', 'text', 'elevated'] as const)(
    'applies the %s variant class',
    async (variant) => {
      const wrapper = await mountView(MdButton, { props: { variant } });

      expect(wrapper.classes()).toContain(`md-button--${variant}`);
    },
  );

  it('renders the default slot as the label', async () => {
    const Host = defineComponent({
      render: () => h(MdButton, null, { default: () => 'Save' }),
    });

    const wrapper = await mountView(Host);

    expect(wrapper.get('.md-button__label').text()).toBe('Save');
  });

  it.each(['submit', 'reset'] as const)('honours type=%s', async (type) => {
    const wrapper = await mountView(MdButton, { props: { type } });

    expect(wrapper.attributes('type')).toBe(type);
  });

  it('renders the leading icon only when an icon is set', async () => {
    const plain = await mountView(MdButton);
    const withIcon = await mountView(MdButton, { props: { icon: 'send' } });

    expect(plain.findComponent(MdIcon).exists()).toBe(false);
    expect(withIcon.findComponent(MdIcon).exists()).toBe(true);
  });

  it('disables itself and swaps the icon for a spinner while busy', async () => {
    const wrapper = await mountView(MdButton, { props: { icon: 'send', busy: true } });

    expect(wrapper.attributes('disabled')).toBeDefined();
    expect(wrapper.attributes('aria-busy')).toBe('true');
    expect(wrapper.find('.md-button__spinner').exists()).toBe(true);
    expect(wrapper.findComponent(MdIcon).exists()).toBe(false);
  });

  it('reports aria-busy=false when idle', async () => {
    const wrapper = await mountView(MdButton);

    expect(wrapper.attributes('aria-busy')).toBe('false');
  });

  it('fires a click when enabled', async () => {
    const onClick = vi.fn();
    const wrapper = await mountView(MdButton, { props: { onClick } });

    await wrapper.trigger('click');

    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it('does not fire a click while disabled', async () => {
    const onClick = vi.fn();
    const wrapper = await mountView(MdButton, { props: { disabled: true, onClick } });

    expect(wrapper.attributes('disabled')).toBeDefined();
    await wrapper.trigger('click');

    expect(onClick).not.toHaveBeenCalled();
  });
});
