// @vitest-environment happy-dom
import { nextTick } from 'vue';
import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import type { VueWrapper } from '@vue/test-utils';

import ImageViewer from './ImageViewer.vue';

const IMAGE = { src: 'file:///pics/cat.png', name: 'cat.png' };

function mountViewer(): VueWrapper {
  return mount(ImageViewer, { props: IMAGE });
}

function imageStyle(wrapper: VueWrapper): string {
  return wrapper.get('.viewer__image').attributes('style') ?? '';
}

function press(key: string): void {
  window.dispatchEvent(new KeyboardEvent('keydown', { key, cancelable: true }));
}

describe('the image viewer', () => {
  it('opens as a native modal dialog showing the picture and its name', () => {
    const wrapper = mountViewer();

    const dialog = wrapper.get<HTMLDialogElement>('dialog').element;
    expect(dialog.open).toBe(true);
    expect(wrapper.get('.viewer__image').attributes('src')).toBe(IMAGE.src);
    expect(wrapper.get('.viewer__image').attributes('alt')).toBe(IMAGE.name);
    expect(wrapper.get('.viewer__name').text()).toBe(IMAGE.name);
    expect(wrapper.get('.viewer__zoom').text()).toBe('100%');
  });

  it('closes from the close button, from Escape and from a click on the empty stage', async () => {
    const button = mountViewer();
    await button.get('button[aria-label="Close the viewer"]').trigger('click');
    expect(button.emitted('close')).toHaveLength(1);

    const escape = mountViewer();
    await escape.get('dialog').trigger('cancel');
    expect(escape.emitted('close')).toHaveLength(1);

    const backdrop = mountViewer();
    backdrop
      .get('.viewer__stage')
      .element.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(backdrop.emitted('close')).toHaveLength(1);
  });

  it('does not close when the picture itself is clicked', () => {
    const wrapper = mountViewer();

    wrapper.get('.viewer__image').element.dispatchEvent(new MouseEvent('click', { bubbles: true }));

    expect(wrapper.emitted('close')).toBeUndefined();
  });

  it('zooms in and out about the stage and reports the level', async () => {
    const wrapper = mountViewer();

    await wrapper.get('button[aria-label="Zoom in"]').trigger('click');
    expect(wrapper.get('.viewer__zoom').text()).toBe('125%');

    await wrapper.get('button[aria-label="Zoom out"]').trigger('click');
    expect(wrapper.get('.viewer__zoom').text()).toBe('100%');
  });

  it('zooms with the wheel and stops the page behind from scrolling', async () => {
    const wrapper = mountViewer();
    const wheel = new WheelEvent('wheel', { deltaY: -120, cancelable: true });

    wrapper.get('.viewer__stage').element.dispatchEvent(wheel);
    await nextTick();

    expect(wheel.defaultPrevented).toBe(true);
    expect(wrapper.get('.viewer__zoom').text()).not.toBe('100%');
  });

  it('turns the picture through quarter turns and back', async () => {
    const wrapper = mountViewer();

    await wrapper.get('button[aria-label="Rotate right"]').trigger('click');
    expect(imageStyle(wrapper)).toContain('rotate(90deg)');

    await wrapper.get('button[aria-label="Rotate right"]').trigger('click');
    expect(imageStyle(wrapper)).toContain('rotate(180deg)');

    await wrapper.get('button[aria-label="Rotate left"]').trigger('click');
    expect(imageStyle(wrapper)).toContain('rotate(90deg)');
  });

  it('fits the picture again, dropping the zoom, the turn and the offset', async () => {
    const wrapper = mountViewer();

    await wrapper.get('button[aria-label="Zoom in"]').trigger('click');
    await wrapper.get('button[aria-label="Rotate right"]').trigger('click');
    await wrapper.get('button[aria-label="Fit the image"]').trigger('click');

    expect(wrapper.get('.viewer__zoom').text()).toBe('100%');
    expect(imageStyle(wrapper)).toContain('rotate(0deg)');
    expect(imageStyle(wrapper)).toContain('translate(0px, 0px)');
  });

  it('zooms to detail on a double click and fits again on the next one', async () => {
    const wrapper = mountViewer();
    const stage = wrapper.get('.viewer__stage').element;

    stage.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    await nextTick();
    expect(wrapper.get('.viewer__zoom').text()).toBe('250%');

    stage.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    await nextTick();
    expect(wrapper.get('.viewer__zoom').text()).toBe('100%');
  });

  it('answers the keyboard for zoom, rotation and fit', async () => {
    const wrapper = mountViewer();

    press('+');
    await nextTick();
    expect(wrapper.get('.viewer__zoom').text()).toBe('125%');

    press('-');
    await nextTick();
    expect(wrapper.get('.viewer__zoom').text()).toBe('100%');

    press('r');
    await nextTick();
    expect(imageStyle(wrapper)).toContain('rotate(90deg)');

    press('R');
    await nextTick();
    expect(imageStyle(wrapper)).toContain('rotate(0deg)');

    press('+');
    press('0');
    await nextTick();
    expect(wrapper.get('.viewer__zoom').text()).toBe('100%');
  });
});
