<script setup lang="ts">
/**
 * The composer: text, files, and the one place the two are put together.
 *
 * Files can arrive here from the picker, from a drop anywhere on the window, or from the
 * clipboard; either way they are described by the host before they are shown, so a file that
 * cannot be sent is visible as such before the user presses send rather than as a failure
 * afterwards. A paste is decided from the event alone, because the default action cannot be
 * cancelled after the host has answered; a clipboard image is stored by the host first, since
 * the web view never sees a path for it.
 *
 * The textarea height is measured against `--localme-composer-max-height`; the length limit
 * mirrors the host's `MAX_BODY_CHARS`, which `send_message` enforces.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { AnimatePresence, motion } from 'motion-v';

import { useI18n } from '@/i18n';
import * as ipc from '@/ipc';
import type { FilePick, Peer } from '@/ipc';
import { useUiStore } from '@/stores/ui';
import MdIcon from '@/ui/MdIcon.vue';
import MdIconButton from '@/ui/MdIconButton.vue';
import { fileIconFor } from '@/ui/fileIcons';

import ImageViewer from './ImageViewer.vue';

const props = defineProps<{
  peer: Peer;
  sending: boolean;
}>();

const emit = defineEmits<{ send: [body: string, files: readonly string[]] }>();

/** `MAX_BODY_CHARS` from `localme-core`'s protocol limits. */
const MAX_BODY_CHARS = 8000;
/** The share of the limit at which the counter appears, to warn before the message is lost. */
const COUNTER_AT = 0.8;
/** `MAX_ATTACHMENTS_PER_MESSAGE`, so the limit is visible before the host refuses. */
const MAX_FILES = 10;
/** `MAX_ATTACHMENT_BYTES` from `localme-core`'s protocol limits, for the refusal message. */
const MAX_FILE_BYTES = 512 * 1024 * 1024;

const i18n = useI18n();
const ui = useUiStore();

const draft = ref('');
const picked = ref<readonly FilePick[]>([]);
/** Paths whose thumbnail the web view could not load; they fall back to the type badge. */
const thumbnailsFailed = ref<readonly string[]>([]);
/** The pending picture open in the viewer, if any; a file that is not an image has none. */
const viewing = ref<FilePick | null>(null);
const dragging = ref(false);
const textarea = ref<HTMLTextAreaElement | null>(null);
/** The token's value, read once: it is a fact of the stylesheet, not of this component. */
const maxHeight = ref(200);

let stopDrop: UnlistenFn | null = null;

const online = computed(() => props.peer.online);
const tooLong = computed(() => draft.value.length > MAX_BODY_CHARS);
const showCounter = computed(() => draft.value.length > MAX_BODY_CHARS * COUNTER_AT);
/** A file the host cannot send blocks the message: it would be dropped by the core anyway. */
const broken = computed(() => picked.value.filter((file) => file.problem !== null));
const usable = computed(() => picked.value.filter((file) => file.problem === null));
// Sending to a peer that is away is allowed: the message waits in the outbox and goes out when
// they are back, so the composer is only gated by length, by the files it holds, and by an
// in-flight command.
const canSend = computed(
  () =>
    !props.sending &&
    !tooLong.value &&
    broken.value.length === 0 &&
    (draft.value.trim().length > 0 || usable.value.length > 0),
);

const placeholder = computed(() =>
  i18n.t('chat.composerPlaceholder', { name: props.peer.nickname }),
);

function problemText(problem: FilePick['problem']): string {
  switch (problem) {
    case 'tooLarge':
      return i18n.t('chat.fileTooLarge', { max: i18n.bytes(MAX_FILE_BYTES) });
    case 'directory':
      return i18n.t('chat.fileDirectory');
    case 'missing':
      return i18n.t('chat.fileMissing');
    default:
      return '';
  }
}

