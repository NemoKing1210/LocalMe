<script setup lang="ts">
// The title row is exactly `--localme-top-bar-height` tall and the default slot lays out below it,
// so a search field's arrival does not reflow the title and the list underneath does not jump.
// The `| undefined` is what lets the `undefined` default typecheck under
// `exactOptionalPropertyTypes`; the prop's type is the same either way.
withDefaults(
  defineProps<{
    title: string;
    subtitle?: string | undefined;
  }>(),
  { subtitle: undefined },
);
</script>

<template>
  <header class="md-top-app-bar">
    <div class="md-top-app-bar__row">
      <div v-if="$slots.leading" class="md-top-app-bar__leading">
        <slot name="leading" />
      </div>
      <div class="md-top-app-bar__titles">
        <span class="md-typescale-title-large md-top-app-bar__title">{{ title }}</span>
        <span v-if="subtitle" class="md-typescale-label-medium md-top-app-bar__subtitle">
          {{ subtitle }}
        </span>
      </div>
      <div v-if="$slots.trailing" class="md-top-app-bar__trailing">
        <slot name="trailing" />
      </div>
    </div>
    <div v-if="$slots.default" class="md-top-app-bar__extra">
      <slot />
    </div>
  </header>
</template>

<style scoped>
.md-top-app-bar {
  position: sticky;
  inset-block-start: 0;
  z-index: 2;
  display: flex;
  flex-direction: column;
  background: var(--md-sys-color-surface);
}

.md-top-app-bar__row {
  display: flex;
  flex: none;
  gap: 4px;
  align-items: center;
  height: var(--localme-top-bar-height);
  padding-inline: 4px;
}

.md-top-app-bar__leading,
.md-top-app-bar__trailing {
  display: flex;
  flex: none;
  gap: 4px;
  align-items: center;
}

.md-top-app-bar__titles {
  display: flex;
  flex: 1;
  flex-direction: column;
  justify-content: center;
  min-width: 0;
  padding-inline: 8px;
}

.md-top-app-bar__title {
  overflow: hidden;
  color: var(--md-sys-color-on-surface);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.md-top-app-bar__subtitle {
  overflow: hidden;
  color: var(--md-sys-color-on-surface-variant);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.md-top-app-bar__extra {
  flex: none;
  padding: 0 16px 12px;
}
</style>
