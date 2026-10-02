// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest';

import { mountView } from '@/test/mount';
import MdTextField from './MdTextField.vue';

describe('MdTextField', () => {
  it('renders the label and the value in a text input', async () => {
    const wrapper = await mountView(MdTextField, { props: { modelValue: 'hi', label: 'Name' } });

    expect(wrapper.get('.md-field__label').text()).toBe('Name');
    expect(wrapper.get('input').attributes('type')).toBe('text');
    expect(wrapper.get('input').element.value).toBe('hi');
    expect(wrapper.classes()).toContain('md-field');
  });

  it('emits update:modelValue on input', async () => {
    const wrapper = await mountView(MdTextField, { props: { modelValue: '', label: 'Name' } });

    await wrapper.get('input').setValue('bye');

    expect(wrapper.emitted('update:modelValue')).toEqual([['bye']]);
  });

  it('shows the placeholder, and floats the label, only once the field is focused or filled', async () => {
    const empty = await mountView(MdTextField, {
      props: { modelValue: '', label: 'Name', placeholder: 'Type here' },
    });

    expect(empty.get('input').attributes('placeholder')).toBeUndefined();
    expect(empty.get('.md-field__box').classes()).not.toContain('md-field__box--filled');

    await empty.get('input').trigger('focus');
    expect(empty.get('input').attributes('placeholder')).toBe('Type here');
    expect(empty.get('.md-field__label').classes()).toContain('md-field__label--floating');

    await empty.get('input').trigger('blur');
    expect(empty.get('input').attributes('placeholder')).toBeUndefined();

    const filled = await mountView(MdTextField, {
      props: { modelValue: 'hi', label: 'Name', placeholder: 'Type here' },
    });
    expect(filled.get('input').attributes('placeholder')).toBe('Type here');
    expect(filled.get('.md-field__box').classes()).toContain('md-field__box--filled');
    expect(filled.get('.md-field__label').classes()).toContain('md-field__label--floating');
  });

  it('reports an error through the box, the icon, the help text and aria-invalid', async () => {
    const wrapper = await mountView(MdTextField, {
      props: { modelValue: 'x', label: 'Name', errorText: 'Too short' },
    });

    expect(wrapper.classes()).toContain('md-field--error');
    expect(wrapper.find('.md-field__icon').exists()).toBe(false);
    expect(wrapper.get('.md-field__trailing').element.tagName).toBe('svg');
    expect(wrapper.get('.md-field__help').text()).toBe('Too short');
    expect(wrapper.get('input').attributes('aria-invalid')).toBe('true');

    const id = wrapper.get('input').element.id;
    expect(wrapper.get('input').attributes('aria-describedby')).toBe(`${id}-help`);
    expect(wrapper.get(`#${id}-help`).text()).toBe('Too short');
  });

  it('uses the supporting text as the help text when there is no error', async () => {
    const wrapper = await mountView(MdTextField, {
      props: { modelValue: '', label: 'Name', supportingText: 'Lowercase only' },
    });

    expect(wrapper.classes()).not.toContain('md-field--error');
    expect(wrapper.get('.md-field__help').text()).toBe('Lowercase only');
    expect(wrapper.get('input').attributes('aria-invalid')).toBe('false');

    const id = wrapper.get('input').element.id;
    expect(wrapper.get('input').attributes('aria-describedby')).toBe(`${id}-help`);
  });

  it('prefers the error text over the supporting text', async () => {
    const wrapper = await mountView(MdTextField, {
      props: { modelValue: '', label: 'Name', supportingText: 'Hint', errorText: 'Bad' },
    });

    expect(wrapper.findAll('.md-field__help')).toHaveLength(1);
    expect(wrapper.get('.md-field__help').text()).toBe('Bad');
  });

  it('counts against maxlength only when the counter is asked for', async () => {
    const counted = await mountView(MdTextField, {
      props: { modelValue: 'abc', label: 'Name', maxlength: 10, showCounter: true },
    });
    expect(counted.get('.md-field__counter').text()).toBe('3 / 10');
    expect(counted.get('input').attributes('maxlength')).toBe('10');

    const hidden = await mountView(MdTextField, {
      props: { modelValue: 'abc', label: 'Name', maxlength: 10 },
    });
    expect(hidden.find('.md-field__counter').exists()).toBe(false);

    const noLimit = await mountView(MdTextField, {
      props: { modelValue: 'abc', label: 'Name', showCounter: true },
    });
    expect(noLimit.find('.md-field__counter').exists()).toBe(false);
  });

  it('emits submit on Enter and nothing on other keys', async () => {
    const wrapper = await mountView(MdTextField, { props: { modelValue: '', label: 'Name' } });

    await wrapper.get('input').trigger('keydown', { key: 'a' });
    expect(wrapper.emitted('submit')).toBeUndefined();

    await wrapper.get('input').trigger('keydown', { key: 'Enter' });
    expect(wrapper.emitted('submit')).toHaveLength(1);
  });

  it('offers a clear button that empties the field and returns focus to it', async () => {
    const wrapper = await mountView(MdTextField, {
      props: { modelValue: 'abc', label: 'Name', clearLabel: 'Clear' },
      attachTo: document.body,
    });

    const clear = wrapper.get('.md-field__clear');
    expect(clear.attributes('aria-label')).toBe('Clear');

    await clear.trigger('click');

    expect(wrapper.emitted('update:modelValue')).toEqual([['']]);
    expect(document.activeElement).toBe(wrapper.get('input').element);
  });

  it('hides the clear button when the field is empty, disabled or in error', async () => {
    const empty = await mountView(MdTextField, {
      props: { modelValue: '', label: 'Name', clearLabel: 'Clear' },
    });
    expect(empty.find('.md-field__clear').exists()).toBe(false);

    const disabled = await mountView(MdTextField, {
      props: { modelValue: 'abc', label: 'Name', clearLabel: 'Clear', disabled: true },
    });
    expect(disabled.find('.md-field__clear').exists()).toBe(false);

    const errored = await mountView(MdTextField, {
      props: { modelValue: 'abc', label: 'Name', clearLabel: 'Clear', errorText: 'Bad' },
    });
    expect(errored.find('.md-field__clear').exists()).toBe(false);
  });

  it('disables the input when disabled', async () => {
    const wrapper = await mountView(MdTextField, {
      props: { modelValue: '', label: 'Name', disabled: true },
    });

    expect(wrapper.get('input').attributes('disabled')).toBeDefined();
  });

  it('draws a leading icon when one is set', async () => {
    const wrapper = await mountView(MdTextField, {
      props: { modelValue: '', label: 'Search', icon: 'search' },
    });

    const icon = wrapper.get('.md-field__icon');
    expect(icon.element.tagName).toBe('svg');
    expect(icon.attributes('width')).toBe('20');
  });

  it('applies autofocus to the input when requested', async () => {
    const wrapper = await mountView(MdTextField, {
      props: { modelValue: '', label: 'Name', autofocus: true },
    });

    expect(wrapper.get('input').element.autofocus).toBe(true);
  });

  it('exposes focus() for programmatic focusing', async () => {
    const wrapper = await mountView(MdTextField, {
      props: { modelValue: 'abc', label: 'Name' },
      attachTo: document.body,
    });

    // `defineExpose({ focus })` is not reflected in the inferred instance type seen here.
    const exposed = wrapper.vm as unknown as { focus: () => void };
    exposed.focus();

    expect(document.activeElement).toBe(wrapper.get('input').element);
  });

  // The component declares no `inheritAttrs: false` and does not bind `$attrs` to the input, so an
  // attribute passed by a caller lands on the root box rather than the field.
  it('lets fallthrough attributes land on the root box', async () => {
    const wrapper = await mountView(MdTextField, {
      props: { modelValue: '', label: 'Name', 'data-testid': 'field' },
    });

    expect(wrapper.attributes('data-testid')).toBe('field');
    expect(wrapper.get('input').attributes('data-testid')).toBeUndefined();
  });
});
