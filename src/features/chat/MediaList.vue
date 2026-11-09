<script setup lang="ts">
/**
 * The sound and video of one message, listed with players rather than file chips.
 *
 * A song or a clip is meant to be heard or watched where it was sent, so a completed row carries
 * its own player: audio across the row, video capped in height. A format the platform cannot
 * decode says so and keeps every file action, so a player that refuses a file is never a dead end.
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

import { attachmentStateLabel } from './attachmentLabels';

const props = defineProps<{ attachments: readonly Attachment[] }>();

const i18n = useI18n();
const ui = useUiStore();

const media = computed(() =>
  props.attachments.filter((item) => item.kind === 'audio' || item.kind === 'video'),
);

/** The state sentence is resolved once per row, beside the row it belongs to. */
const rows = computed(() =>
  media.value.map((attachment) => ({
    attachment,
    label: attachmentStateLabel(attachment),
  })),
);

/** Players the platform refused: the row falls back to its actions rather than to silence. */
const broken = ref<ReadonlySet<string>>(new Set());

function markBroken(id: string): void {
  broken.value = new Set([...broken.value, id]);
}

function playable(attachment: Attachment): boolean {
  return (
    attachment.state === 'complete' && attachment.path !== null && !broken.value.has(attachment.id)
  );
}

function sourceOf(attachment: Attachment): string {
  // `playable` is what the template checks first; a path always exists past it.
  return attachment.path === null ? '' : ipc.fileSrc(attachment.path);
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
  <div v-if="rows.length > 0" class="media-list">
    <article
      v-for="row in rows"
      :key="row.attachment.id"
      class="media"
      :class="{ 'media--failed': row.attachment.state === 'failed' }"
    >
      <div class="media__row">
        <span class="media__glyph" aria-hidden="true">
          <component :is="glyphFor(row.attachment.name)" />
        </span>
        <span class="media__text">
          <span class="md-typescale-body-medium media__name" :title="row.attachment.name">
            {{ row.attachment.name }}
          </span>
          <span class="md-typescale-label-small media__meta">
            <span>{{ i18n.bytes(row.attachment.size) }}</span>
            <template v-if="row.label !== null">
              <span class="media__separator" aria-hidden="true">·</span>
              <span>{{ i18n.t(row.label, { percent: percent(row.attachment) }) }}</span>
            </template>
          </span>
        </span>

        <span v-if="row.attachment.state === 'complete'" class="media__actions">
          <MdIconButton
            icon="launch"
            :label="i18n.t('chat.attachmentOpen')"
            @click="run(() => ipc.openAttachment(row.attachment.id))"
          />
          <MdIconButton
            icon="download"
            :label="i18n.t('chat.attachmentSave')"
            @click="save(row.attachment)"
          />
          <MdIconButton
            icon="folder"
            :label="i18n.t('chat.attachmentReveal')"
            @click="run(() => ipc.revealAttachment(row.attachment.id))"
          />
        </span>
        <MdIconButton
          v-else-if="inFlight(row.attachment)"
          icon="close"
          :label="i18n.t('chat.attachmentCancel')"
          @click="run(() => ipc.cancelAttachment(row.attachment.id))"
        />
      </div>

      <div v-if="inFlight(row.attachment)" class="media__progress">
        <div class="media__bar" :style="{ inlineSize: `${percent(row.attachment)}%` }" />
      </div>

      <template v-if="playable(row.attachment)">
        <audio
          v-if="row.attachment.kind === 'audio'"
          class="media__player"
          :src="sourceOf(row.attachment)"
          controls
          preload="metadata"
          @error="markBroken(row.attachment.id)"
        />
        <video
          v-else
          class="media__player"
          :src="sourceOf(row.attachment)"
          controls
          preload="metadata"
          @error="markBroken(row.attachment.id)"
        />
      </template>
      <p
        v-else-if="row.attachment.state === 'complete'"
        class="md-typescale-label-small media__unplayable"
      >
        {{ i18n.t('chat.attachmentCannotPreview') }}
      </p>

      <button
        v-if="retryable(row.attachment)"
        type="button"
        class="md-typescale-label-large media__retry"
        @click="run(() => ipc.retryAttachment(row.attachment.id))"
      >
        <MdIcon name="refresh" :size="16" />
        <span>{{ i18n.t('chat.attachmentRetry') }}</span>
      </button>
    </article>
  </div>
</template>

<style scoped>
.media-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-inline-size: 240px;
}

.media {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.media__row {
  display: flex;
  gap: 8px;
  align-items: center;
}

.media__glyph {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  inline-size: 36px;
  block-size: 36px;
  flex: none;
  border-radius: var(--md-sys-shape-corner-small);
  background: var(--md-sys-color-surface-container-highest);
}

.media__glyph :deep(svg) {
  inline-size: 24px;
  block-size: 24px;
}

.media__text {
  display: flex;
  flex-direction: column;
  min-inline-size: 0;
  flex: 1;
}

.media__name {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.media__meta {
  display: flex;
  gap: 4px;
  color: var(--md-sys-color-on-surface-variant);
  font-variant-numeric: tabular-nums;
}

.media__separator {
  opacity: 0.7;
}

.media--failed .media__meta {
  color: var(--md-sys-color-error);
}

.media__player {
  inline-size: 100%;
  border-radius: var(--md-sys-shape-corner-medium);
}

video.media__player {
  max-block-size: 240px;
  background: var(--md-sys-color-surface-container-highest);
}

.media__unplayable {
  color: var(--md-sys-color-on-surface-variant);
}

.media__actions {
  display: inline-flex;
  align-items: center;
  flex: none;
  opacity: 0;
  pointer-events: none;
  transition: opacity var(--md-sys-motion-duration-short4) var(--md-sys-motion-easing-standard);
}

.media:hover .media__actions,
.media:focus-within .media__actions {
  opacity: 1;
  pointer-events: auto;
}

@media (hover: none) {
  .media__actions {
    opacity: 1;
    pointer-events: auto;
  }
}

.media__progress {
  block-size: 4px;
  border-radius: var(--md-sys-shape-corner-full);
  background: var(--md-sys-color-surface-container-highest);
  overflow: hidden;
}

.media__bar {
  block-size: 100%;
  border-radius: inherit;
  background: var(--md-sys-color-primary);
  transition: inline-size var(--md-sys-motion-duration-medium2) var(--md-sys-motion-easing-standard);
}

.media__retry {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  align-self: flex-start;
  padding: 4px 14px;
  border: none;
  border-radius: var(--md-sys-shape-corner-full);
  background: var(--md-sys-color-secondary-container);
  color: var(--md-sys-color-on-secondary-container);
  cursor: pointer;
}
</style>
