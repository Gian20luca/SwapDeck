import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

// Anteprima di sviluppo (`pnpm --filter @swapdeck/ui dev`): la dashboard con il FakeBackend.
export default defineConfig({
  plugins: [react()],
  server: { port: 5174, strictPort: false },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./vitest.setup.ts'],
    css: false,
    exclude: ['e2e/**', 'node_modules/**'],
    coverage: {
      provider: 'v8',
      include: ['src/**/*.{ts,tsx}'],
      exclude: ['src/dev/**', 'src/**/*.test.{ts,tsx}', 'src/**/*.d.ts', 'src/index.ts'],
      reporter: ['text-summary', 'text'],
      thresholds: { lines: 80, functions: 80 },
    },
  },
});
