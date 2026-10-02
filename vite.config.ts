import { fileURLToPath, URL } from 'node:url';

import vue from '@vitejs/plugin-vue';
import { defineConfig } from 'vitest/config';
import Icons from 'unplugin-icons/vite';

export default defineConfig({
  plugins: [
    vue(),
    // File-type icons are the one thing this application does not draw itself: there are a
    // hundred of them, they are keyed by file extension, and hand-drawing a hundred glyphs is
    // neither cheaper nor better. `unplugin-icons` resolves `~icons/<set>/<name>` at build time
    // and emits only the icons a module actually imports, so nothing is shipped that is not
    // shown. `compiler: 'vue3'` makes each one an ordinary single-file component: no runtime
    // dependency, no `v-html`, and the icons are static SVG in the bundle.
    Icons({ compiler: 'vue3' }),
  ],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  // Tauri's own logs share this terminal; do not wipe them on rebuild.
  clearScreen: false,
  server: {
    host: '127.0.0.1',
    port: 5173,
    strictPort: true,
    watch: {
      // The Rust side is rebuilt by cargo, not by Vite.
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    target: 'es2022',
    sourcemap: false,
    // The bundle is audited (see README); a warning threshold of 700 kB would never fire
    // silently, so it is lowered to make growth visible in CI.
    chunkSizeWarningLimit: 400,
    reportCompressedSize: true,
  },
  test: {
    environment: 'node',
    include: ['src/**/*.spec.ts'],
    restoreMocks: true,
  },
});