/** Adds files the host has looked at, ignoring ones already in the list. */
function add(described: readonly FilePick[]): void {
  const known = new Set(picked.value.map((file) => file.path));
  const fresh = described.filter((file) => !known.has(file.path));
  if (fresh.length === 0) return;

  const room = MAX_FILES - picked.value.length;
  if (fresh.length > room) {
    // Saying so is the point: silently keeping ten of eleven files would be a mystery later.
    ui.notify('chat.tooManyFiles', { max: MAX_FILES });
  }
  picked.value = [...picked.value, ...fresh.slice(0, Math.max(0, room))];
  void nextTick(resize);
}

async function pick(): Promise<void> {
  const files = await ui.attempt(() => ipc.pickFiles(), 'the file picker failed');
  if (files.ok) add(files.value);
}

/**
 * Whether a paste is about files rather than prose.
 *
 * A copied file list and a copied image arrive as file items; a Linux file manager copies a
 * `file://` URI list instead. A clipboard with no text at all is asked about too: WebView2 does
 * not always name the format of a file list copied in Explorer, and the host can read it where
 * the web view cannot.
 */
function clipboardCarriesFiles(data: DataTransfer): boolean {
  if (data.files.length > 0) return true;
  if (data.types.includes('Files')) return true;
  if (Array.from(data.items).some((item) => item.kind === 'file')) return true;
  if (!data.types.some((type) => type.startsWith('text/'))) return true;
  return data.getData('text/uri-list').includes('file://');
}

/** Hands the paste to the host, the only side that can see what was copied. */
async function absorbPasted(): Promise<void> {
  const files = await ui.attempt(() => ipc.pasteFiles(), 'the pasted files could not be read');
  if (!files.ok) return;
  if (files.value.length === 0) {
    ui.notify('chat.nothingToPaste');
    return;
  }
  add(files.value);
}

function onPaste(event: ClipboardEvent): void {
  const data = event.clipboardData;
  if (data === null || !clipboardCarriesFiles(data)) return;
  event.preventDefault();
  void absorbPasted();
}

function remove(path: string): void {
  picked.value = picked.value.filter((file) => file.path !== path);
  thumbnailsFailed.value = thumbnailsFailed.value.filter((known) => known !== path);
  // Only a pasted image was copied by the host, and only that is deleted: the host refuses
  // every path outside its own staging directory, so a user's file is never touched.
  void ipc.discardPastedFile(path).catch((error: unknown) => {
    console.warn('[localme] a discarded paste could not be deleted', error);
  });
  void nextTick(resize);
}

function resize(): void {
  const element = textarea.value;
  if (element === null) return;
  // Measured from `auto`, otherwise the current height would be the floor of the measurement.
  element.style.height = 'auto';
  element.style.height = `${Math.min(element.scrollHeight, maxHeight.value)}px`;
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    event.preventDefault();
    draft.value = '';
    void nextTick(resize);
    return;
  }
  // `isComposing` keeps the Enter that commits an IME candidate from sending the message.
  if (event.key !== 'Enter' || event.shiftKey || event.isComposing) return;
  event.preventDefault();
  submit();
}

function submit(): void {
  if (!canSend.value) return;
  const files = usable.value.map((file) => file.path);
  emit('send', draft.value, files);
  draft.value = '';
  picked.value = [];
  void nextTick(resize);
}

/** Describes dropped paths and adds them, reporting a failure like every other file action. */
async function absorb(paths: readonly string[]): Promise<void> {
  if (paths.length === 0) return;
  const files = await ui.attempt(
    () => ipc.inspectFiles(paths),
    'the dropped files could not be read',
  );
  if (files.ok) add(files.value);
}

onMounted(async () => {
  const element = textarea.value;
  if (element !== null) {
    const token = getComputedStyle(element).getPropertyValue('--localme-composer-max-height');
    const parsed = Number.parseFloat(token);
    if (Number.isFinite(parsed) && parsed > 0) maxHeight.value = parsed;
    resize();
  }

  // Handled by the web view rather than by the DOM: with drag and drop enabled the window takes
  // the drop itself, which is what stops the web view from navigating to the file. There is no
  // web view in the browser-only development mode, and that must not be an error either.
  try {
    stopDrop = await getCurrentWebview().onDragDropEvent((event) => {
      switch (event.payload.type) {
        case 'enter':
        case 'over':
          dragging.value = true;
          return;
        case 'drop':
          dragging.value = false;
          void absorb(event.payload.paths);
          return;
        default:
          dragging.value = false;
      }
    });
  } catch (error) {
    console.warn('[localme] drag and drop is unavailable without a host', error);
  }
});

