// @ts-check
import js from '@eslint/js';
import { defineConfig } from 'eslint/config';
import prettier from 'eslint-config-prettier';
import simpleImportSort from 'eslint-plugin-simple-import-sort';
import svelte from 'eslint-plugin-svelte';
import unusedImports from 'eslint-plugin-unused-imports';
import globals from 'globals';
import ts from 'typescript-eslint';

import svelteConfig from './svelte.config.js';

export default defineConfig(
  { ignores: ['dist/', 'src-tauri/', 'node_modules/'] },
  js.configs.recommended,
  ts.configs.recommended,
  svelte.configs.recommended,
  // Formatting is Prettier's job; switch off rules that would fight it.
  prettier,
  svelte.configs.prettier,
  {
    languageOptions: {
      parserOptions: { extraFileExtensions: ['.svelte'] },
      globals: { ...globals.browser },
    },
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts'],
    languageOptions: {
      parserOptions: { projectService: true, parser: ts.parser, svelteConfig },
    },
  },
  {
    files: ['*.config.{js,ts}'],
    languageOptions: { globals: { ...globals.node } },
  },
  {
    plugins: { 'simple-import-sort': simpleImportSort, 'unused-imports': unusedImports },
    rules: {
      'simple-import-sort/imports': 'error',
      'simple-import-sort/exports': 'error',
      'unused-imports/no-unused-imports': 'error',
      'unused-imports/no-unused-vars': 'error',
    },
  },
  {
    // The remote page runs in a browser on another device: no Tauri, and nothing of the app that needs it (window
    // handling, shortcuts with editing, files). It reaches the backend only through `lib/transport.ts`.
    files: ['src/remote/**'],
    ignores: ['src/remote/**/*.test.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            { group: ['@tauri-apps/*'], message: 'The remote page has no Tauri.' },
            {
              group: [
                '**/lib/windows.svelte',
                '**/lib/window.svelte',
                '**/lib/shortcuts',
                '**/lib/shows',
                '**/lib/media',
                '**/lib/fileDrop',
                '**/lib/commands',
              ],
              message: 'Needs the app window or edits the show; not for the remote page.',
            },
          ],
        },
      ],
    },
  },
);
