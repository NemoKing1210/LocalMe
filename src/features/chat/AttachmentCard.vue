<script setup lang="ts">
/**
 * One attachment inside a message: its picture when it is an image and it is here, its name and
 * size always, and — while it is moving — how far it has come.
 *
 * The card is the whole of the user's control over a transfer, so the actions are on it rather
 * than in a menu: a menu would have to be drawn from inside the bubble's entrance transform.
 * A transfer that failed says so where the file would have been, rather than in a snackbar the
 * reader has already dismissed.
 */
import { computed, ref } from 'vue';

import { useI18n, type MessageKey } from '@/i18n';
import * as ipc from '@/ipc';
import type { Attachment, AttachmentState } from '@/ipc';
import { useUiStore } from '@/stores/ui';
import MdIcon from '@/ui/MdIcon.vue';
import MdIconButton from '@/ui/MdIconButton.vue';
import { fileIconFor } from '@/ui/fileIcons';

const props = defineProps<{ attachment: Attachment }>();

const i18n = useI18n();
const ui = useUiStore();

/** A record so a new host state is a compile error here rather than a silently blank card. */
const LABELS: Record<AttachmentState, MessageKey | null> = {
  queued: 'chat.attachmentQueued',
  sending: 'chat.attachmentSending',
  receiving: 'chat.attachmentReceiving',
  complete: null,
  cancelled: 'chat.attachmentCancelled',
  failed: 'chat.attachmentFailed',
};

const inFlight = computed(
  () => !['complete', 'cancelled', 'failed'].includes(props.attachment.state),
);
const finished = computed(() => props.attachment.state === 'complete');
const retryable = computed(
  () => props.attachment.state === 'failed' || props.attachment.state === 'cancelled',
);

const percent = computed(() => {
  if (props.attachment.size === 0) return 100;
  return Math.min(100, Math.round((props.attachment.transferred / props.attachment.size) * 100));
});

const label = computed<MessageKey | null>(() => LABELS[props.attachment.state]);

/** A preview that the web view could not load is not shown again. */
const previewBroken = ref(false);

/** Only an image that is actually here has something to render. */
const preview = computed<string | null>(() => {
  const { kind, path } = props.attachment;
  if (previewBroken.value || kind !== 'image' || path === null || !finished.value) return null;
  return ipc.fileSrc(path);
});

/**
 * A file that was there a moment ago can be gone, and the web view answers a missing source with a
 * broken-image mark rather than nothing. Falling back to the icon keeps the card readable instead
 * of alarming: the name, the size and the actions are all still there.
 */
function onPreviewError(): void {
  previewBroken.value = true;
}

/** The badge for the file's extension; the composer shows the same one for the same file. */
const glyph = computed(() => fileIconFor(props.attachment.name));

const size = computed(() => i18n.bytes(props.attachment.size));

/**
 * Runs one host command, reporting a failure the way every other action does. The console keeps
 * the detail; the user gets the same sentence a failed message produces.
 */
async function run(
  action: () => Promise<unknown>,
  failure: MessageKey = 'error.internal',
): Promise<void> {
  try {
    await action();
  } catch (error) {
    console.error('[localme] the file action failed', error);
    ui.fail(failure);
  }
}

async function save(): Promise<void> {
  const target = await ipc.saveAttachment(props.attachment.id).catch((error: unknown) => {
    console.error('[localme] the file could not be saved', error);
    ui.fail('error.storage');
    return null;
  });
  // `null` means the dialog was dismissed, which is not a failure and says nothing.
  if (target !== null) ui.notify('chat.attachmentSaved', { path: target });
}
</script>

