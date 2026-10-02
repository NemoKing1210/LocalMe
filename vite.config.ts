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
    setupFiles: ['./src/test/setup.ts'],
    server: {
      deps: {
        // `@material/material-color-utilities` ships one extensionless relative import
        // (`dynamiccolor/color_spec_2025.js` → `./dynamic_color`), which Node's ESM resolver
        // refuses when the package is externalised. Inlining it lets Vite resolve the specifier
        // the same way the application build does.
        inline: ['@material/material-color-utilities'],
      },
    },
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html', 'lcov'],
      reportsDirectory: 'coverage',
      // Every source file counts, not only the imported ones, so an untested module is visible
      // as a gap rather than absent from the report.
      include: ['src/**/*.{ts,vue}'],
      exclude: [
        'src/**/*.spec.ts',
        'src/**/*.d.ts',
        'src/test/**',
        'src/main.ts',
        'src/i18n/messages/**',
      ],
      // A gate, not a report: `npm run test:coverage` fails when a change lowers coverage past
      // these. They sit a little under what the suite actually reaches, so ordinary refactoring
      // does not fail the build for noise, while a new untested module does.
      thresholds: {
        statements: 93,
        branches: 85,
        functions: 94,
        lines: 95,
      },
    },
  },
});
