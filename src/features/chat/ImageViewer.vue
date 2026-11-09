<script setup lang="ts">
/**
 * A picture on its own, with the gestures a viewer is expected to have: zoom towards the
 * pointer, drag to move, rotate in quarter turns, and fit again.
 *
 * It is a native modal dialog, which is what brings the top layer, the focus trap and the
 * Escape key with it, so the picture is never clipped by a bubble's entrance transform or lost
 * behind the composer. A viewer opened from a message and one opened from the composer are the
 * same component: only the source differs.
 *
 * The transform order is translate, rotate, scale — zooming happens in the picture's own frame,
 * so a rotated picture still zooms along its edges.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';

import { useI18n } from '@/i18n';
import MdIconButton from '@/ui/MdIconButton.vue';

const props = defineProps<{
  src: string;
  name: string;
}>();

const emit = defineEmits<{ close: [] }>();

/** How far the picture may be taken, and how quickly the wheel moves it. */
const MIN_SCALE = 0.5;
const MAX_SCALE = 8;
const WHEEL_RATE = 0.0015;
const BUTTON_STEP = 1.25;
/** A double click is the usual "show me the detail, then get out of the way" gesture. */
const DOUBLE_CLICK_SCALE = 2.5;
const ROTATION_STEP = 90;
/** Movement below this is a click, not a drag; closing on a drag would be a surprise. */
const DRAG_THRESHOLD = 3;

const i18n = useI18n();

const dialog = ref<HTMLDialogElement | null>(null);
const stage = ref<HTMLDivElement | null>(null);

const scale = ref(1);
const rotation = ref(0);
const offset = ref({ x: 0, y: 0 });

const zoomLabel = computed(() =>
  i18n.t('imageViewer.zoomLevel', { percent: Math.round(scale.value * 100) }),
);

const imageStyle = computed(() => ({
  transform: `translate(${offset.value.x}px, ${offset.value.y}px) rotate(${rotation.value}deg) scale(${scale.value})`,
}));

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/**
 * Zooms by `factor` while keeping the point under the pointer where it is.
 *
 * With the picture centred, the point under the pointer scales about the centre; solving for
 * the offset that keeps it still gives `offset + (1 - ratio) * (pointer - center - offset)`.
 */
function zoomAround(factor: number, clientX: number, clientY: number): void {
  const next = clamp(scale.value * factor, MIN_SCALE, MAX_SCALE);
  const ratio = next / scale.value;
  if (ratio === 1) return;

  const box = stage.value?.getBoundingClientRect();
  const centerX = box === undefined ? clientX : box.left + box.width / 2;
  const centerY = box === undefined ? clientY : box.top + box.height / 2;
  offset.value = {
    x: offset.value.x + (1 - ratio) * (clientX - centerX - offset.value.x),
    y: offset.value.y + (1 - ratio) * (clientY - centerY - offset.value.y),
  };
  scale.value = next;
}

/** A button or the keyboard zooms about the middle of the stage, not the last pointer. */
function zoomBy(factor: number): void {
  const box = stage.value?.getBoundingClientRect();
  if (box === undefined) {
    zoomAround(factor, 0, 0);
    return;
  }
  zoomAround(factor, box.left + box.width / 2, box.top + box.height / 2);
}

function reset(): void {
  scale.value = 1;
  rotation.value = 0;
  offset.value = { x: 0, y: 0 };
}

function rotate(direction: 1 | -1): void {
  rotation.value = (rotation.value + direction * ROTATION_STEP + 360) % 360;
}

/** A trackpad pinch arrives as ctrl+wheel; it is the same gesture as a wheel zoom. */
function onWheel(event: WheelEvent): void {
  zoomAround(Math.exp(-event.deltaY * WHEEL_RATE), event.clientX, event.clientY);
}

function onDoubleClick(event: MouseEvent): void {
  if (scale.value > 1.01) {
    reset();
    return;
  }
  zoomAround(DOUBLE_CLICK_SCALE, event.clientX, event.clientY);
}

