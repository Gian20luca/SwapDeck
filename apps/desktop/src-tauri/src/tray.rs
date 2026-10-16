//! Icona nell'area di notifica: anello colorato, tooltip e menu con il cambio rapido.

use std::sync::Arc;

use swapdeck_core::cswap::{CswapAction, ListResponse};
use swapdeck_core::engine::{EngineProfiles, ProfileCommand};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::state::{Shared, Snapshot, lock};
use crate::text::{self, Lang};
use crate::{app, ring, windows};

pub const TRAY_ID: &str = "swapdeck";
const ACCOUNT_PREFIX: &str = "acct:";
const MAP_PREFIX: &str = "map:";

fn image(pct: Option<f64>, paused: bool) -> Image<'static> {
    let r = ring::render(pct, paused);
    Image::new_owned(r.data, r.width, r.height)
}

/// Voce di un account nel menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountEntry {
    pub id: String,
    pub label: String,
    pub checked: bool,
    pub enabled: bool,
}

/// Voci degli account, nell'ordine di cswap.
pub fn account_entries(list: &ListResponse) -> Vec<AccountEntry> {
    list.accounts
        .iter()
        .map(|a| AccountEntry {
            id: format!("{ACCOUNT_PREFIX}{}", a.number),
            label: text::account_label(a),
            checked: a.active,
            enabled: a.disabled != Some(true),
        })
        .collect()
}

/// Numero dell'account da un id del menu.
pub fn account_from_id(id: &str) -> Option<u32> {
    id.strip_prefix(ACCOUNT_PREFIX)?.parse().ok()
}

/// Voce di una mappa nel sottomenu «Mappa».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapEntry {
    pub id: String,
    pub label: String,
    pub checked: bool,
}

/// Voci delle mappe, nell'ordine salvato, con quella attiva spuntata.
pub fn map_entries(profiles: &EngineProfiles) -> Vec<MapEntry> {
    profiles
        .profiles
        .iter()
        .map(|p| MapEntry {
            id: format!("{MAP_PREFIX}{}", p.id),
            label: p.name.clone(),
            checked: p.id == profiles.active_id,
        })
        .collect()
}

/// Id della mappa da un id del menu.
pub fn map_from_id(id: &str) -> Option<&str> {
    id.strip_prefix(MAP_PREFIX).filter(|m| !m.is_empty())
}

fn signature(
    list: Option<&ListResponse>,
    maps: &[MapEntry],
    mini_open: bool,
    lang: Lang,
) -> String {
    let entries = list.map(account_entries).unwrap_or_default();
    format!("{lang:?}|{mini_open}|{entries:?}|{maps:?}")
}

fn build_menu(
    app: &AppHandle,
    list: Option<&ListResponse>,
    maps: &[MapEntry],
    lang: Lang,
) -> tauri::Result<Menu<Wry>> {
    let l = text::labels(lang);
    let mut items: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    match list.map(account_entries) {
        Some(entries) if !entries.is_empty() => {
            for e in entries {
                items.push(Box::new(CheckMenuItem::with_id(
                    app,
                    e.id,
                    e.label,
                    e.enabled,
                    e.checked,
                    None::<&str>,
                )?));
            }
        }
        _ => items.push(Box::new(MenuItem::with_id(
            app,
            "none",
            l.no_accounts,
            false,
            None::<&str>,
        )?)),
    }
    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    if !maps.is_empty() {
        let mut checks = Vec::with_capacity(maps.len());
        for m in maps {
            checks.push(CheckMenuItem::with_id(
                app,
                m.id.clone(),
                m.label.clone(),
                true,
                m.checked,
                None::<&str>,
            )?);
        }
        let refs: Vec<&dyn IsMenuItem<Wry>> =
            checks.iter().map(|c| c as &dyn IsMenuItem<Wry>).collect();
        items.push(Box::new(Submenu::with_items(app, l.map, true, &refs)?));
    }
    items.push(Box::new(MenuItem::with_id(
        app,
        "open",
        l.open,
        true,
        None::<&str>,
    )?));
    items.push(Box::new(CheckMenuItem::with_id(
        app,
        "mini",
        l.mini,
        true,
        windows::mini_open(app),
        None::<&str>,
    )?));
    items.push(Box::new(MenuItem::with_id(
        app,
        "refresh",
        l.refresh,
        true,
        None::<&str>,
    )?));
    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(MenuItem::with_id(
        app,
        "quit",
        l.quit,
        true,
        None::<&str>,
    )?));
    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|b| b.as_ref()).collect();
    Menu::with_items(app, &refs)
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let shared = app.state::<Arc<Shared>>();
    let (lang, maps) = {
        let settings = lock(&shared.settings);
        (settings.lang, map_entries(&settings.engine_profiles))
    };
    let menu = build_menu(app, None, &maps, lang)?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(image(None, false))
        .tooltip("SwapDeck")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                windows::show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Aggiorna icona, tooltip, titolo (macOS) e, se è cambiato, il menu.
pub fn update(app: &AppHandle, snap: &Snapshot) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let lang = lock(&app.state::<Arc<Shared>>().settings).lang;
    let pct = snap.list.as_ref().and_then(text::active_five_hour);
    let _ = tray.set_icon(Some(image(pct, snap.engine_suspended)));
    let mut tip = text::tooltip(lang, snap.list.as_ref(), snap.error.as_ref());
    if snap.engine_suspended {
        let detail = snap
            .engine
            .as_ref()
            .map(|e| e.detail.as_slice())
            .unwrap_or(&[]);
        tip.push('\n');
        tip.push_str(&text::engine_paused(lang, detail));
    }
    let _ = tray.set_tooltip(Some(tip));
    #[cfg(target_os = "macos")]
    let _ = tray.set_title(Some(
        pct.map(|p| format!("{}%", p.round() as i64))
            .unwrap_or_default(),
    ));
    refresh_menu(app, snap.list.as_ref(), false);
}

