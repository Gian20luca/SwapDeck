import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

// Pagina dell'app Tauri: `pnpm dev` per `tauri dev` (porta fissa di `devUrl`), `pnpm build` in `dist/`.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 5175, strictPort: true, host: '127.0.0.1' },
  build: { outDir: 'dist', emptyOutDir: true, target: 'es2022' },
  test: {
    environment: 'jsdom',
    globals: true,
    css: false,
    setupFiles: ['./vitest.setup.ts'],
  },
});