/**
 * One pointer drags; two pinch and pan together. The pointers are kept in a map because a
 * second finger can arrive or leave at any time, and the gesture has to survive both.
 */
const pointers = new Map<number, { readonly x: number; readonly y: number }>();
let lastPan = { x: 0, y: 0 };
let pinchDistance = 0;
/** How far the pointers have travelled since they went down; a click must not close on a drag. */
let travel = 0;
let dragged = false;

function spread(a: { x: number; y: number }, b: { x: number; y: number }): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

function panBy(dx: number, dy: number): void {
  travel += Math.abs(dx) + Math.abs(dy);
  if (travel > DRAG_THRESHOLD) dragged = true;
  offset.value = { x: offset.value.x + dx, y: offset.value.y + dy };
}

function onPointerDown(event: PointerEvent): void {
  if (event.button !== 0) return;
  const element = event.currentTarget as HTMLElement | null;
  if (typeof element?.setPointerCapture === 'function') {
    element.setPointerCapture(event.pointerId);
  }
  pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
  dragged = false;
  travel = 0;

  if (pointers.size === 1) {
    lastPan = { x: event.clientX, y: event.clientY };
    return;
  }
  // The second finger starts the pinch from the midpoint, not from the first finger's last
  // position, or the picture would jump sideways on the first move.
  const points = [...pointers.values()];
  const first = points[0];
  const second = points[1];
  if (first !== undefined && second !== undefined) {
    pinchDistance = spread(first, second);
    lastPan = { x: (first.x + second.x) / 2, y: (first.y + second.y) / 2 };
  }
}

function onPointerMove(event: PointerEvent): void {
  if (!pointers.has(event.pointerId)) return;
  pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });

  const points = [...pointers.values()];
  if (points.length >= 2) {
    const first = points[0];
    const second = points[1];
    if (first === undefined || second === undefined) return;
    const centerX = (first.x + second.x) / 2;
    const centerY = (first.y + second.y) / 2;
    const distance = spread(first, second);
    if (pinchDistance > 0 && distance > 0) zoomAround(distance / pinchDistance, centerX, centerY);
    pinchDistance = distance;
    panBy(centerX - lastPan.x, centerY - lastPan.y);
    lastPan = { x: centerX, y: centerY };
    return;
  }

  panBy(event.clientX - lastPan.x, event.clientY - lastPan.y);
  lastPan = { x: event.clientX, y: event.clientY };
}

function onPointerUp(event: PointerEvent): void {
  pointers.delete(event.pointerId);
  pinchDistance = 0;
  const [only] = pointers.values();
  if (only !== undefined) lastPan = { x: only.x, y: only.y };
}

/** The empty stage is a backdrop: a click there closes, a click on the picture does not. */
function onStageClick(event: MouseEvent): void {
  const wasDragged = dragged;
  dragged = false;
  if (wasDragged || event.target !== stage.value) return;
  emit('close');
}

function onKeydown(event: KeyboardEvent): void {
  // Shortcuts with a modifier belong to the application, not to the viewer.
  if (event.altKey || event.ctrlKey || event.metaKey) return;
  switch (event.key) {
    case '+':
    case '=':
      event.preventDefault();
      zoomBy(BUTTON_STEP);
      return;
    case '-':
    case '_':
      event.preventDefault();
      zoomBy(1 / BUTTON_STEP);
      return;
    case '0':
      event.preventDefault();
      reset();
      return;
    case 'r':
      event.preventDefault();
      rotate(1);
      return;
    case 'R':
      event.preventDefault();
      rotate(-1);
      return;
    default:
      return;
  }
}

onMounted(() => {
  const element = dialog.value;
  if (element !== null && !element.open) element.showModal();
  window.addEventListener('keydown', onKeydown);
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown);
  // Closing an already-closed dialog is harmless, and this is the path that leaves the top
  // layer clean when the parent removes the viewer.
  dialog.value?.close();
});
</script>

