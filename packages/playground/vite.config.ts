import { defineConfig } from 'vite';

export default defineConfig({
  base: '/chordsketch/',
  build: {
    outDir: 'dist',
  },
  server: {
    fs: {
      allow: ['../..'],
    },
  },
});
