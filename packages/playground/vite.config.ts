import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';

const here = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  base: '/chordsketch/',
  build: {
    outDir: 'dist',
  },
  resolve: {
    alias: {
      // Decouple the playground source from the npm package's internal
      // layout. The deep relative path `../../npm/web/chordsketch_wasm.js`
      // had to move whenever the npm package's directory structure
      // changed (#1026 dual-package layout broke deploy-playground.yml
      // before #1061 was filed). Importing under the published name lets
      // future layout changes only touch the alias here. See #1057.
      '@chordsketch/wasm': resolve(here, '../npm/web/chordsketch_wasm.js'),
    },
  },
  server: {
    fs: {
      allow: ['../npm'],
    },
  },
});
