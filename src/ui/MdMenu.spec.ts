// @vitest-environment happy-dom
import { mount } from '@vue/test-utils';
import { nextTick } from 'vue';
import { describe, expect, it, vi } from 'vitest';

import MdMenu from './MdMenu.vue';

const LABEL = 'Message actions';

const ITEMS = [
  { id: 'copy', label: 'Copy', icon: 'copy' },
  { id: 'delete', label: 'Delete', danger: true },
] as const;

interface MenuExposed {
  show: (at?: { readonly x: number; readonly y: number }) => void;
}

/** The surface is measured and focused one frame after the first render, so an open needs two. */
async function settleMenu(): Promise<void> {
  await nextTick();
  await nextTick();
}

describe('MdMenu', () => {
  it('shows a closed trigger and no items before it is opened', () => {
    const wrapper = mount(MdMenu, { props: { items: ITEMS, label: LABEL } });

    const trigger = wrapper.get('.md-menu__trigger');
    expect(trigger.attributes('aria-haspopup')).toBe('menu');
    expect(trigger.attributes('aria-expanded')).toBe('false');
    expect(trigger.attributes('aria-label')).toBe(LABEL);
    expect(wrapper.find('.md-menu__surface').exists()).toBe(false);
    expect(wrapper.findAll('[role="menuitem"]')).toHaveLength(0);
  });

  it('opens on the trigger with the items in order and their modifiers', async () => {
    const wrapper = mount(MdMenu, { props: { items: ITEMS, label: LABEL } });

    await wrapper.get('.md-menu__trigger').trigger('click');
    await settleMenu();

    const surface = wrapper.get('.md-menu__surface');
    expect(surface.attributes('role')).toBe('menu');
    expect(surface.attributes('aria-label')).toBe(LABEL);
    expect(wrapper.get('.md-menu__trigger').attributes('aria-expanded')).toBe('true');

    const items = wrapper.findAll('[role="menuitem"]');
    expect(items.map((item) => item.text())).toEqual(['Copy', 'Delete']);
    expect(items[0]?.find('svg').exists()).toBe(true);
    expect(items[0]?.classes()).not.toContain('md-menu__item--danger');
    expect(items[1]?.classes()).toContain('md-menu__item--danger');
    expect(items[1]?.find('svg').exists()).toBe(false);
  });

  it('toggles closed when the trigger is pressed again', async () => {
    const wrapper = mount(MdMenu, { props: { items: ITEMS, label: LABEL } });
    await wrapper.get('.md-menu__trigger').trigger('click');
    await settleMenu();

    await wrapper.get('.md-menu__trigger').trigger('click');
    await settleMenu();

    expect(wrapper.find('.md-menu__surface').exists()).toBe(false);
    expect(wrapper.get('.md-menu__trigger').attributes('aria-expanded')).toBe('false');
  });

  it('emits the chosen id, closes and restores focus to the trigger', async () => {
    const wrapper = mount(MdMenu, {
      props: { items: ITEMS, label: LABEL },
      attachTo: document.body,
    });
    await wrapper.get('.md-menu__trigger').trigger('click');
    await settleMenu();

    await wrapper.findAll('[role="menuitem"]')[1]?.trigger('click');

    expect(wrapper.emitted('select')).toEqual([['delete']]);
    expect(wrapper.find('.md-menu__surface').exists()).toBe(false);
    expect(document.activeElement).toBe(wrapper.get('.md-menu__trigger').element);
  });

  it('closes on an outside pointerdown but not on one inside the menu', async () => {
    const wrapper = mount(MdMenu, { props: { items: ITEMS, label: LABEL } });
    await wrapper.get('.md-menu__trigger').trigger('click');
    await settleMenu();

    wrapper
      .findAll('[role="menuitem"]')[0]
      ?.element.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    expect(wrapper.find('.md-menu__surface').exists()).toBe(true);

    document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    await nextTick();
    expect(wrapper.find('.md-menu__surface').exists()).toBe(false);
  });

  it('closes on Escape, preventing the default, and restores focus', async () => {
    const wrapper = mount(MdMenu, {
      props: { items: ITEMS, label: LABEL },
      attachTo: document.body,
    });
    await wrapper.get('.md-menu__trigger').trigger('click');
    await settleMenu();

    const escape = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
    wrapper.get('.md-menu').element.dispatchEvent(escape);
    await nextTick();

    expect(escape.defaultPrevented).toBe(true);
    expect(wrapper.find('.md-menu__surface').exists()).toBe(false);
    expect(document.activeElement).toBe(wrapper.get('.md-menu__trigger').element);
  });

  it('moves the active item with the arrow, Home and End keys, wrapping around', async () => {
    const wrapper = mount(MdMenu, {
      props: { items: ITEMS, label: LABEL },
      attachTo: document.body,
    });
    await wrapper.get('.md-menu__trigger').trigger('click');
    await settleMenu();
    const items = wrapper.findAll<HTMLElement>('[role="menuitem"]');

    expect(document.activeElement).toBe(items[0]?.element);

    await wrapper.get('.md-menu').trigger('keydown', { key: 'ArrowDown' });
    expect(document.activeElement).toBe(items[1]?.element);

    await wrapper.get('.md-menu').trigger('keydown', { key: 'ArrowDown' });
    expect(document.activeElement).toBe(items[0]?.element);

    await wrapper.get('.md-menu').trigger('keydown', { key: 'ArrowUp' });
    expect(document.activeElement).toBe(items[1]?.element);

    await wrapper.get('.md-menu').trigger('keydown', { key: 'Home' });
    expect(document.activeElement).toBe(items[0]?.element);

    await wrapper.get('.md-menu').trigger('keydown', { key: 'End' });
    expect(document.activeElement).toBe(items[1]?.element);
  });

  it('ignores keys while closed', async () => {
    const wrapper = mount(MdMenu, { props: { items: ITEMS, label: LABEL } });

    await wrapper.get('.md-menu').trigger('keydown', { key: 'ArrowDown' });
    await wrapper.get('.md-menu').trigger('keydown', { key: 'Escape' });

    expect(wrapper.find('.md-menu__surface').exists()).toBe(false);
    expect(wrapper.emitted('select')).toBeUndefined();
  });

  it('closes when focus leaves the menu', async () => {
    const wrapper = mount(MdMenu, {
      props: { items: ITEMS, label: LABEL },
      attachTo: document.body,
    });
    await wrapper.get('.md-menu__trigger').trigger('click');
    await settleMenu();

    wrapper
      .get('.md-menu')
      .element.dispatchEvent(new FocusEvent('focusout', { relatedTarget: null }));
    await nextTick();

    expect(wrapper.find('.md-menu__surface').exists()).toBe(false);
  });

  it('closes when the surface would no longer be in place: scroll and resize', async () => {
    const wrapper = mount(MdMenu, { props: { items: ITEMS, label: LABEL } });
    await wrapper.get('.md-menu__trigger').trigger('click');
    await settleMenu();

    document.dispatchEvent(new Event('scroll', { bubbles: true }));
    await nextTick();
    expect(wrapper.find('.md-menu__surface').exists()).toBe(false);

    await wrapper.get('.md-menu__trigger').trigger('click');
    await settleMenu();
    window.dispatchEvent(new Event('resize'));
    await nextTick();
    expect(wrapper.find('.md-menu__surface').exists()).toBe(false);
  });

  it('attaches the outside-press guards only while open and removes them on unmount', async () => {
    const documentAdd = vi.spyOn(document, 'addEventListener');
    const documentRemove = vi.spyOn(document, 'removeEventListener');
    const windowRemove = vi.spyOn(window, 'removeEventListener');
    const wrapper = mount(MdMenu, { props: { items: ITEMS, label: LABEL } });

    await wrapper.get('.md-menu__trigger').trigger('click');
    await settleMenu();
    const capturedPointerDown = documentAdd.mock.calls.some(
      ([type, , capture]) => type === 'pointerdown' && capture === true,
    );
    expect(capturedPointerDown).toBe(true);

    wrapper.unmount();
    expect(documentRemove.mock.calls.some(([type]) => type === 'pointerdown')).toBe(true);
    expect(documentRemove.mock.calls.some(([type]) => type === 'scroll')).toBe(true);
    expect(windowRemove.mock.calls.some(([type]) => type === 'resize')).toBe(true);
  });

  it('opens a bare context menu at the requested point, with no trigger', async () => {
    const wrapper = mount(MdMenu, {
      props: { items: ITEMS, label: LABEL, trigger: false },
      attachTo: document.body,
    });

    expect(wrapper.classes()).toContain('md-menu--bare');
    expect(wrapper.find('.md-menu__trigger').exists()).toBe(false);

    (wrapper.vm as unknown as MenuExposed).show({ x: 120, y: 80 });
    await settleMenu();

    const surface = wrapper.get<HTMLElement>('.md-menu__surface').element;
    expect(surface.style.left).toBe('120px');
    expect(surface.style.top).toBe('80px');
    expect(wrapper.findAll('[role="menuitem"]')).toHaveLength(2);
    expect(document.activeElement).toBe(wrapper.findAll('[role="menuitem"]')[0]?.element);
  });

  it('does not open a context menu that has no items', async () => {
    const wrapper = mount(MdMenu, {
      props: { items: [], label: LABEL, trigger: false },
      attachTo: document.body,
    });

    (wrapper.vm as unknown as MenuExposed).show({ x: 10, y: 10 });
    await settleMenu();

    expect(wrapper.find('.md-menu__surface').exists()).toBe(false);
  });
});
