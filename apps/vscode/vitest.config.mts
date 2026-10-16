import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    coverage: {
      provider: 'v8',
      include: ['src/**/*.{ts,tsx}'],
      // `generated/` sono tipi, `testing.ts` fixture. `extension.ts`, `panel.ts` e `webview/main.tsx`
      // usano l'API di VS Code o il DOM del webview: li prova `test:ext` in un VS Code isolato.
      exclude: [
        'src/**/*.test.ts',
        'src/generated/**',
        'src/testing.ts',
        'src/**/*.d.ts',
        'src/extension.ts',
        'src/panel.ts',
        'src/webview/main.tsx',
      ],
      reporter: ['text-summary', 'text'],
      thresholds: { lines: 70 },
    },
  },
});