/// Ricostruisce il menu se la firma è cambiata (o sempre, con `force`).
pub fn refresh_menu(app: &AppHandle, list: Option<&ListResponse>, force: bool) {
    let shared = app.state::<Arc<Shared>>();
    let (lang, maps) = {
        let settings = lock(&shared.settings);
        (settings.lang, map_entries(&settings.engine_profiles))
    };
    let owned;
    let list = match list {
        Some(l) => Some(l),
        None => {
            owned = lock(&shared.snapshot).list.clone();
            owned.as_ref()
        }
    };
    let sig = signature(list, &maps, windows::mini_open(app), lang);
    {
        let mut last = lock(&shared.menu_signature);
        if !force && *last == sig {
            return;
        }
        *last = sig;
    }
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build_menu(app, list, &maps, lang)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => windows::show_main(app),
        "mini" => windows::toggle_mini(app),
        "refresh" => app::poller(app).refresh(),
        "quit" => app.exit(0),
        _ => {
            if let Some(n) = account_from_id(id) {
                switch_to(app, n);
            } else if let Some(map) = map_from_id(id) {
                select_map(app, map.to_owned());
            }
        }
    }
    // Su alcuni sistemi le voci a spunta si invertono da sole al clic: si riallinea il menu.
    if id != "quit" {
        refresh_menu(app, None, true);
    }
}

fn switch_to(app: &AppHandle, number: u32) {
    let shared = Arc::clone(&app.state::<Arc<Shared>>());
    if lock(&shared.snapshot).active() == Some(number) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let action = CswapAction::Switch {
            target: Some(number),
            strategy: None,
            models: None,
            force: false,
        };
        match shared.api.execute(&action) {
            Ok(outcome) => app::apply_outcome(&app, &outcome),
            Err(e) => {
                let lang = lock(&shared.settings).lang;
                app::notify(&app, text::switch_failed(lang, &e));
            }
        }
        refresh_menu(&app, None, true);
    });
}

/// Rende attiva una mappa (come «Cambia mappa» dell'interfaccia): le sue regole valgono dal giro
/// di lettura chiesto subito dopo, che avvisa anche l'interfaccia.
fn select_map(app: &AppHandle, id: String) {
    let shared = Arc::clone(&app.state::<Arc<Shared>>());
    if lock(&shared.settings).engine_profiles.active_id == id {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let cmd = ProfileCommand::Select { id };
        match shared.update_config(|c| c.apply_profile_command(&cmd)) {
            Ok(_) => app::poller(&app).refresh(),
            Err(e) => {
                let lang = lock(&shared.settings).lang;
                app::notify(&app, text::map_failed(lang, &e));
            }
        }
        refresh_menu(&app, None, true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mock::fixture_list;
    use swapdeck_core::engine::EngineRules;

    #[test]
    fn entries_mark_active_and_disabled() {
        let mut list = fixture_list();
        list.accounts[0].disabled = Some(true);
        let e = account_entries(&list);
        assert_eq!(e.len(), 5);
        assert_eq!(e[0].id, "acct:1");
        assert!(!e[0].enabled);
        assert!(e[4].checked && e[4].enabled);
        assert_eq!(e.iter().filter(|x| x.checked).count(), 1);
        assert!(e.iter().all(|x| !x.label.contains('@')));
    }

    #[test]
    fn ids_round_trip() {
        assert_eq!(account_from_id("acct:12"), Some(12));
        assert_eq!(account_from_id("acct:x"), None);
        assert_eq!(account_from_id("open"), None);
    }

    #[test]
    fn signature_changes_with_state() {
        let list = fixture_list();
        let a = signature(Some(&list), &[], false, Lang::It);
        assert_eq!(a, signature(Some(&list), &[], false, Lang::It));
        assert_ne!(a, signature(Some(&list), &[], true, Lang::It));
        assert_ne!(a, signature(None, &[], false, Lang::It));
        let maps = map_entries(&EngineProfiles::seeded(&EngineRules::default()));
        assert_ne!(a, signature(Some(&list), &maps, false, Lang::It));
        let mut other = list.clone();
        other.accounts[4]
            .usage
            .as_mut()
            .unwrap()
            .five_hour
            .as_mut()
            .unwrap()
            .pct = 70.0;
        assert_ne!(a, signature(Some(&other), &[], false, Lang::It));
    }

    #[test]
    fn map_entries_check_the_active_one() {
        let mut profiles = EngineProfiles::seeded(&EngineRules::default());
        let select = ProfileCommand::Select { id: "eco".into() };
        profiles.apply(&select, &EngineRules::default()).unwrap();
        let e = map_entries(&profiles);
        assert_eq!(e.len(), profiles.profiles.len());
        assert_eq!(e.iter().filter(|m| m.checked).count(), 1);
        let active = e.iter().find(|m| m.checked).unwrap();
        assert_eq!(map_from_id(&active.id), Some("eco"));
        assert_eq!(map_from_id("map:"), None);
        assert_eq!(map_from_id("acct:1"), None);
    }
}
