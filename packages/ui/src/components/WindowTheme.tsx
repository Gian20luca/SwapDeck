import { useEffect } from 'react';
import { useBackend } from '../backend/context.js';
import { useTheme } from '../theme/index.js';

/**
 * Porta il tema della UI alla cornice nativa della finestra (barra del titolo): `'dark'` o `'light'`
 * se scelto, `null` per «auto» (la cornice segue il sistema). Non mostra nulla; senza
 * `setWindowTheme` nel backend (webview di VS Code) non fa niente.
 */
export function WindowTheme() {
  const backend = useBackend();
  const { mode } = useTheme();
  useEffect(() => {
    void backend.setWindowTheme?.(mode === 'auto' ? null : mode).catch(() => {});
  }, [backend, mode]);
  return null;
}
