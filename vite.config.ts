import { fileURLToPath, URL } from 'node:url';

import vue from '@vitejs/plugin-vue';
import { defineConfig } from 'vitest/config';

// The front end is a single window rendered by the Tauri webview, so the build targets a
// known-modern engine: no polyfills, no legacy transpilation, no bundle of `core-js`.
export default defineConfig({
  plugins: [vue()],
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
