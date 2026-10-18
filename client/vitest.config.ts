import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

// ESM-native, so the config loads under Vite's bundling and runner loaders alike.
const here = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  root: here,
  // Never inside node_modules: in the deployed image node_modules is a
  // symlink to the host client's tree, which this build must not write to.
  cacheDir: process.env.CODESCAPE_VITE_CACHE ?? path.resolve(here, '.vite'),
  resolve: {
    alias: { '@cs': path.resolve(here, 'src') },
  },
  test: {
    environment: 'jsdom',
    globals: false,
    env: { NODE_ENV: 'test' },
    include: ['src/**/*.test.ts', 'src/**/*.test.tsx'],
  },
});
