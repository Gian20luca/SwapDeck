//! Finestre: la principale si nasconde alla chiusura; il mini-widget (senza cornice, sempre in primo
//! piano) si apre e si chiude dal menu e ricorda la posizione nella config.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{
    AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent,
};

use crate::state::{Shared, lock};
use crate::tray;

pub const MAIN: &str = "main";
pub const MINI: &str = "mini";
/// Dimensioni del mini-widget in pixel logici.
pub const MINI_SIZE: (f64, f64) = (280.0, 160.0);
/// Margine dal bordo dello schermo per la posizione di default.
const MARGIN: i32 = 24;
/// Spazio lasciato alla barra delle applicazioni sotto il mini-widget.
const TASKBAR: i32 = 56;
/// Parte minima del mini-widget che deve restare su uno schermo.
const VISIBLE_MIN: i32 = 40;
/// Pausa minima tra due salvataggi della posizione durante il trascinamento.
const SAVE_EVERY: Duration = Duration::from_secs(2);

/// Rettangolo di un monitor in pixel fisici: x, y, larghezza, altezza.
pub type Rect = (i32, i32, u32, u32);

/// `true` se il mini-widget in `pos` resta visibile almeno in parte su uno dei monitor.
pub fn fits(pos: (i32, i32), monitors: &[Rect]) -> bool {
    monitors.iter().any(|&(x, y, w, h)| {
        let (w, h) = (w as i32, h as i32);
        pos.0 >= x - VISIBLE_MIN * 4
            && pos.0 <= x + w - VISIBLE_MIN
            && pos.1 >= y
            && pos.1 <= y + h - VISIBLE_MIN
    })
}

/// Posizione di default: in basso a destra del monitor, sopra la barra delle applicazioni.
pub fn default_position(monitor: Rect, size: (u32, u32)) -> (i32, i32) {
    let (x, y, w, h) = monitor;
    (
        (x + w as i32 - size.0 as i32 - MARGIN).max(x),
        (y + h as i32 - size.1 as i32 - MARGIN - TASKBAR).max(y),
    )
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn mini_open(app: &AppHandle) -> bool {
    app.get_webview_window(MINI).is_some()
}

pub fn toggle_mini(app: &AppHandle) {
    match app.get_webview_window(MINI) {
        Some(w) => {
            remember_mini(app, w.outer_position(), true);
            let _ = w.close();
        }
        None => {
            if let Err(e) = open_mini(app) {
                eprintln!("mini-widget non aperto: {e}");
            }
        }
    }
}

fn open_mini(app: &AppHandle) -> tauri::Result<()> {
    let w = WebviewWindowBuilder::new(app, MINI, WebviewUrl::App("index.html".into()))
        .title("SwapDeck")
        .inner_size(MINI_SIZE.0, MINI_SIZE.1)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()?;
    let monitors: Vec<Rect> = w
        .available_monitors()?
        .iter()
        .map(|m| {
            (
                m.position().x,
                m.position().y,
                m.size().width,
                m.size().height,
            )
        })
        .collect();
    let saved = lock(&app.state::<Arc<Shared>>().settings).mini_position;
    let pos = match saved.filter(|p| fits(*p, &monitors)) {
        Some(p) => p,
        None => {
            let size = w.outer_size()?;
            let monitor = w
                .primary_monitor()?
                .map(|m| {
                    (
                        m.position().x,
                        m.position().y,
                        m.size().width,
                        m.size().height,
                    )
                })
                .or_else(|| monitors.first().copied())
                .unwrap_or((0, 0, 1280, 800));
            default_position(monitor, (size.width, size.height))
        }
    };
    w.set_position(PhysicalPosition::new(pos.0, pos.1))?;
    w.show()?;
    Ok(())
}

/// Salva la posizione del mini-widget: subito con `force`, altrimenti al massimo ogni [`SAVE_EVERY`].
fn remember_mini(app: &AppHandle, pos: tauri::Result<PhysicalPosition<i32>>, force: bool) {
    let Ok(pos) = pos else {
        return;
    };
    // Finestra ridotta a icona su Windows: posizione fittizia da non salvare.
    if pos.x <= -30000 || pos.y <= -30000 {
        return;
    }
    let shared = app.state::<Arc<Shared>>();
    lock(&shared.settings).mini_position = Some((pos.x, pos.y));
    let mut saved = lock(&shared.mini_saved_at);
    if force || saved.is_none_or(|t| t.elapsed() >= SAVE_EVERY) {
        *saved = Some(Instant::now());
        drop(saved);
        if let Err(e) = shared.save_settings() {
            eprintln!("posizione del mini-widget non salvata: {e}");
        }
    }
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    let app = window.app_handle();
    match (window.label(), event) {
        (MAIN, WindowEvent::CloseRequested { api, .. }) => {
            api.prevent_close();
            let _ = window.hide();
        }
        (MINI, WindowEvent::Moved(_)) => remember_mini(app, window.outer_position(), false),
        (MINI, WindowEvent::CloseRequested { .. }) => {
            remember_mini(app, window.outer_position(), true)
        }
        (MINI, WindowEvent::Destroyed) => tray::refresh_menu(app, None, true),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FHD: Rect = (0, 0, 1920, 1080);
    const RIGHT: Rect = (1920, 0, 2560, 1440);

    #[test]
    fn default_is_bottom_right() {
        assert_eq!(default_position(FHD, (280, 160)), (1616, 840));
        assert_eq!(default_position((0, 0, 200, 100), (280, 160)), (0, 0));
    }

    #[test]
    fn saved_position_must_stay_on_a_monitor() {
        assert!(fits((1616, 840), &[FHD]));
        assert!(fits((3000, 100), &[FHD, RIGHT]));
        assert!(!fits((3000, 100), &[FHD]), "monitor scollegato");
        assert!(!fits((-32000, -32000), &[FHD]), "finestra ridotta a icona");
        assert!(!fits((100, 1075), &[FHD]), "quasi tutta sotto lo schermo");
        assert!(!fits((100, 100), &[]));
    }
}
