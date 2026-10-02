<script setup lang="ts">
// The outside press is bound on `pointerdown` in the capture phase rather than `click`, so the
// menu is gone before the press reaches whatever is underneath it.
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue';

import MdIcon from './MdIcon.vue';
import MdIconButton from './MdIconButton.vue';
import type { IconName } from './icons';

const props = defineProps<{
  items: readonly {
    readonly id: string;
    readonly label: string;
    readonly icon?: IconName;
    readonly danger?: boolean;
  }[];
  label: string;
}>();

const emit = defineEmits<{ select: [id: string] }>();

const root = ref<HTMLElement | null>(null);
const surface = ref<HTMLElement | null>(null);
const open = ref(false);
const activeIndex = ref(0);

// Fixed positioning keeps the menu out of the list's scrollable area; the surface is measured
// before it is shown and stays `visibility: hidden` until then so it never paints in the wrong place.
const coords = ref<{ left: number; top: number } | null>(null);

const surfaceStyle = computed(() =>
  coords.value === null
    ? { visibility: 'hidden' as const }
    : { left: `${coords.value.left}px`, top: `${coords.value.top}px` },
);

const GAP = 4;
const EDGE = 8;

function menuItems(): HTMLButtonElement[] {
  const surfaceElement = surface.value;
  if (surfaceElement === null) return [];
  return Array.from(surfaceElement.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'));
}

// `preventScroll` keeps the browser from scrolling the list to the newly focused item, which
// would move the row the menu is attached to.
function focusItem(index: number): void {
  activeIndex.value = index;
  menuItems()[index]?.focus({ preventScroll: true });
}

function place(): void {
  const trigger = root.value?.querySelector<HTMLElement>('.md-menu__trigger');
  const surfaceElement = surface.value;
  if (trigger === null || trigger === undefined || surfaceElement === null) return;

  const rect = trigger.getBoundingClientRect();
  const width = surfaceElement.offsetWidth;
  const height = surfaceElement.offsetHeight;

  let top = rect.bottom + GAP;
  if (top + height > window.innerHeight - EDGE && rect.top - GAP - height >= EDGE) {
    top = rect.top - GAP - height;
  }
  let left = rect.right - width;
  left = Math.min(Math.max(EDGE, left), window.innerWidth - width - EDGE);

  coords.value = { left, top };
}

function show(): void {
  if (props.items.length === 0) return;
  open.value = true;
  activeIndex.value = 0;
  coords.value = null;
  // The surface is not in the DOM until this render has flushed, so it cannot be measured before then.
  void nextTick(() => {
    place();
    focusItem(0);
  });
}

function close(restoreFocus: boolean): void {
  if (!open.value) return;
  open.value = false;
  coords.value = null;
  if (restoreFocus) {
    root.value
      ?.querySelector<HTMLButtonElement>('.md-menu__trigger')
      ?.focus({ preventScroll: true });
  }
}

function toggle(): void {
  if (open.value) close(false);
  else show();
}

// Exposed so a row can open the menu from elsewhere, such as a context-menu gesture.
defineExpose({ show });

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

/** The fixed menu is placed in viewport coordinates, so it cannot follow a scroll or a resize. */
function onViewportChange(): void {
  close(false);
}

watch(open, (isOpen) => {
  if (isOpen) {
    document.addEventListener('pointerdown', onPointerDown, true);
    document.addEventListener('scroll', onViewportChange, true);
    window.addEventListener('resize', onViewportChange);
  } else {
    document.removeEventListener('pointerdown', onPointerDown, true);
    document.removeEventListener('scroll', onViewportChange, true);
    window.removeEventListener('resize', onViewportChange);
  }
});

onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onPointerDown, true);
  document.removeEventListener('scroll', onViewportChange, true);
  window.removeEventListener('resize', onViewportChange);
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
    <div
      v-if="open"
      ref="surface"
      class="md-menu__surface"
      role="menu"
      :aria-label="label"
      :style="surfaceStyle"
    >
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
  display: inline-flex;
}

/* Painted `fixed` and placed from script (see `place`), so it is not part of the list's scroll area. */
.md-menu__surface {
  position: fixed;
  z-index: 3;
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
