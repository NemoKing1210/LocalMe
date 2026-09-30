<script setup lang="ts">
/**
 * A Material 3 overflow menu.
 *
 * The trigger is an `MdIconButton` with `aria-haspopup` and `aria-expanded`, and the menu is a
 * `role="menu"` of `role="menuitem"` buttons. Focus is moved into the menu when it opens and
 * left there while it is open — a menu that opens without taking focus is unusable by keyboard,
 * and one that keeps focus on the trigger cannot be arrowed through.
 *
 * It closes on four things, because each is a different user intent that must all be honoured:
 * a selection, a pointer press anywhere outside, `Escape`, and focus leaving the component. The
 * outside press is bound on `pointerdown` in the capture phase rather than on `click`, so the
 * menu is gone before the press reaches whatever is underneath it and cannot swallow a click
 * meant for the page.
 *
 * The items are a prop, not slots. A menu is a list of (label, action) pairs and a caller
 * assembling it from markup would have to reproduce the item's padding, its state layer and its
 * `role` — the three things this component exists to get right.
 */
import { nextTick, onBeforeUnmount, ref, watch } from 'vue';

import MdIcon from './MdIcon.vue';
import MdIconButton from './MdIconButton.vue';
import type { IconName } from './icons';

const props = withDefaults(
  defineProps<{
    /** The entries, in order. */
    items: readonly {
      readonly id: string;
      readonly label: string;
      readonly icon?: IconName;
      readonly danger?: boolean;
    }[];
    /** The trigger's accessible name, such as "Actions for Anna". */
    label: string;
  }>(),
  {},
);

const emit = defineEmits<{ select: [id: string] }>();

const root = ref<HTMLElement | null>(null);
const surface = ref<HTMLElement | null>(null);
const open = ref(false);
const activeIndex = ref(0);

/** The rendered items, in the order the arrow keys walk them. */
function menuItems(): HTMLButtonElement[] {
  const surfaceElement = surface.value;
  if (surfaceElement === null) return [];
  return Array.from(surfaceElement.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'));
}

function focusItem(index: number): void {
  activeIndex.value = index;
  menuItems()[index]?.focus();
}

function show(): void {
  if (props.items.length === 0) return;
  open.value = true;
  activeIndex.value = 0;
  // The menu does not exist in the DOM until this render has flushed.
  void nextTick(() => {
    focusItem(0);
  });
}

function close(restoreFocus: boolean): void {
  if (!open.value) return;
  open.value = false;
  if (restoreFocus) root.value?.querySelector<HTMLButtonElement>('.md-menu__trigger')?.focus();
}

function toggle(): void {
  if (open.value) close(false);
  else show();
}

function choose(id: string): void {
  close(true);
  emit('select', id);
}

function onKeydown(event: KeyboardEvent): void {
  if (!open.value) return;
  const items = menuItems();
  if (items.length === 0) return;

  let next: number;
  switch (event.key) {
    case 'ArrowDown':
      next = (activeIndex.value + 1) % items.length;
      break;
    case 'ArrowUp':
      next = (activeIndex.value - 1 + items.length) % items.length;
      break;
    case 'Home':
      next = 0;
      break;
    case 'End':
      next = items.length - 1;
      break;
    case 'Escape':
      event.preventDefault();
      close(true);
      return;
    default:
      return;
  }

  event.preventDefault();
  focusItem(next);
}

function onFocusOut(event: FocusEvent): void {
  const next = event.relatedTarget;
  if (next instanceof Node && root.value?.contains(next)) return;
  close(false);
}

function onPointerDown(event: PointerEvent): void {
  const target = event.target;
  if (target instanceof Node && root.value?.contains(target)) return;
  close(false);
}

watch(open, (isOpen) => {
  if (isOpen) document.addEventListener('pointerdown', onPointerDown, true);
  else document.removeEventListener('pointerdown', onPointerDown, true);
});

onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onPointerDown, true);
});
</script>

<template>
  <div ref="root" class="md-menu" @keydown="onKeydown" @focusout="onFocusOut">
    <MdIconButton
      class="md-menu__trigger"
      icon="more-vertical"
      :label="label"
      aria-haspopup="menu"
      :aria-expanded="open"
      @click="toggle"
    />
    <div v-if="open" ref="surface" class="md-menu__surface" role="menu" :aria-label="label">
      <button
        v-for="(item, index) in items"
        :key="item.id"
        class="md-menu__item md-state-layer md-typescale-label-large"
        :class="{ 'md-menu__item--danger': item.danger }"
        type="button"
        role="menuitem"
        tabindex="-1"
        @focus="activeIndex = index"
        @click="choose(item.id)"
      >
        <MdIcon v-if="item.icon" :name="item.icon" :size="20" />
        <span class="md-menu__label">{{ item.label }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.md-menu {
  position: relative;
  display: inline-flex;
}

/*
 * Anchored below the trigger and flush with its inline end, which is where a menu attached to a
 * trailing control is expected to appear. The elevation and the container colour are what make
 * it read as a sheet above the row rather than as part of it.
 */
.md-menu__surface {
  position: absolute;
  z-index: 3;
  inset-block-start: calc(100% + 4px);
  inset-inline-end: 0;
  min-width: 160px;
  padding-block: 8px;
  border-radius: var(--md-sys-shape-corner-extra-small);
  background: var(--md-sys-color-surface-container);
  color: var(--md-sys-color-on-surface);
  box-shadow: var(--md-sys-elevation-level2);
}

.md-menu__item {
  display: flex;
  gap: 12px;
  align-items: center;
  width: 100%;
  height: 40px;
  padding: 0 12px;
  color: var(--md-sys-color-on-surface);
  text-align: start;
}

.md-menu__item--danger {
  color: var(--md-sys-color-error);
}

.md-menu__label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
