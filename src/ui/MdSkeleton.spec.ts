// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest';

import { mountView } from '@/test/mount';
import MdSkeleton from './MdSkeleton.vue';

describe('MdSkeleton', () => {
  it('is a text-shaped block that fills its line by default', async () => {
    const wrapper = await mountView(MdSkeleton);

    expect(wrapper.find('span').element.tagName).toBe('SPAN');
    expect(wrapper.classes()).toEqual(['md-skeleton', 'md-skeleton--text']);
    expect(wrapper.attributes('aria-hidden')).toBe('true');
    expect(wrapper.find('span').element.style.width).toBe('100%');
    expect(wrapper.find('span').element.style.height).toBe('1em');
  });

  it.each(['bubble', 'pill', 'text'] as const)('applies the %s shape class', async (shape) => {
    const wrapper = await mountView(MdSkeleton, { props: { shape } });

    expect(wrapper.classes()).toContain(`md-skeleton--${shape}`);
  });

  it('takes an explicit width and height', async () => {
    const wrapper = await mountView(MdSkeleton, { props: { width: '64px', height: '2rem' } });

    expect(wrapper.find('span').element.style.width).toBe('64px');
    expect(wrapper.find('span').element.style.height).toBe('2rem');
    expect(wrapper.classes()).toContain('md-skeleton--text');
  });
});
