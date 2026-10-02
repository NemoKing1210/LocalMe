// @vitest-environment happy-dom
import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';

import MdAvatar from './MdAvatar.vue';

describe('MdAvatar', () => {
  it('renders the seed as an SVG data URI in a static image by default', () => {
    const wrapper = mount(MdAvatar, { props: { seed: 'peer-alpha', name: 'Ada' } });

    const src = wrapper.get('img.md-avatar__image').attributes('src');
    expect(src).toMatch(/^data:image\/svg\+xml,/);
  });

  it('is deterministic: the same seed and size produce the same URI', () => {
    const first = mount(MdAvatar, { props: { seed: 'peer-alpha', name: 'Ada', size: 48 } });
    const second = mount(MdAvatar, { props: { seed: 'peer-alpha', name: 'Ada', size: 48 } });

    expect(first.get('img').attributes('src')).toBe(second.get('img').attributes('src'));
  });

  it('gives different seeds different drawings', () => {
    const a = mount(MdAvatar, { props: { seed: 'peer-alpha', name: 'Ada' } });
    const b = mount(MdAvatar, { props: { seed: 'peer-beta', name: 'Bob' } });

    expect(a.get('img').attributes('src')).not.toBe(b.get('img').attributes('src'));
  });

  it('folds the size into the drawing and into the box', () => {
    const small = mount(MdAvatar, { props: { seed: 'peer-alpha', name: 'Ada', size: 40 } });
    const large = mount(MdAvatar, { props: { seed: 'peer-alpha', name: 'Ada', size: 96 } });

    expect(small.get('img').attributes('src')).not.toBe(large.get('img').attributes('src'));
    const smallBox = small.get<HTMLElement>('span.md-avatar').element.style;
    const largeBox = large.get<HTMLElement>('span.md-avatar').element.style;
    expect(smallBox.width).toBe('40px');
    expect(smallBox.height).toBe('40px');
    expect(largeBox.width).toBe('96px');
  });

  it('carries the name as a title and stays hidden from assistive technology', () => {
    const wrapper = mount(MdAvatar, { props: { seed: 'peer-alpha', name: 'Ada Lovelace' } });

    expect(wrapper.attributes('title')).toBe('Ada Lovelace');
    expect(wrapper.attributes('aria-hidden')).toBe('true');
  });

  it('marks a dimmed avatar with a modifier class', () => {
    const wrapper = mount(MdAvatar, { props: { seed: 'peer-alpha', name: 'Ada', dimmed: true } });

    expect(wrapper.classes()).toContain('md-avatar--dimmed');
  });

  it('draws no presence dot when presence is null', () => {
    const wrapper = mount(MdAvatar, { props: { seed: 'peer-alpha', name: 'Ada' } });

    expect(wrapper.find('.md-avatar__presence').exists()).toBe(false);
  });

  it.each(['online', 'offline'] as const)(
    'draws a %s presence dot scaled to the size',
    (presence) => {
      const wrapper = mount(MdAvatar, {
        props: { seed: 'peer-alpha', name: 'Ada', size: 40, presence },
      });

      const dot = wrapper.get<HTMLElement>('.md-avatar__presence').element;
      expect(dot.classList.contains(`md-avatar__presence--${presence}`)).toBe(true);
      // max(8, round(40 / 3.2)) = 13
      expect(dot.style.width).toBe('13px');
      expect(dot.style.height).toBe('13px');
    },
  );

  it('never lets the presence dot shrink below 8px', () => {
    const wrapper = mount(MdAvatar, {
      props: { seed: 'peer-alpha', name: 'Ada', size: 16, presence: 'online' },
    });

    expect(wrapper.get<HTMLElement>('.md-avatar__presence').element.style.width).toBe('8px');
  });

  it.each(['hover', 'always'] as const)(
    'switches to the animated adapter for animate=%s',
    (animate) => {
      const wrapper = mount(MdAvatar, { props: { seed: 'peer-alpha', name: 'Ada', animate } });

      expect(wrapper.find('img').exists()).toBe(false);
      expect(wrapper.get('svg').classes()).toContain('md-avatar__image');
      expect(wrapper.find('g.mo-root').exists()).toBe(true);
    },
  );

  it('marks the adapter always-on only in the always mode', () => {
    const always = mount(MdAvatar, {
      props: { seed: 'peer-alpha', name: 'Ada', animate: 'always' },
    });
    const hover = mount(MdAvatar, { props: { seed: 'peer-alpha', name: 'Ada', animate: 'hover' } });

    expect(always.get('g.mo-root').classes()).toContain('mo-always');
    expect(hover.get('g.mo-root').classes()).not.toContain('mo-always');
  });
});
