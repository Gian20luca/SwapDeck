//! Comandi `invoke` della UI. Le chiamate a cswap girano fuori dal thread principale.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use swapdeck_core::cswap::{
    ConfigGetResponse, ConfigListResponse, Confirmation, CswapAction, CswapError, CswapErrorKind,
    ListResponse, StatusResponse,
};
use tauri::{AppHandle, State};

use crate::api::{Outcome, Res};
use crate::app;
use crate::poller::REFRESH_WAIT;
use crate::state::{Shared, lock, now_ms};
use crate::windows;

/// Un aggiornamento a richiesta più vicino di così all'ultimo riusa la lettura appena fatta.
const MIN_REFRESH_GAP: Duration = Duration::from_secs(5);

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Res<T> + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .unwrap_or_else(|e| Err(CswapError::new(CswapErrorKind::Failed, e.to_string())))
}

/// Lista da restituire senza nuova lettura, se c'è e (con `refresh`) è abbastanza recente.
pub fn cached_list(shared: &Shared, refresh: bool, now: u64) -> Option<ListResponse> {
    let snap = lock(&shared.snapshot);
    let fresh = now.saturating_sub(snap.updated_at_ms) < MIN_REFRESH_GAP.as_millis() as u64;
    match (&snap.list, refresh) {
        (Some(list), false) => Some(list.clone()),
        (Some(list), true) if fresh && snap.error.is_none() => Some(list.clone()),
        _ => None,
    }
}

#[tauri::command]
pub async fn list_accounts(
    app: AppHandle,
    shared: State<'_, Arc<Shared>>,
    refresh: Option<bool>,
) -> Res<ListResponse> {
    if let Some(list) = cached_list(&shared, refresh.unwrap_or(false), now_ms()) {
        return Ok(list);
    }
    let poller = app::poller(&app);
    blocking(move || poller.refresh_wait(REFRESH_WAIT)).await
}

#[tauri::command]
pub async fn get_status(shared: State<'_, Arc<Shared>>) -> Res<StatusResponse> {
    let api = Arc::clone(&shared.api);
    blocking(move || api.status()).await
}

#[tauri::command]
pub async fn get_config(shared: State<'_, Arc<Shared>>) -> Res<ConfigListResponse> {
    let api = Arc::clone(&shared.api);
    blocking(move || api.config_list()).await
}

#[tauri::command]
pub async fn get_config_value(
    shared: State<'_, Arc<Shared>>,
    key: String,
) -> Res<ConfigGetResponse> {
    let api = Arc::clone(&shared.api);
    blocking(move || api.config_get(&key)).await
}

/// Filtro dello storico (`HistoryQuery` della UI).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // letti quando lo storico del core (step 2) è collegato
pub struct HistoryQuery {
    pub account: Option<u32>,
    pub since_seconds: Option<i64>,
}

/// Punto dello storico (`HistoryPoint` della UI).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPoint {
    pub ts: i64,
    pub account: u32,
    pub five_hour_pct: Option<f64>,
    pub seven_day_pct: Option<f64>,
    pub five_hour_expected_pct: Option<f64>,
    pub seven_day_expected_pct: Option<f64>,
}

/// Storico dei consumi: vuoto finché lo storico del core (step 2) non è collegato.
#[tauri::command]
pub async fn get_history(query: Option<HistoryQuery>) -> Res<Vec<HistoryPoint>> {
    let _ = query.unwrap_or_default();
    Ok(Vec::new())
}

#[tauri::command]
pub fn confirmation_for(shared: State<'_, Arc<Shared>>, action: CswapAction) -> Confirmation {
    action.confirmation(lock(&shared.snapshot).active())
}

#[tauri::command]
pub async fn run_action(
    app: AppHandle,
    shared: State<'_, Arc<Shared>>,
    action: CswapAction,
) -> Res<Outcome> {
    let api = Arc::clone(&shared.api);
    let outcome = blocking(move || api.execute(&action)).await?;
    app::apply_outcome(&app, &outcome);
    if let Some(cmd) = &outcome.command {
        crate::terminal::open(cmd)?;
    }
    Ok(outcome)
}