<template>
  <dialog
    ref="dialog"
    class="viewer"
    :aria-label="i18n.t('imageViewer.label')"
    @cancel.prevent="emit('close')"
  >
    <div
      ref="stage"
      class="viewer__stage"
      @wheel.prevent="onWheel"
      @dblclick="onDoubleClick"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerUp"
      @click="onStageClick"
    >
      <img
        class="viewer__image"
        :src="props.src"
        :alt="props.name"
        :style="imageStyle"
        draggable="false"
        decoding="async"
        @dragstart.prevent
      />
    </div>

    <div class="viewer__bar">
      <span class="md-typescale-body-medium viewer__name" :title="props.name">
        {{ props.name }}
      </span>
      <span class="viewer__tools">
        <MdIconButton
          icon="zoom-out"
          :label="i18n.t('imageViewer.zoomOut')"
          @click="zoomBy(1 / BUTTON_STEP)"
        />
        <span class="md-typescale-label-large viewer__zoom">{{ zoomLabel }}</span>
        <MdIconButton
          icon="zoom-in"
          :label="i18n.t('imageViewer.zoomIn')"
          @click="zoomBy(BUTTON_STEP)"
        />
        <span class="viewer__divider" aria-hidden="true" />
        <MdIconButton
          icon="rotate-left"
          :label="i18n.t('imageViewer.rotateLeft')"
          @click="rotate(-1)"
        />
        <MdIconButton
          icon="rotate-right"
          :label="i18n.t('imageViewer.rotateRight')"
          @click="rotate(1)"
        />
        <MdIconButton icon="fit" :label="i18n.t('imageViewer.reset')" @click="reset" />
        <span class="viewer__divider" aria-hidden="true" />
        <MdIconButton icon="close" :label="i18n.t('imageViewer.close')" @click="emit('close')" />
      </span>
    </div>
  </dialog>
</template>

<style scoped>
/*
 * The picture is the surface, so the viewer is dark in both themes and the design system's
 * "on surface" colour is the one token that has to be pointed at white: the icon buttons
 * inherit it, and so does the state layer they draw with `currentColor`.
 */
.viewer {
  --md-sys-color-on-surface-variant: rgb(255 255 255 / 0.88);

  position: fixed;
  inset: 0;
  inline-size: 100%;
  block-size: 100%;
  max-inline-size: none;
  max-block-size: none;
  margin: 0;
  padding: 0;
  border: none;
  background: rgb(0 0 0 / 0.9);
  color: #fff;
  overflow: hidden;
  overscroll-behavior: contain;
}

.viewer::backdrop {
  background: rgb(0 0 0 / 0.72);
  backdrop-filter: blur(4px);
}

.viewer__stage {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  /* The bar overlays the top of the stage; the picture is kept clear of it. */
  padding: 64px 24px 24px;
  overflow: hidden;
  touch-action: none;
  cursor: grab;
}

.viewer__stage:active {
  cursor: grabbing;
}

.viewer__image {
  max-inline-size: 100%;
  max-block-size: 100%;
  transform-origin: center;
  object-fit: contain;
  box-shadow: 0 12px 40px rgb(0 0 0 / 0.5);
  user-select: none;
  -webkit-user-drag: none;
  will-change: transform;
}

.viewer__bar {
  position: absolute;
  inset-block-start: 0;
  inset-inline: 0;
  z-index: 1;
  display: flex;
  gap: 16px;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  background: linear-gradient(180deg, rgb(0 0 0 / 0.64), transparent);
}

.viewer__name {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  opacity: 0.9;
}

.viewer__tools {
  display: inline-flex;
  gap: 2px;
  align-items: center;
  flex: none;
}

.viewer__zoom {
  min-inline-size: 52px;
  font-variant-numeric: tabular-nums;
  text-align: center;
  opacity: 0.8;
}

.viewer__divider {
  inline-size: 1px;
  block-size: 22px;
  margin-inline: 6px;
  background: rgb(255 255 255 / 0.22);
}

/* A viewport too narrow for the whole toolbar keeps the picture usable: the name goes first. */
@media (max-width: 640px) {
  .viewer__name {
    display: none;
  }

  .viewer__bar {
    justify-content: flex-end;
  }
}
</style>
