/** Punto d'ingresso di `@swapdeck/ui`: componenti, backend, i18n e tema. Gli stili sono in `@swapdeck/ui/styles.css`. */

export { clampPercent } from './lib/clamp.js';

export { Dashboard, type DashboardProps } from './components/Dashboard.js';
export { MiniWidget } from './components/MiniWidget.js';
export { AccountCard, type AccountCardProps } from './components/AccountCard.js';
export { UsageBar, type UsageBarProps } from './components/UsageBar.js';
export { PaceIndicator, type PaceIndicatorProps } from './components/PaceIndicator.js';
export {
  QuickSwitch,
  type QuickStrategy,
  type QuickSwitchProps,
} from './components/QuickSwitch.js';
export { HistoryPanel, type HistoryPanelProps } from './components/HistoryPanel.js';
export { UsageChart, type ChartLine, type UsageChartProps } from './components/UsageChart.js';
export { ConfirmDialog, type ConfirmDialogProps } from './components/ConfirmDialog.js';
export { LanguageSelect, ThemeSelect } from './components/PreferenceControls.js';
export { SetupWizard, type SetupWizardProps } from './components/SetupWizard.js';
export { SetupRun, type SetupRunProps } from './components/SetupRun.js';
export { CswapUpdateNotice, type CswapUpdateNoticeProps } from './components/CswapUpdateNotice.js';
export {
  INSTALL_STEPS,
  UPGRADE_STEPS,
  reduceProgress,
  setupErrorKey,
  type RunState,
  type StepState,
} from './lib/setup.js';

export {
  BackendError,
  type ActionOutcome,
  type Backend,
  type HistoryPoint,
  type HistoryQuery,
  type Unsubscribe,
} from './backend/types.js';
export { actions } from './backend/actions.js';
export { BackendProvider, useBackend } from './backend/context.js';
export { useAccounts, type AccountsState } from './backend/useAccounts.js';

export {
  I18nProvider,
  LOCALES,
  resolveLocale,
  translate,
  useI18n,
  type Locale,
  type LocalePreference,
  type MessageKey,
  type Translate,
} from './i18n/index.js';
export {
  THEME_MODES,
  ThemeProvider,
  resolveTheme,
  useTheme,
  type ResolvedTheme,
  type ThemeMode,
} from './theme/index.js';
export {
  DEFAULT_THRESHOLDS,
  levelFor,
  shortEmail,
  type Level,
  type Thresholds,
} from './lib/format.js';
