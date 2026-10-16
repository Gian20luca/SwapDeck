//! Comandi `invoke` della UI. Le chiamate a cswap girano fuori dal thread principale.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use swapdeck_core::cswap::{
    ConfigGetResponse, ConfigListResponse, Confirmation, CswapAction, CswapError, CswapErrorKind,
    ListResponse, StatusResponse,
};
use swapdeck_core::history::{DEFAULT_RETENTION_DAYS, History, HistoryRecord};
use swapdeck_core::timeutil::{now_unix, parse_iso};
use tauri::{AppHandle, State};

use crate::api::{Outcome, Res};
use crate::app;
use crate::poller::REFRESH_WAIT;
use crate::state::{Shared, Snapshot, lock, now_ms};
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

/// Intervallo di default dello storico (come la UI): 7 giorni.
const HISTORY_DEFAULT_SECS: i64 = 7 * 86_400;
/// Punti per account oltre i quali se ne tiene uno per intervallo (grafici leggeri).
const HISTORY_MAX_POINTS: i64 = 1_500;
const FIVE_HOURS_SECS: i64 = 5 * 3_600;

/// Consumo atteso al ritmo lineare della finestra di 5 ore che si chiude a `resets_at`.
fn five_hour_expected(ts: i64, resets_at: Option<&str>) -> Option<f64> {
    let reset = parse_iso(resets_at?)?;
    let elapsed = ts - (reset - FIVE_HOURS_SECS);
    (0..=FIVE_HOURS_SECS)
        .contains(&elapsed)
        .then(|| elapsed as f64 * 100.0 / FIVE_HOURS_SECS as f64)
}

/// Campioni dello storico in punti per la UI: filtro per account, ordine per istante e al
/// massimo un punto per account ogni `bucket_secs`.
pub fn history_points(
    records: &[HistoryRecord],
    account: Option<u32>,
    bucket_secs: i64,
) -> Vec<HistoryPoint> {
    let mut samples: Vec<_> = records
        .iter()
        .filter_map(|r| match r {
            HistoryRecord::Sample(s) if account.is_none_or(|a| a == s.account) => Some(s),
            _ => None,
        })
        .collect();
    samples.sort_by_key(|s| s.ts);
    let bucket = bucket_secs.max(1);
    let mut last_bucket = std::collections::HashMap::new();
    samples
        .into_iter()
        .filter(|s| {
            last_bucket.insert(s.account, s.ts.div_euclid(bucket)) != Some(s.ts.div_euclid(bucket))
        })
        .map(|s| HistoryPoint {
            ts: s.ts,
            account: s.account,
            five_hour_pct: s.five_hour_pct,
            seven_day_pct: s.seven_day_pct,
            five_hour_expected_pct: five_hour_expected(s.ts, s.five_hour_resets_at.as_deref()),
            seven_day_expected_pct: s.seven_day_expected_pct,
        })
        .collect()
}

/// Storico dei consumi dalla cartella dati (scritto da chi tiene il lock del motore).
#[tauri::command]
pub async fn get_history(
    data: State<'_, app::DataDir>,
    query: Option<HistoryQuery>,
) -> Res<Vec<HistoryPoint>> {
    let Some(dir) = data.0.clone() else {
        return Ok(Vec::new());
    };
    let q = query.unwrap_or_default();
    blocking(move || {
        let now = now_unix();
        let oldest = now - i64::from(DEFAULT_RETENTION_DAYS) * 86_400;
        let from = q
            .since_seconds
            .unwrap_or(now - HISTORY_DEFAULT_SECS)
            .clamp(oldest, now);
        let records = History::new(&dir)
            .read_range(from, now)
            .map_err(|e| CswapError::new(CswapErrorKind::Failed, format!("storico: {e}")))?;
        let bucket = ((now - from) / HISTORY_MAX_POINTS).max(60);
        Ok(history_points(&records, q.account, bucket))
    })
    .await
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
    let auto = match &action {
        CswapAction::AutoStart { .. } => Some(true),
        CswapAction::AutoStop => Some(false),
        _ => None,
    };
    let outcome = blocking(move || api.execute(&action)).await?;
    if let Some(running) = auto {
        app::set_auto_running(&app, running);
    }
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
        autostart_managed: crate::autostart::managed(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}

#[tauri::command]
pub fn set_autostart(
    app: AppHandle,
    shared: State<'_, Arc<Shared>>,
    enabled: bool,
) -> Res<AppSettingsView> {
    {
        let mut s = lock(&shared.settings);
        s.autostart = enabled;
        s.autostart_saved = true;
    }
    let failed = |m: String| CswapError::new(CswapErrorKind::Failed, m);
    shared.save_settings().map_err(failed)?;
    crate::autostart::apply(&app, enabled).map_err(failed)?;
    Ok(get_app_settings(shared))
}

/// Ultimo stato (lo stesso dell'evento `state-changed`), per chi apre una finestra dopo l'invio.
#[tauri::command]
pub fn get_snapshot(shared: State<'_, Arc<Shared>>) -> Snapshot {
    lock(&shared.snapshot).clone()
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
        Shared::new(
            Arc::new(MockApi::new()),
            SettingsStore::in_memory(),
            Settings::default(),
        )
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

    #[test]
    fn history_points_filter_sort_and_thin_out() {
        use swapdeck_core::history::UsageSample;
        use swapdeck_core::timeutil::format_iso;
        let list = fixture_list();
        let a = &list.accounts[0];
        let sample = |ts: i64| {
            let mut s = UsageSample::from_account(a, ts);
            s.five_hour_pct = Some(10.0);
            s.five_hour_resets_at = Some(format_iso(3_600 * 5));
            HistoryRecord::Sample(s)
        };
        let other = HistoryRecord::Sample(UsageSample::from_account(&list.accounts[1], 0));
        let records = vec![sample(9_000), sample(0), sample(30), other, sample(120)];
        let points = history_points(&records, Some(a.number), 60);
        assert_eq!(
            points.iter().map(|p| p.ts).collect::<Vec<_>>(),
            [0, 120, 9_000]
        );
        assert_eq!(points[0].five_hour_expected_pct, Some(0.0));
        assert_eq!(points[2].five_hour_expected_pct, Some(50.0));
        assert_eq!(history_points(&records, None, 1).len(), 5);
        assert_eq!(five_hour_expected(0, Some("non una data")), None);
        assert_eq!(five_hour_expected(20_000, Some(&format_iso(18_000))), None);
    }
}
