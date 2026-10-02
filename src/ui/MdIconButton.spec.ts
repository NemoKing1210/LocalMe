// @vitest-environment happy-dom
import { describe, expect, it, vi } from 'vitest';

import { mountView } from '@/test/mount';
import MdIconButton from './MdIconButton.vue';

describe('MdIconButton', () => {
  it('defaults to a medium standard button labelled for assistive tech', async () => {
    const wrapper = await mountView(MdIconButton, { props: { icon: 'close', label: 'Close' } });

    expect(wrapper.element.tagName).toBe('BUTTON');
    expect(wrapper.attributes('type')).toBe('button');
    expect(wrapper.classes()).toContain('md-state-layer');
    expect(wrapper.classes()).toContain('md-icon-button--standard');
    expect(wrapper.classes()).toContain('md-icon-button--medium');
    expect(wrapper.attributes('aria-label')).toBe('Close');
    expect(wrapper.attributes('title')).toBe('Close');
    expect(wrapper.attributes('aria-pressed')).toBeUndefined();
    expect(wrapper.attributes('disabled')).toBeUndefined();
  });

  it.each(['filled', 'tonal'] as const)('applies the %s variant class', async (variant) => {
    const wrapper = await mountView(MdIconButton, {
      props: { icon: 'close', label: 'Close', variant },
    });

    expect(wrapper.classes()).toContain(`md-icon-button--${variant}`);
  });

  it('marks a selected toggle as pressed', async () => {
    const wrapper = await mountView(MdIconButton, {
      props: { icon: 'check', label: 'Pin', selected: true },
    });

    expect(wrapper.classes()).toContain('md-icon-button--selected');
    expect(wrapper.attributes('aria-pressed')).toBe('true');
  });

  it('shrinks the icon to 18px in the small size', async () => {
    const medium = await mountView(MdIconButton, { props: { icon: 'close', label: 'Close' } });
    const small = await mountView(MdIconButton, {
      props: { icon: 'close', label: 'Close', size: 'small' },
    });

    expect(medium.get('svg').attributes('width')).toBe('22');
    expect(small.classes()).toContain('md-icon-button--small');
    expect(small.get('svg').attributes('width')).toBe('18');
  });

  it('fires a click when enabled but not while disabled', async () => {
    const onClick = vi.fn();
    const enabled = await mountView(MdIconButton, {
      props: { icon: 'close', label: 'Close', onClick },
    });
    await enabled.trigger('click');
    expect(onClick).toHaveBeenCalledTimes(1);

    const disabled = await mountView(MdIconButton, {
      props: { icon: 'close', label: 'Close', disabled: true, onClick },
    });
    expect(disabled.attributes('disabled')).toBeDefined();
    await disabled.trigger('click');
    expect(onClick).toHaveBeenCalledTimes(1);
  });
});