<template>
  <article class="attachment" :class="{ 'attachment--failed': attachment.state === 'failed' }">
    <button
      v-if="preview !== null"
      type="button"
      class="attachment__preview"
      :title="i18n.t('chat.attachmentOpen')"
      @click="run(() => ipc.openAttachment(attachment.id))"
    >
      <img
        :src="preview"
        :alt="attachment.name"
        loading="lazy"
        decoding="async"
        @error="onPreviewError"
      />
    </button>

    <div class="attachment__row">
      <span class="attachment__glyph" aria-hidden="true">
        <component :is="glyph" />
      </span>
      <span class="attachment__text">
        <span class="md-typescale-body-medium attachment__name" :title="attachment.name">
          {{ attachment.name }}
        </span>
        <!--
          The size is always readable, and the state rides alongside it rather than replacing it:
          "how big is it" is the question a file list is scanned for, and it must not disappear for
          the minute a transfer takes.
        -->
        <span class="md-typescale-label-small attachment__meta">
          <span>{{ size }}</span>
          <template v-if="label !== null">
            <span class="attachment__separator" aria-hidden="true">·</span>
            <span>{{ i18n.t(label, { percent }) }}</span>
          </template>
        </span>
      </span>

      <span v-if="finished" class="attachment__actions">
        <MdIconButton
          icon="launch"
          :label="i18n.t('chat.attachmentOpen')"
          @click="run(() => ipc.openAttachment(attachment.id))"
        />
        <MdIconButton icon="download" :label="i18n.t('chat.attachmentSave')" @click="save" />
        <MdIconButton
          icon="folder"
          :label="i18n.t('chat.attachmentReveal')"
          @click="run(() => ipc.revealAttachment(attachment.id))"
        />
      </span>
      <MdIconButton
        v-else-if="inFlight"
        icon="close"
        :label="i18n.t('chat.attachmentCancel')"
        @click="run(() => ipc.cancelAttachment(attachment.id))"
      />
    </div>

    <div v-if="inFlight" class="attachment__progress">
      <div class="attachment__bar" :style="{ inlineSize: `${percent}%` }" />
    </div>

    <button
      v-if="retryable"
      type="button"
      class="md-typescale-label-large attachment__retry"
      @click="run(() => ipc.retryAttachment(attachment.id))"
    >
      <MdIcon name="refresh" :size="16" />
      <span>{{ i18n.t('chat.attachmentRetry') }}</span>
    </button>
  </article>
</template>

<style scoped>
.attachment {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-inline-size: 240px;
  max-inline-size: 340px;
  overflow: hidden;
}

.attachment__preview {
  display: block;
  padding: 0;
  border: none;
  background: none;
  cursor: pointer;
}

.attachment__preview img {
  display: block;
  inline-size: 100%;
  max-block-size: 240px;
  object-fit: cover;
  border-radius: var(--md-sys-shape-corner-medium);
  background: var(--md-sys-color-surface-container-highest);
}

.attachment__row {
  display: flex;
  gap: 8px;
  align-items: center;
}

.attachment__glyph {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  inline-size: 36px;
  block-size: 36px;
  flex: none;
  border-radius: var(--md-sys-shape-corner-small);
  background: var(--md-sys-color-surface-container-highest);
}

/* The icon set ships its own 32 px canvas; the badge decides the box it lives in. */
.attachment__glyph :deep(svg) {
  inline-size: 24px;
  block-size: 24px;
}

.attachment__text {
  display: flex;
  flex-direction: column;
  min-inline-size: 0;
  flex: 1;
}

/* A long name is the normal case: one line, clipped, with the whole name in the tooltip. */
.attachment__name {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.attachment__meta {
  display: flex;
  gap: 4px;
  color: var(--md-sys-color-on-surface-variant);
  font-variant-numeric: tabular-nums;
}

.attachment__separator {
  opacity: 0.7;
}

.attachment--failed .attachment__meta {
  color: var(--md-sys-color-error);
}

.attachment__actions {
  display: inline-flex;
  align-items: center;
  flex: none;
}

.attachment__progress {
  block-size: 4px;
  border-radius: var(--md-sys-shape-corner-full);
  background: var(--md-sys-color-surface-container-highest);
  overflow: hidden;
}

.attachment__bar {
  block-size: 100%;
  border-radius: inherit;
  background: var(--md-sys-color-primary);
  transition: inline-size var(--md-sys-motion-duration-medium2) var(--md-sys-motion-easing-standard);
}

.attachment__retry {
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

.attachment__retry:hover {
  filter: brightness(0.97);
}
</style>
