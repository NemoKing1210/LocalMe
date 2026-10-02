// @vitest-environment happy-dom
import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';

import MdIcon from './MdIcon.vue';
import { ICONS } from './icons';

const TAG_FOR_KIND: Record<string, string> = {
  path: 'path',
  line: 'line',
  polyline: 'polyline',
  circle: 'circle',
  rect: 'rect',
  dot: 'circle',
};

describe('icon data', () => {
  it('defines at least one icon and every icon has at least one primitive', () => {
    const entries = Object.entries(ICONS);

    expect(entries.length).toBeGreaterThan(0);
    for (const [name, shapes] of entries) {
      expect(shapes.length, name).toBeGreaterThan(0);
    }
  });

  it('uses only the primitives the renderer knows how to draw', () => {
    for (const [name, shapes] of Object.entries(ICONS)) {
      for (const shape of shapes) {
        expect(Object.keys(TAG_FOR_KIND), `${name}:${shape.kind}`).toContain(shape.kind);
      }
    }
  });

  it('gives every primitive the fields its kind needs', () => {
    for (const [name, shapes] of Object.entries(ICONS)) {
      for (const shape of shapes) {
        switch (shape.kind) {
          case 'path':
            expect(shape.d.length, name).toBeGreaterThan(0);
            break;
          case 'line':
            for (const value of [shape.x1, shape.y1, shape.x2, shape.y2]) {
              expect(Number.isFinite(value), name).toBe(true);
            }
            break;
          case 'polyline':
            expect(shape.points.length, name).toBeGreaterThan(0);
            break;
          case 'circle':
          case 'dot':
            for (const value of [shape.cx, shape.cy, shape.r]) {
              expect(Number.isFinite(value), name).toBe(true);
            }
            break;
          case 'rect':
            for (const value of [shape.x, shape.y, shape.width, shape.height, shape.rx]) {
              expect(Number.isFinite(value), name).toBe(true);
            }
            break;
        }
      }
    }
  });

  it('expands the generated loop icons to the expected number of primitives', () => {
    // `sun` is one disc plus eight rays; `tune` is three tracks with three handles.
    expect(ICONS['sun']).toHaveLength(9);
    expect(ICONS['tune']).toHaveLength(6);
    const bell = ICONS['bell'] ?? [];
    expect(ICONS['bell-off']).toHaveLength(bell.length + 1);
  });
});

describe('MdIcon', () => {
  it('renders every icon as a 24px SVG with one element per primitive', () => {
    for (const name of Object.keys(ICONS)) {
      const wrapper = mount(MdIcon, { props: { name } });
      const svg = wrapper.get('svg');
      const shapes = ICONS[name] ?? [];

      expect(svg.attributes('viewBox'), name).toBe('0 0 24 24');
      expect(svg.attributes('aria-hidden'), name).toBe('true');
      expect(svg.attributes('fill'), name).toBe('none');

      const children = Array.from(svg.element.children);
      expect(children.length, name).toBe(shapes.length);
      children.forEach((child, index) => {
        const shape = shapes[index];
        expect(shape, `${name}[${index}]`).toBeDefined();
        if (shape === undefined) return;
        expect(child.tagName.toLowerCase(), `${name}[${index}]`).toBe(TAG_FOR_KIND[shape.kind]);
      });

      wrapper.unmount();
    }
  });

  it('draws a dot as a filled circle while a plain circle stays an outline', () => {
    const wrapper = mount(MdIcon, { props: { name: 'more-vertical' } });

    const circles = wrapper.findAll('circle');
    expect(circles).toHaveLength(3);
    for (const circle of circles) {
      expect(circle.attributes('fill')).toBe('currentColor');
      expect(circle.attributes('stroke')).toBe('none');
    }
  });

  it('draws an outline circle as a stroke on the SVG', () => {
    const wrapper = mount(MdIcon, { props: { name: 'search' } });

    const circle = wrapper.get('circle');
    expect(circle.attributes('fill')).toBeUndefined();
    expect(circle.attributes('stroke')).toBeUndefined();
  });

  it('sizes the SVG from its props', () => {
    const wrapper = mount(MdIcon, { props: { name: 'search', size: 18, strokeWidth: 1.5 } });

    const svg = wrapper.get('svg');
    expect(svg.attributes('width')).toBe('18');
    expect(svg.attributes('height')).toBe('18');
    expect(svg.attributes('stroke-width')).toBe('1.5');
  });
});
