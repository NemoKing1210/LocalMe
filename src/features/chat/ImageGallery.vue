<script setup lang="ts">
/**
 * The pictures of one message, arranged as a mosaic rather than listed as files.
 *
 * The arrangement says how many there are before the names are read: one large picture, a pair, a
 * tall first picture beside two stacked ones, a 2×2 grid, then square tiles three to a row. Every
 * tile that has arrived still opens the viewer and keeps the open / save as / show in folder
 * actions; a tile that has not arrived shows its progress where the picture will be.
 */
import { computed, ref } from 'vue';
import type { Component } from 'vue';

import { useI18n, type MessageKey } from '@/i18n';
import * as ipc from '@/ipc';
import type { Attachment } from '@/ipc';
import { useUiStore } from '@/stores/ui';
import MdIcon from '@/ui/MdIcon.vue';
import MdIconButton from '@/ui/MdIconButton.vue';
import { fileIconFor } from '@/ui/fileIcons';

import ImageViewer from './ImageViewer.vue';

const props = defineProps<{ attachments: readonly Attachment[] }>();

const i18n = useI18n();
const ui = useUiStore();

const images = computed(() => props.attachments.filter((item) => item.kind === 'image'));

const groupClass = computed<string | null>(() => {
  switch (images.value.length) {
    case 1:
      return 'gallery--single';
    case 2:
      return 'gallery--pair';
    case 3:
      return 'gallery--trio';
    case 4:
      return 'gallery--grid';
    case 0:
      return null;
    default:
      return 'gallery--many';
  }
});

/** The picture open in the viewer, if any. */
const viewing = ref<Attachment | null>(null);
/** Pictures the web view could not decode: they fall back to the glyph rather than a broken mark. */
const broken = ref<ReadonlySet<string>>(new Set());

/** Only a completed picture whose source still decodes has something to draw. */
function sourceOf(attachment: Attachment): string | null {
  if (attachment.state !== 'complete' || attachment.path === null) return null;
  if (broken.value.has(attachment.id)) return null;
  return ipc.fileSrc(attachment.path);
}

const tiles = computed(() =>
  images.value.map((attachment) => ({ attachment, src: sourceOf(attachment) })),
);

const viewingSrc = computed(() => {
  const attachment = viewing.value;
  if (attachment === null || attachment.path === null) return null;
  return ipc.fileSrc(attachment.path);
});

function markBroken(id: string): void {
  broken.value = new Set([...broken.value, id]);
}

function inFlight(attachment: Attachment): boolean {
  return ['queued', 'sending', 'receiving'].includes(attachment.state);
}

function retryable(attachment: Attachment): boolean {
  return attachment.state === 'failed' || attachment.state === 'cancelled';
}

function percent(attachment: Attachment): number {
  if (attachment.size === 0) return 100;
  return Math.min(100, Math.round((attachment.transferred / attachment.size) * 100));
}

/** The badge for the file's extension, shared with the composer and the file card. */
function glyphFor(name: string): Component {
  return fileIconFor(name);
}

/** Runs one host action, reporting a failure the way every other action does. */
function run(action: () => Promise<unknown>, failure: MessageKey = 'error.internal'): void {
  void ui.attempt(action, 'the file action failed', failure);
}

async function save(attachment: Attachment): Promise<void> {
  const saved = await ui.attempt(
    () => ipc.saveAttachment(attachment.id),
    'the file could not be saved',
    'error.storage',
  );
  // A dismissed dialog is a successful `null`, and says nothing.
  if (saved.ok && saved.value !== null) {
    ui.notify('chat.attachmentSaved', { path: saved.value });
  }
}
</script>

<template>
  <div v-if="images.length > 0" class="gallery" :class="groupClass">
    <div
      v-for="tile in tiles"
      :key="tile.attachment.id"
      class="gallery__tile"
      :class="{ 'gallery__tile--failed': tile.attachment.state === 'failed' }"
    >
      <button
        v-if="tile.src !== null"
        type="button"
        class="gallery__open"
        :title="i18n.t('imageViewer.open', { name: tile.attachment.name })"
        :aria-label="i18n.t('imageViewer.open', { name: tile.attachment.name })"
        @click="viewing = tile.attachment"
      >
        <img
          :src="tile.src"
          :alt="tile.attachment.name"
          loading="lazy"
          decoding="async"
          @error="markBroken(tile.attachment.id)"
        />
      </button>

      <!-- Not here yet, or not decodable: the glyph, the name, and whatever action applies. -->
      <div v-else class="gallery__fallback">
        <span class="gallery__glyph" aria-hidden="true">
          <component :is="glyphFor(tile.attachment.name)" />
        </span>
        <span class="md-typescale-label-small gallery__name">{{ tile.attachment.name }}</span>
      </div>

      <div v-if="inFlight(tile.attachment)" class="gallery__progress">
        <div class="gallery__bar" :style="{ inlineSize: `${percent(tile.attachment)}%` }" />
      </div>

      <span v-if="tile.src !== null" class="gallery__actions">
        <MdIconButton
          icon="launch"
          :label="i18n.t('chat.attachmentOpen')"
          @click.stop="run(() => ipc.openAttachment(tile.attachment.id))"
        />
        <MdIconButton
          icon="download"
          :label="i18n.t('chat.attachmentSave')"
          @click.stop="save(tile.attachment)"
        />
        <MdIconButton
          icon="folder"
          :label="i18n.t('chat.attachmentReveal')"
          @click.stop="run(() => ipc.revealAttachment(tile.attachment.id))"
        />
      </span>

      <button
        v-if="retryable(tile.attachment)"
        type="button"
        class="md-typescale-label-large gallery__retry"
        @click="run(() => ipc.retryAttachment(tile.attachment.id))"
      >
        <MdIcon name="refresh" :size="16" />
        <span>{{ i18n.t('chat.attachmentRetry') }}</span>
      </button>
    </div>

    <ImageViewer
      v-if="viewing !== null && viewingSrc !== null"
      :src="viewingSrc"
      :name="viewing.name"
      @close="viewing = null"
    />
  </div>
