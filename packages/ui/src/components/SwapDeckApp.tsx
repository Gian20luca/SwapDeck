import { useId, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { useI18n, type MessageKey } from '../i18n/index.js';
import { Dashboard } from './Dashboard.js';
import { FoldersPage } from './manage/FoldersPage.js';
import { HistoryPage } from './manage/HistoryPage.js';
import { RulesPage } from './manage/RulesPage.js';
import { SettingsPage } from './manage/SettingsPage.js';
import { ToolsPage } from './manage/ToolsPage.js';

export type TabId = 'accounts' | 'folders' | 'rules' | 'history' | 'tools' | 'settings';

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

/** Interfaccia completa a schede, pensata per 420×640: account, cartelle, regole, storico, strumenti, impostazioni. */
export function SwapDeckApp({ initialTab = 'accounts', toolbarExtra, header }: SwapDeckAppProps) {
  const { t } = useI18n();
  const [tab, setTab] = useState<TabId>(initialTab);
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
    <div className="sd-shell">
      {header}
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
        {tab === 'history' && <HistoryPage />}
        {tab === 'tools' && <ToolsPage />}
        {tab === 'settings' && <SettingsPage />}
      </div>
    </div>
  );
}