onBeforeUnmount(() => {
  stopDrop?.();
});
</script>

<template>
  <footer class="composer">
    <AnimatePresence>
      <motion.div
        v-if="dragging"
        class="composer__drop"
        :initial="{ opacity: 0 }"
        :animate="{ opacity: 1 }"
        :exit="{ opacity: 0 }"
        :transition="{ duration: 0.12, ease: [0.2, 0, 0, 1] }"
      >
        <MdIcon name="paperclip" :size="28" />
        <p class="md-typescale-title-medium">{{ i18n.t('chat.dropFiles') }}</p>
        <p class="md-typescale-body-small">{{ i18n.t('chat.dropFilesHint') }}</p>
      </motion.div>
    </AnimatePresence>

    <ul v-if="picked.length > 0" class="composer__files">
      <li v-for="file in picked" :key="file.path" class="composer__file">
        <button
          v-if="
            file.kind === 'image' && file.problem === null && !thumbnailsFailed.includes(file.path)
          "
          type="button"
          class="composer__thumb-button"
          :title="i18n.t('imageViewer.open', { name: file.name })"
          :aria-label="i18n.t('imageViewer.open', { name: file.name })"
          @click="viewing = file"
        >
          <img
            class="composer__thumb"
            :src="ipc.fileSrc(file.path)"
            :alt="file.name"
            @error="thumbnailsFailed = [...thumbnailsFailed, file.path]"
          />
        </button>
        <span v-else class="composer__thumb composer__thumb--glyph">
          <component :is="fileIconFor(file.name)" />
        </span>
        <span class="composer__file-text">
          <span class="md-typescale-label-large composer__file-name" :title="file.name">
            {{ file.name }}
          </span>
          <span
            class="md-typescale-label-small"
            :class="file.problem === null ? 'composer__file-size' : 'composer__file-problem'"
          >
            {{ file.problem === null ? i18n.bytes(file.size) : problemText(file.problem) }}
          </span>
        </span>
        <MdIconButton
          icon="close"
          :label="i18n.t('chat.removeAttachment', { name: file.name })"
          @click="remove(file.path)"
        />
      </li>
    </ul>

    <AnimatePresence>
      <motion.p
        v-if="!online"
        class="md-typescale-label-large composer__offline"
        :initial="{ opacity: 0, y: 6 }"
        :animate="{ opacity: 1, y: 0 }"
        :exit="{ opacity: 0, y: 6 }"
        :transition="{ duration: 0.16, ease: [0.2, 0, 0, 1] }"
      >
        <MdIcon name="offline" :size="18" />
        <span>{{ i18n.t('chat.composerOfflineHint', { name: peer.nickname }) }}</span>
      </motion.p>
    </AnimatePresence>

    <div class="composer__row">
      <MdIconButton icon="paperclip" :label="i18n.t('chat.attach')" @click="pick" />
      <textarea
        ref="textarea"
        v-model="draft"
        class="md-typescale-body-medium composer__input"
        rows="1"
        :placeholder="placeholder"
        :aria-label="placeholder"
        @input="resize"
        @keydown="onKeydown"
        @paste="onPaste"
      />
      <MdIconButton
        icon="send"
        variant="filled"
        :label="i18n.t('chat.send')"
        :disabled="!canSend"
        @click="submit"
      />
    </div>

    <div v-if="tooLong || showCounter" class="composer__footer md-typescale-label-small">
      <span v-if="tooLong" class="composer__error">
        {{ i18n.t('chat.tooLong', { max: MAX_BODY_CHARS }) }}
      </span>
      <span v-else class="composer__counter">{{ draft.length }} / {{ MAX_BODY_CHARS }}</span>
    </div>

    <ImageViewer
      v-if="viewing !== null"
      :src="ipc.fileSrc(viewing.path)"
      :name="viewing.name"
      @close="viewing = null"
    />
  </footer>
