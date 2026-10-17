import '@testing-library/jest-dom/vitest';
import { cleanup, configure } from '@testing-library/react';
import { afterEach } from 'vitest';

// `findBy*` e `waitFor` aspettano 1 s di default: con la suite in parallelo e la macchina carica
// (build, altre suite) una scheda pesante come Impostazioni può metterci di più a ridisegnarsi, e
// il test falliva senza un errore vero. Il limite di ogni test resta `testTimeout` (20 s).
configure({ asyncUtilTimeout: 5000 });

afterEach(() => {
  cleanup();
});

// jsdom non ha matchMedia, che uPlot usa già al caricamento del modulo.
if (typeof window.matchMedia !== 'function') {
  window.matchMedia = ((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => false,
  })) as unknown as typeof window.matchMedia;
}
