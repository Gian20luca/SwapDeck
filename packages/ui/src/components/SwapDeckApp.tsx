import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react';
import type { SetupDiagnosis } from '@swapdeck/types';
import { useBackend } from '../backend/context.js';
import { hasSetup } from '../backend/types.js';
import { useI18n, type MessageKey } from '../i18n/index.js';
import { CswapUpdateNotice } from './CswapUpdateNotice.js';
import { Dashboard } from './Dashboard.js';
import { NavProvider, type NavTarget, type TabId } from './navigation.js';
import { FoldersPage } from './manage/FoldersPage.js';
import { HistoryPage } from './manage/HistoryPage.js';
import { RulesPage } from './manage/RulesPage.js';
import { SettingsPage } from './manage/SettingsPage.js';
import { ToolsPage } from './manage/ToolsPage.js';
import { SetupWizard } from './SetupWizard.js';

export type { TabId };

const TABS: readonly { id: TabId; label: MessageKey }[] = [
  { id: 'accounts', label: 'tabs.accounts' },
  { id: 'folders', label: 'tabs.folders' },
  { id: 'rules', label: 'tabs.rules' },
  { id: 'history', label: 'tabs.history' },
  { id: 'tools', label: 'tabs.tools' },
  { id: 'settings', label: 'tabs.settings' },
];

export interface SwapDeckAppProps {
  initialTab?: TabId;
  /** Contenuto extra nella barra della dashboard (per esempio le preferenze dell'anteprima). */
  toolbarExtra?: ReactNode;
  /** Sopra le schede (per esempio le righe di stato del motore dell'app desktop). */
  header?: ReactNode;
}

/** La procedura di primo avvio serve se cswap manca o non è utilizzabile, o se non ci sono account. */
export function needsSetup(d: SetupDiagnosis): boolean {
  return (
    d.status === 'cswapMissing' ||
    d.status === 'unsupportedVersion' ||
    d.status === 'unsupportedSchema' ||
    (d.status === 'ready' && (d.accountCount ?? 0) === 0)
  );
}

/**
 * Interfaccia completa a schede, pensata per 420×640: account, cartelle, regole, storico, strumenti,
 * impostazioni. Con un host che offre `SetupBackend` fa prima la diagnosi: se cswap manca (o non è
 * utilizzabile, o non ci sono account) mostra `SetupWizard` al posto delle schede; poi l'avviso di
 * nuova versione sopra le schede. Durante la diagnosi e se non riesce si vedono subito le schede.
 */
export function SwapDeckApp(props: SwapDeckAppProps) {
  const backend = useBackend();
  const [setup, setSetup] = useState<SetupDiagnosis | null>(null);
  useEffect(() => {
    if (!hasSetup(backend)) return;
    let alive = true;
    backend.diagnoseSetup().then(
      (d) => {
        if (alive && needsSetup(d)) setSetup(d);
      },
      () => undefined,
    );
    return () => {
      alive = false;
    };
  }, [backend]);

  if (setup)
    return (
      <div className="sd-shell">
        {props.header}
        <div className="sd-shell__panel">
          <SetupWizard initial={setup} onReady={() => setSetup(null)} />
        </div>
      </div>
    );
  return <Tabs {...props} notice={hasSetup(backend) ? <CswapUpdateNotice /> : null} />;
}

function Tabs({
  initialTab = 'accounts',
  toolbarExtra,
  header,
  notice,
}: SwapDeckAppProps & { notice: ReactNode }) {
  const { t } = useI18n();
  const [tab, setTab] = useState<TabId>(initialTab);
  /** Account da aprire nella scheda Storico; `key` cambia a ogni richiesta, anche per lo stesso account. */
  const [historyTarget, setHistoryTarget] = useState<{ account: number; key: number } | null>(null);
  const navigate = useCallback((target: NavTarget) => {
    if (target.tab === 'history' && target.account !== undefined) {
      const account = target.account;
      setHistoryTarget((prev) => ({ account, key: (prev?.key ?? 0) + 1 }));
    }
    setTab(target.tab);
  }, []);
  const base = useId();
  const refs = useRef<Record<string, HTMLButtonElement | null>>({});

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const index = TABS.findIndex((x) => x.id === tab);
    let next = index;
    if (event.key === 'ArrowRight') next = (index + 1) % TABS.length;
    else if (event.key === 'ArrowLeft') next = (index - 1 + TABS.length) % TABS.length;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = TABS.length - 1;
    else return;
    event.preventDefault();
    const target = TABS[next]!;
    setTab(target.id);
    refs.current[target.id]?.focus();
  }

  return (
    <NavProvider value={navigate}>
      <div className="sd-shell">
        {header}
        {notice}
        <div className="sd-tabs" role="tablist" aria-label={t('tabs.label')} onKeyDown={onKeyDown}>
          {TABS.map(({ id, label }) => (
            <button
              key={id}
              ref={(node) => {
                refs.current[id] = node;
              }}
              type="button"
              role="tab"
              id={`${base}-tab-${id}`}
              aria-selected={tab === id}
              aria-controls={`${base}-panel-${id}`}
              tabIndex={tab === id ? 0 : -1}
              className="sd-tab"
              onClick={() => setTab(id)}
            >
              {t(label)}
            </button>
          ))}
        </div>
        <div
          role="tabpanel"
          id={`${base}-panel-${tab}`}
          aria-labelledby={`${base}-tab-${tab}`}
          className="sd-shell__panel"
        >
          {tab === 'accounts' && <Dashboard manage {...(toolbarExtra ? { toolbarExtra } : {})} />}
          {tab === 'folders' && <FoldersPage />}
          {tab === 'rules' && <RulesPage />}
          {tab === 'history' && <HistoryPage target={historyTarget} />}
          {tab === 'tools' && <ToolsPage />}
          {tab === 'settings' && <SettingsPage />}
        </div>
      </div>
    </NavProvider>
  );
}
