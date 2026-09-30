<script setup lang="ts">
/**
 * A peer's avatar.
 *
 * The drawing is generated from the seed the *peer* announced, never from anything derived
 * locally: two machines must render the same peer pixel-for-pixel, and only the owner's own
 * announcement is authoritative. This is why the seed is a required prop — there is no
 * "fall back to the nickname" here, because that fallback would be wrong on one of the two
 * machines and silently so.
 *
 * `blobatar/uri` is used rather than the package's Vue adapter. The adapter renders inline SVG
 * carrying an idle animation, and one always-running animation per row is exactly the
 * per-frame work a virtualised list exists to avoid; a `data:` URI in an `<img>` is decoded
 * once and then composited by the browser with no per-frame cost.
 *
 * Generated URIs are memoised in a module-level cache rather than a per-instance one: the same
 * peer is drawn in the user list, the chat header and a dialog, and a virtualised row that
 * scrolls out of view and back must not regenerate its avatar either. The cache is bounded and
 * emptied wholesale when it fills — a true LRU would carry per-read bookkeeping for an entry
 * whose regeneration costs a hash and a few hundred bytes of traits, which is the wrong trade.
 *
 * The image is decorative: the nickname always sits next to it, so an alternative text there
 * would only make a screen reader say the name twice. The wrapper is `aria-hidden`, and the
 * nickname is kept as the native tooltip for the places where the text is truncated.
 */
import { computed } from 'vue';

import { blobatarUri } from 'blobatar/uri';

const props = withDefaults(
  defineProps<{
    /** The seed announced by the peer: the identity of the drawing. */
    seed: string;
    /** The peer's nickname, used for the hover tooltip. */
    name: string;
    /** Box size in pixels. */
    size?: number;
    /** The offline look: desaturated and reduced in opacity. */
    dimmed?: boolean;
    /** Which presence dot to draw, or `null` for none. */
    presence?: 'online' | 'offline' | null;
  }>(),
  { size: 40, dimmed: false, presence: null },
);

/** Entries kept in the module cache before it is emptied. */
const CACHE_LIMIT = 400;
const cache = new Map<string, string>();

function avatarUri(seed: string, size: number): string {
  const key = `${seed}|${size}`;
  const memoised = cache.get(key);
  if (memoised !== undefined) return memoised;

  if (cache.size >= CACHE_LIMIT) cache.clear();
  const uri = blobatarUri(seed, { background: 'squircle', size });
  cache.set(key, uri);
  return uri;
}

const uri = computed(() => avatarUri(props.seed, props.size));
const box = computed(() => `${props.size}px`);
/** The dot tracks the avatar so a 24px avatar in a message row is not dwarfed by its marker. */
const dot = computed(() => `${Math.max(8, Math.round(props.size / 3.2))}px`);
</script>

<template>
  <span
    class="md-avatar"
    :class="{ 'md-avatar--dimmed': dimmed }"
    :style="{ width: box, height: box }"
    :title="name"
    aria-hidden="true"
  >
    <img class="md-avatar__image" :src="uri" alt="" />
    <span
      v-if="presence !== null"
      class="md-avatar__presence"
      :class="`md-avatar__presence--${presence}`"
      :style="{ width: dot, height: dot }"
    />
  </span>
</template>

<style scoped>
.md-avatar {
  position: relative;
  display: block;
  flex: none;
  border-radius: var(--md-sys-shape-corner-full);
}

.md-avatar__image {
  width: 100%;
  height: 100%;
  object-fit: cover;
  border-radius: var(--md-sys-shape-corner-full);
}

/* Desaturation carries "not here" even to someone who cannot read the dimming. */
.md-avatar--dimmed .md-avatar__image {
  filter: grayscale(1);
  opacity: var(--md-sys-state-disabled-opacity);
}

.md-avatar__presence {
  position: absolute;
  inset-block-end: 0;
  inset-inline-end: 0;
  border: 2px solid var(--md-sys-color-surface);
  border-radius: var(--md-sys-shape-corner-full);
  /* Presence changes under the user's eyes — a peer going offline mid-conversation — so the dot
     recolours rather than flicking between two colours on the next frame. */
  transition: background-color var(--md-sys-motion-duration-medium1)
    var(--md-sys-motion-easing-standard);
}

.md-avatar__presence--online {
  background: var(--md-sys-color-primary);
}

.md-avatar__presence--offline {
  background: var(--md-sys-color-outline);
}
</style>
