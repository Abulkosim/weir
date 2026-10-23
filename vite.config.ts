import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';

export default defineConfig({
  base: './',
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1430,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**', '**/.cache/**', '**/.toolchain/**'] },
  },
  build: { target: ['es2022', 'safari15'] },
});