</template>

<style scoped>
.gallery {
  display: grid;
  gap: 4px;
  inline-size: 100%;
}

.gallery--pair {
  grid-template-columns: 1fr 1fr;
}

.gallery--trio {
  grid-template-columns: 1fr 1fr;
  grid-template-rows: 1fr 1fr;
}

/* The first of three is tall, the other two stack beside it. */
.gallery--trio .gallery__tile:first-child {
  grid-row: span 2;
}

.gallery--grid {
  grid-template-columns: 1fr 1fr;
  grid-template-rows: 1fr 1fr;
}

.gallery--many {
  grid-template-columns: repeat(3, 1fr);
}

.gallery__tile {
  position: relative;
  min-block-size: 96px;
  overflow: hidden;
  border-radius: var(--md-sys-shape-corner-medium);
  background: var(--md-sys-color-surface-container-highest);
}

/* Whole-number tiles keep the mosaic even; a single picture is capped instead. */
.gallery--pair .gallery__tile,
.gallery--trio .gallery__tile,
.gallery--grid .gallery__tile,
.gallery--many .gallery__tile {
  aspect-ratio: 1;
}

.gallery--trio .gallery__tile:first-child {
  aspect-ratio: auto;
}

.gallery--single .gallery__tile {
  max-block-size: 320px;
}

.gallery__open {
  display: block;
  inline-size: 100%;
  block-size: 100%;
  padding: 0;
  border: none;
  background: none;
  cursor: pointer;
}

.gallery__open img {
  display: block;
  inline-size: 100%;
  block-size: 100%;
  object-fit: cover;
}

.gallery--single .gallery__open img {
  max-block-size: 320px;
  object-fit: contain;
}

.gallery__fallback {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  block-size: 100%;
  min-block-size: 96px;
  padding: 8px;
  flex-direction: column;
  text-align: center;
}

.gallery__glyph {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  inline-size: 36px;
  block-size: 36px;
  border-radius: var(--md-sys-shape-corner-small);
  background: var(--md-sys-color-surface-container);
}

.gallery__glyph :deep(svg) {
  inline-size: 24px;
  block-size: 24px;
}

.gallery__name {
  max-inline-size: 100%;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  color: var(--md-sys-color-on-surface-variant);
}

.gallery__tile--failed .gallery__name {
  color: var(--md-sys-color-error);
}

.gallery__progress {
  position: absolute;
  inset-inline: 0;
  inset-block-end: 0;
  block-size: 4px;
  background: rgb(0 0 0 / 0.25);
}

.gallery__bar {
  block-size: 100%;
  background: var(--md-sys-color-primary);
  transition: inline-size var(--md-sys-motion-duration-medium2) var(--md-sys-motion-easing-standard);
}

.gallery__actions {
  position: absolute;
  inset-block-start: 4px;
  inset-inline-end: 4px;
  display: inline-flex;
  align-items: center;
  border-radius: var(--md-sys-shape-corner-full);
  background: var(--md-sys-color-surface-container-high);
  opacity: 0;
  pointer-events: none;
  transition: opacity var(--md-sys-motion-duration-short4) var(--md-sys-motion-easing-standard);
}

.gallery__tile:hover .gallery__actions,
.gallery__tile:focus-within .gallery__actions {
  opacity: 1;
  pointer-events: auto;
}

@media (hover: none) {
  .gallery__actions {
    opacity: 1;
    pointer-events: auto;
  }
}

.gallery__retry {
  position: absolute;
  inset-block-end: 8px;
  inset-inline-start: 50%;
  display: inline-flex;
  gap: 6px;
  align-items: center;
  padding: 4px 14px;
  border: none;
  border-radius: var(--md-sys-shape-corner-full);
  background: var(--md-sys-color-secondary-container);
  color: var(--md-sys-color-on-secondary-container);
  cursor: pointer;
  transform: translateX(-50%);
}
</style>
