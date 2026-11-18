/// <reference types="vitest/config" />
import { fileURLToPath } from 'node:url';

import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { FileSystemIconLoader } from 'unplugin-icons/loaders';
import Icons from 'unplugin-icons/vite';
import { defineConfig } from 'vite';

const host = process.env.TAURI_DEV_HOST;

// https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
  plugins: [
    tailwindcss(),
    Icons({
      compiler: 'svelte',
      // Own icons in the Phosphor bold style (256 grid, 24 stroke, round): `~icons/jq/<name>`.
      customCollections: { jq: FileSystemIconLoader('./src/icons') },
    }),
    svelte(),
  ],
  // Keep Rust errors visible and use the fixed port Tauri expects.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || '127.0.0.1',
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  build: {
    target: 'es2022',
    chunkSizeWarningLimit: 1000,
    // The app's windows, and the remote page the app serves to other devices (`src/remote`).
    rolldownOptions: {
      input: {
        index: fileURLToPath(new URL('./index.html', import.meta.url)),
        remote: fileURLToPath(new URL('./remote.html', import.meta.url)),
      },
    },
  },
  // Svelte's browser build, so components can be mounted in tests (jsdom).
  resolve: {
    alias: { $lib: fileURLToPath(new URL('./src/lib', import.meta.url)) },
    conditions: process.env.VITEST ? ['browser'] : undefined,
  },
  test: {
    environment: 'jsdom',
    // Mocks the Tauri backend (`src/test/backend.ts`); tests run in English, whatever the system language is.
    setupFiles: ['src/lib/test-setup.ts'],
    // Component tests click through whole dialogs; instrumented for coverage and run side by side, some take over
    // the default 5 s.
    testTimeout: 30_000,
    coverage: {
      provider: 'v8',
      include: ['src/**/*.{ts,svelte}'],
      // shadcn-svelte components (generated), the language catalogs and the test helpers themselves.
      exclude: ['src/lib/components/ui/**', 'src/lib/i18n/{en,de}.ts', 'src/test/**', 'src/**/*.test.ts', '**/*.d.ts'],
      reporter: ['text-summary', 'html'],
    },
  },
});
