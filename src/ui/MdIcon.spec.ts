// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest';

import { mountView } from '@/test/mount';
import MdIcon from './MdIcon.vue';
import type { IconName } from './icons';

const UNKNOWN_ICON = 'not-an-icon' as unknown as IconName;

describe('MdIcon', () => {
  it('renders a 24px frame with the default stroke', async () => {
    const wrapper = await mountView(MdIcon, { props: { name: 'check' } });

    expect(wrapper.find('svg').element.tagName).toBe('svg');
    expect(wrapper.attributes('viewBox')).toBe('0 0 24 24');
    expect(wrapper.attributes('width')).toBe('24');
    expect(wrapper.attributes('height')).toBe('24');
    expect(wrapper.attributes('fill')).toBe('none');
    expect(wrapper.attributes('stroke')).toBe('currentColor');
    expect(wrapper.attributes('stroke-width')).toBe('2');
    expect(wrapper.attributes('stroke-linecap')).toBe('round');
    expect(wrapper.attributes('stroke-linejoin')).toBe('round');
    expect(wrapper.attributes('aria-hidden')).toBe('true');
    expect(wrapper.attributes('focusable')).toBe('false');
  });

  it('honours size and strokeWidth', async () => {
    const wrapper = await mountView(MdIcon, {
      props: { name: 'check', size: 48, strokeWidth: 1.5 },
    });

    expect(wrapper.attributes('width')).toBe('48');
    expect(wrapper.attributes('height')).toBe('48');
    expect(wrapper.attributes('stroke-width')).toBe('1.5');
  });

  it('renders a circle and a line for a magnifier', async () => {
    const wrapper = await mountView(MdIcon, { props: { name: 'search' } });

    expect(wrapper.findAll('circle')).toHaveLength(1);
    expect(wrapper.get('circle').attributes('cx')).toBe('11');
    expect(wrapper.get('circle').attributes('r')).toBe('6.6');
    expect(wrapper.get('line').attributes('x2')).toBe('20.5');
  });

  it('renders a polyline for an arrow', async () => {
    const wrapper = await mountView(MdIcon, { props: { name: 'back' } });

    expect(wrapper.get('polyline').attributes('points')).toBe('11.5,19 4.5,12 11.5,5');
    expect(wrapper.findAll('line')).toHaveLength(1);
  });

  it('renders one path per curve', async () => {
    const wrapper = await mountView(MdIcon, { props: { name: 'send' } });

    expect(wrapper.findAll('path')).toHaveLength(2);
  });

  it('renders a rect primitive', async () => {
    const wrapper = await mountView(MdIcon, { props: { name: 'copy' } });

    const rect = wrapper.get('rect');
    expect(rect.attributes('x')).toBe('8.8');
    expect(rect.attributes('width')).toBe('12.4');
    expect(rect.attributes('rx')).toBe('2.6');
  });

  it('fills dot primitives instead of stroking them', async () => {
    const wrapper = await mountView(MdIcon, { props: { name: 'more-vertical' } });

    const dots = wrapper.findAll('circle');
    expect(dots).toHaveLength(3);
    for (const dot of dots) {
      expect(dot.attributes('fill')).toBe('currentColor');
      expect(dot.attributes('stroke')).toBe('none');
    }
  });

  it('renders nothing for a name that is not in the icon set', async () => {
    const wrapper = await mountView(MdIcon, { props: { name: UNKNOWN_ICON } });

    expect(wrapper.find('svg').element.children).toHaveLength(0);
  });
});