/// Impostazioni dell'app mostrate nella UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsView {
    pub autostart: bool,
    /// `false` nelle build di sviluppo: la scelta si salva ma il sistema non viene toccato.
    pub autostart_managed: bool,
    pub version: String,
}

#[tauri::command]
pub fn get_app_settings(shared: State<'_, Arc<Shared>>) -> AppSettingsView {
    AppSettingsView {
        autostart: lock(&shared.settings).autostart,
        autostart_managed: app::autostart_managed(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}

#[tauri::command]
pub fn set_autostart(
    app: AppHandle,
    shared: State<'_, Arc<Shared>>,
    enabled: bool,
) -> Res<AppSettingsView> {
    lock(&shared.settings).autostart = enabled;
    let failed = |m: String| CswapError::new(CswapErrorKind::Failed, m);
    shared.save_settings().map_err(failed)?;
    app::apply_autostart(&app, enabled).map_err(failed)?;
    Ok(get_app_settings(shared))
}

#[tauri::command]
pub fn show_main(app: AppHandle) {
    windows::show_main(&app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mock::{MockApi, fixture_list};
    use crate::settings::{Settings, SettingsStore};

    fn shared() -> Shared {
        Shared::new(Arc::new(MockApi::new()), SettingsStore, Settings::default())
    }

    #[test]
    fn cache_rules() {
        let s = shared();
        assert!(
            cached_list(&s, false, 1_000).is_none(),
            "nessuna lettura ancora"
        );
        lock(&s.snapshot).apply(&Ok(fixture_list()), 1_000);
        assert!(
            cached_list(&s, false, 999_999).is_some(),
            "senza refresh: ultima lista"
        );
        assert!(
            cached_list(&s, true, 3_000).is_some(),
            "refresh ravvicinato: riuso"
        );
        assert!(
            cached_list(&s, true, 7_000).is_none(),
            "refresh: nuova lettura"
        );
        lock(&s.snapshot).apply(
            &Err(CswapError::new(CswapErrorKind::Timeout, "lento")),
            8_000,
        );
        assert!(
            cached_list(&s, true, 9_000).is_none(),
            "dopo un errore si rilegge"
        );
    }

    #[test]
    fn confirmation_uses_active_account() {
        let s = shared();
        lock(&s.snapshot).apply(&Ok(fixture_list()), 1);
        let active = lock(&s.snapshot).active();
        let disable_active: CswapAction =
            serde_json::from_str(r#"{"kind":"disable","account":6}"#).unwrap();
        assert_eq!(disable_active.confirmation(active), Confirmation::Simple);
        let disable_other: CswapAction =
            serde_json::from_str(r#"{"kind":"disable","account":3}"#).unwrap();
        assert_eq!(disable_other.confirmation(active), Confirmation::None);
    }

    #[test]
    fn history_query_and_point_shapes() {
        let q: HistoryQuery = serde_json::from_str(r#"{"account":3,"sinceSeconds":10}"#).unwrap();
        assert_eq!((q.account, q.since_seconds), (Some(3), Some(10)));
        let p = HistoryPoint {
            ts: 1,
            account: 3,
            five_hour_pct: Some(2.0),
            seven_day_pct: None,
            five_hour_expected_pct: None,
            seven_day_expected_pct: None,
        };
        let v = serde_json::to_value(p).unwrap();
        assert!(v.get("fiveHourExpectedPct").is_some());
    }

    #[test]
    fn outcome_serializes_for_the_ui() {
        let api = MockApi::new();
        let action: CswapAction = serde_json::from_str(r#"{"kind":"switch","target":3}"#).unwrap();
        let out = crate::api::CswapApi::execute(&api, &action).unwrap();
        let v = serde_json::to_value(&out).unwrap();
        assert_eq!(v["message"], "ok");
        assert!(v.get("command").is_none());
        assert_eq!(api.actions.lock().unwrap().len(), 1);
    }
}
