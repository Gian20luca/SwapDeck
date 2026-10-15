# Mappa del codice

Struttura iniziale (step 0). Ogni step aggiorna la sezione che tocca.

## Radice

| File | Ruolo |
|---|---|
| `package.json` | script `build`, `test`, `lint`, `format`, `test:rust`, `lint:rust` |
| `pnpm-workspace.yaml` | pacchetti, catalogo delle versioni, override di Vite, script di installazione ammessi |
| `tsconfig.base.json` | opzioni TypeScript strict condivise |
| `.prettierrc.json`, `.prettierignore` | formattazione (esclusi `docs/` e `fixtures/`) |
| `Cargo.toml` | workspace Rust, edizione 2024, lint condivisi (`unsafe_code` vietato) |

## Rust

| Crate | Percorso | Contenuto |
|---|---|---|
| `swapdeck-core` | `crates/swapdeck-core` | `version()`; logica dallo step 1 |
| `swapdeck-desktop` (bin `swapdeck`) | `apps/desktop/src-tauri` | segnaposto; Tauri 2 dallo step 4 |

## TypeScript

Ogni pacchetto ha `src/index.ts`, test `src/*.test.ts` (vitest), `tsconfig.json` (typecheck, test inclusi)
e `tsconfig.build.json` (emissione in `dist/`, test esclusi).

| Pacchetto | Percorso | Contenuto |
|---|---|---|
| `@swapdeck/types` | `packages/types` | `SCHEMA_VERSION`; tipi ts-rs dallo step 1 |
| `@swapdeck/ui` | `packages/ui` | React 19 + Vite 6: dashboard, mini-widget, grafici uPlot, i18n, tema (vedi sotto) |
| `swapdeck-vscode` | `apps/vscode` | `swapdeckExecutable`; estensione dallo step 7 |

### `packages/ui` (step 3)

Esporta i sorgenti (`src/index.ts`), non una build: i consumatori (Tauri, webview VS Code) li compilano con Vite. Stili in `@swapdeck/ui/styles.css`; `@swapdeck/ui/fake` espone il FakeBackend (fuori dal bundle di produzione). Anteprima: `pnpm --filter @swapdeck/ui dev`.

| Percorso (`src/`) | Ruolo |
|---|---|
| `backend/types.ts` | interfaccia `Backend` (async, `onStateChanged`), `HistoryPoint`, `BackendError`; `actions.ts` costruisce le `CswapAction`; `context.tsx`, `useAccounts.ts` |
| `dev/` | `FakeBackend` (fixture di `fixtures/cswap/` con istanti riallineati a «ora», cambi simulati), `history.ts` (storico sintetico di 7 giorni), `main.tsx` (anteprima) |
| `components/` | `Dashboard`, `AccountCard`, `UsageBar`, `PaceIndicator`, `QuickSwitch`, `HistoryPanel`, `UsageChart` (uPlot), `MiniWidget`, `ConfirmDialog` (anche a doppia conferma con parola), `PreferenceControls` |
| `i18n/` | dizionari `it.ts` (fonte delle chiavi) ed `en.ts`, `I18nProvider`, `useI18n` |
| `theme/`, `styles/index.css` | `ThemeProvider` (chiaro/scuro/auto via `data-theme`), variabili CSS, test di contrasto AA |
| `lib/` | soglie e formattazione (`format.ts`), helper degli account (`account.ts`), `useNow` |
