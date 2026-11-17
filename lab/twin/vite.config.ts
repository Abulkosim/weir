import { defineConfig } from 'vite';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  root: fileURLToPath(new URL('.', import.meta.url)),
  base: './',
  clearScreen: false,
  build: { outDir: fileURLToPath(new URL('../../lab-out/twin', import.meta.url)), emptyOutDir: true, target: 'es2022' },
});
