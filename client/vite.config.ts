import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// ESM-native, so the config loads under Vite's bundling and runner loaders alike.
const here = path.dirname(fileURLToPath(import.meta.url));

// The Codescape desktop client is its own Vite root. root, outDir and the
// alias are explicit so the config behaves the same from any cwd (the deploy
// entrypoint builds from a copy of this directory).
const isolationHeaders = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'require-corp',
};

export default defineConfig({
  root: here,
  // Never inside node_modules: in the deployed image node_modules is a
  // symlink to the host client's tree, which this build must not write to.
  cacheDir: process.env.CODESCAPE_VITE_CACHE ?? path.resolve(here, '.vite'),
  base: './',
  plugins: [react()],
  resolve: {
    alias: { '@cs': path.resolve(here, 'src') },
  },
  worker: { format: 'es' },
  build: {
    outDir: process.env.CODESCAPE_DIST ?? path.resolve(here, '../target/codescape-dist'),
    emptyOutDir: true,
    target: 'es2022',
    sourcemap: true,
    chunkSizeWarningLimit: 1500,
  },
  server: { headers: isolationHeaders, fs: { allow: [here] } },
  preview: { headers: isolationHeaders },
});