</template>

<style scoped>
.composer {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 16px 12px;
  border-block-start: 1px solid var(--md-sys-color-outline-variant);
  background: var(--md-sys-color-surface);
}

/*
 * Covers the window rather than the composer: a drop is aimed at the conversation, and the
 * target the user sees is the half of the window the files will land in.
 */
.composer__drop {
  position: fixed;
  inset: 0;
  z-index: 20;
  display: flex;
  flex-direction: column;
  gap: 4px;
  align-items: center;
  justify-content: center;
  border: 2px dashed var(--md-sys-color-primary);
  border-radius: var(--md-sys-shape-corner-large);
  background: var(--md-sys-color-surface-container);
  color: var(--md-sys-color-on-surface);
  pointer-events: none;
}

.composer__files {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  margin: 0;
  padding: 0;
  list-style: none;
}

.composer__file {
  display: flex;
  gap: 8px;
  align-items: center;
  max-inline-size: 260px;
  padding: 4px 4px 4px 6px;
  border-radius: var(--md-sys-shape-corner-medium);
  background: var(--md-sys-color-surface-container-high);
}

.composer__thumb-button {
  display: inline-flex;
  flex: none;
  padding: 0;
  border: none;
  border-radius: var(--md-sys-shape-corner-small);
  background: none;
  cursor: zoom-in;
}

.composer__thumb-button:focus-visible {
  outline: 2px solid var(--md-sys-color-primary);
  outline-offset: 1px;
}

.composer__thumb {
  inline-size: 36px;
  block-size: 36px;
  flex: none;
  border-radius: var(--md-sys-shape-corner-small);
  object-fit: cover;
  background: var(--md-sys-color-surface-container-highest);
}

.composer__thumb--glyph {
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

/* The icon set ships its own 32 px canvas; the chip decides the box it lives in. */
.composer__thumb--glyph :deep(svg) {
  inline-size: 24px;
  block-size: 24px;
}

.composer__file-text {
  display: flex;
  flex-direction: column;
  min-inline-size: 0;
}

.composer__file-name {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.composer__file-size {
  color: var(--md-sys-color-on-surface-variant);
}

.composer__file-problem {
  color: var(--md-sys-color-error);
}

/*
 * A tonal banner rather than a line of grey text: the peer being away changes what sending
 * means, and the composer is where that has to be read. The container colour is the same tonal
 * surface the app's other secondary actions use, so it stands out from the field without
 * shouting like an error.
 */
.composer__offline {
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 8px 12px;
  border-radius: var(--md-sys-shape-corner-medium);
  background: var(--md-sys-color-secondary-container);
  color: var(--md-sys-color-on-secondary-container);
}

.composer__row {
  display: flex;
  gap: 8px;
  align-items: flex-end;
}

.composer__input {
  flex: 1;
  min-width: 0;
  min-height: 44px;
  max-height: var(--localme-composer-max-height);
  padding: 11px 16px;
  border: 1px solid transparent;
  border-radius: var(--md-sys-shape-corner-extra-large);
  background: var(--md-sys-color-surface-container-highest);
  color: var(--md-sys-color-on-surface);
  resize: none;
  overflow-y: auto;
  transition: border-color var(--md-sys-motion-duration-short3) var(--md-sys-motion-easing-standard);
}

.composer__input::placeholder {
  color: var(--md-sys-color-on-surface-variant);
}

.composer__input:focus-visible {
  border-color: var(--md-sys-color-primary);
  outline: none;
}

.composer__input:disabled {
  background: var(--md-sys-color-surface-container);
  color: var(--md-sys-color-on-surface-variant);
}

.composer__footer {
  display: flex;
  gap: 12px;
  align-items: baseline;
  justify-content: flex-end;
  min-height: 16px;
  padding-inline: 4px;
}

.composer__counter {
  color: var(--md-sys-color-on-surface-variant);
}

.composer__error {
  color: var(--md-sys-color-error);
}
</style>
