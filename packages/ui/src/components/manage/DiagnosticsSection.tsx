import { useState } from 'react';
import { useBackend } from '../../backend/context.js';
import type { Backend } from '../../backend/types.js';
import { errorMessage } from '../../backend/useAccounts.js';
import type { Notice } from '../../backend/useActionRunner.js';
import { useI18n } from '../../i18n/index.js';
import { Tooltip } from '../Tooltip.js';
import { NoticeView, Section } from './common.js';

/** Appunti: prima l'host (VS Code), poi `navigator.clipboard`. Errore se nessuno dei due va. */
async function copy(backend: Backend, text: string): Promise<void> {
  if (backend.copyText) {
    await backend.copyText(text);
    return;
  }
  if (!navigator.clipboard) throw new Error('clipboard');
  await navigator.clipboard.writeText(text);
}

/** Strumenti → Diagnostica: cartella dei log e testo di diagnostica negli appunti. */
export function DiagnosticsSection() {
  const { t } = useI18n();
  const backend = useBackend();
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  // Testo mostrato per la copia a mano quando gli appunti non sono disponibili.
  const [manual, setManual] = useState<string | null>(null);

  if (!backend.getDiagnostics && !backend.openLogsFolder) return null;

  const copyDiagnostics = async () => {
    if (!backend.getDiagnostics) return;
    setBusy(true);
    setNotice(null);
    setManual(null);
    let text: string;
    try {
      text = await backend.getDiagnostics();
    } catch (error) {
      setNotice({ tone: 'danger', text: t('tools.diag.failed', { message: errorMessage(error) }) });
      setBusy(false);
      return;
    }
    try {
      await copy(backend, text);
      setNotice({ tone: 'info', text: t('tools.diag.copied') });
    } catch {
      setManual(text);
      setNotice({ tone: 'danger', text: t('tools.diag.copyFailed') });
    } finally {
      setBusy(false);
    }
  };

  const openLogs = async () => {
    if (!backend.openLogsFolder) return;
    setNotice(null);
    try {
      await backend.openLogsFolder();
    } catch (error) {
      setNotice({
        tone: 'danger',
        text: t('tools.diag.openFailed', { message: errorMessage(error) }),
      });
    }
  };

  return (
    <Section title={t('tools.diag')} hint={t('tools.diag.hint')} info={t('info.tools.diag')}>
      <NoticeView notice={notice} />
      <div className="sd-row">
        {backend.openLogsFolder && (
          <Tooltip text={t('tools.diag.openLogs.tip')}>
            <button type="button" className="sd-btn" onClick={() => void openLogs()}>
              {t('tools.diag.openLogs')}
            </button>
          </Tooltip>
        )}
        {backend.getDiagnostics && (
          <Tooltip text={t('tools.diag.copy.tip')}>
            <button
              type="button"
              className="sd-btn"
              disabled={busy}
              onClick={() => void copyDiagnostics()}
            >
              {t('tools.diag.copy')}
            </button>
          </Tooltip>
        )}
      </div>
      {manual !== null && (
        <textarea
          className="sd-input sd-diagnostics"
          aria-label={t('tools.diag.text')}
          readOnly
          rows={10}
          value={manual}
          onFocus={(e) => e.currentTarget.select()}
        />
      )}
    </Section>
  );
}
