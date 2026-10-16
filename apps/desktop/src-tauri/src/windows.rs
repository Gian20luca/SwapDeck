//! Finestre: la principale si nasconde alla chiusura; il mini-widget (senza cornice, sempre in primo
//! piano) si apre e si chiude dal menu e ricorda la posizione nella config.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use swapdeck_core::config::Theme;
use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, Theme as WindowTheme, WebviewUrl,
    WebviewWindowBuilder, Window, WindowEvent,
};

use crate::state::{Shared, lock};
use crate::tray;

pub const MAIN: &str = "main";
pub const MINI: &str = "mini";
/// Dimensioni del mini-widget in pixel logici.
pub const MINI_SIZE: (f64, f64) = (280.0, 160.0);
/// Striscia del mini-widget compresso, in pixel logici.
pub const MINI_COMPACT_SIZE: (f64, f64) = (280.0, 28.0);
/// Altezza adattabile del mini-widget esteso (pixel logici): sotto e sopra non si va.
const MINI_HEIGHT_RANGE: (u32, u32) = (100, 400);
/// Ultima altezza del mini-widget esteso, per riespanderlo com'era.
static EXPANDED_HEIGHT: AtomicU32 = AtomicU32::new(MINI_SIZE.1 as u32);
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

/// Posizione che tiene la finestra di `size` tutta dentro il monitor che contiene `pos`
/// (il primo se nessuno lo contiene): serve quando la finestra cresce o lo schermo è cambiato.
pub fn clamp_into(pos: (i32, i32), size: (u32, u32), monitors: &[Rect]) -> (i32, i32) {
    let contains = |&&(x, y, w, h): &&Rect| {
        pos.0 >= x && pos.0 < x + w as i32 && pos.1 >= y && pos.1 < y + h as i32
    };
    let Some(&(x, y, w, h)) = monitors.iter().find(contains).or_else(|| monitors.first()) else {
        return pos;
    };
    let max_x = (x + w as i32 - size.0 as i32).max(x);
    let max_y = (y + h as i32 - size.1 as i32).max(y);
    (pos.0.clamp(x, max_x), pos.1.clamp(y, max_y))
}

/// Posizione di default: in basso a destra del monitor, sopra la barra delle applicazioni.
pub fn default_position(monitor: Rect, size: (u32, u32)) -> (i32, i32) {
    let (x, y, w, h) = monitor;
    (
        (x + w as i32 - size.0 as i32 - MARGIN).max(x),
        (y + h as i32 - size.1 as i32 - MARGIN - TASKBAR).max(y),
    )
}

/// Tema della cornice nativa della finestra principale dalle preferenze salvate, prima di
/// mostrarla (la UI lo riapplica a ogni cambio): evita il lampo chiaro con il tema scuro.
pub fn apply_saved_theme(app: &AppHandle) {
    let theme = match lock(&app.state::<Arc<Shared>>().settings).theme {
        Theme::Dark => Some(WindowTheme::Dark),
        Theme::Light => Some(WindowTheme::Light),
        Theme::System => None,
    };
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.set_theme(theme);
    }
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
    let compact = lock(&app.state::<Arc<Shared>>().settings).mini_compact;
    let (width, height) = mini_size(compact);
    let w = WebviewWindowBuilder::new(app, MINI, WebviewUrl::App("index.html".into()))
        .title("SwapDeck")
        .inner_size(width, height)
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
    let size = w.outer_size()?;
    let pos = clamp_into(pos, (size.width, size.height), &monitors);
    w.set_position(PhysicalPosition::new(pos.0, pos.1))?;
    w.show()?;
    Ok(())
}

/// Dimensioni in pixel logici del mini-widget: striscia o, esteso, l'ultima altezza adattata.
fn mini_size(compact: bool) -> (f64, f64) {
    if compact {
        MINI_COMPACT_SIZE
    } else {
        (
            MINI_SIZE.0,
            f64::from(EXPANDED_HEIGHT.load(Ordering::Relaxed)),
        )
    }
}

/// Ridimensiona il mini-widget aperto tenendo l'angolo in alto a sinistra e, se la nuova altezza
/// lo porterebbe sotto il bordo dello schermo, lo sposta in su.
fn resize_mini(app: &AppHandle, size: (f64, f64)) -> tauri::Result<()> {
    let Some(w) = app.get_webview_window(MINI) else {
        return Ok(());
    };
    w.set_size(LogicalSize::new(size.0, size.1))?;
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
    let (pos, outer) = (w.outer_position()?, w.outer_size()?);
    let wanted = clamp_into((pos.x, pos.y), (outer.width, outer.height), &monitors);
    if wanted != (pos.x, pos.y) {
        w.set_position(PhysicalPosition::new(wanted.0, wanted.1))?;
    }
    remember_mini(app, w.outer_position(), true);
    Ok(())
}

/// Comprime il mini-widget in una striscia o lo riespande; lo stato si ricorda nella config.
pub fn set_mini_compact(app: &AppHandle, compact: bool) -> tauri::Result<()> {
    let shared = app.state::<Arc<Shared>>();
    lock(&shared.settings).mini_compact = compact;
    resize_mini(app, mini_size(compact))?;
    if let Err(e) = shared.save_settings() {
        eprintln!("stato del mini-widget non salvato: {e}");
    }
    Ok(())
}

/// Adatta l'altezza del mini-widget esteso al contenuto (px logici); da compresso non fa nulla.
pub fn fit_mini_height(app: &AppHandle, height: f64) -> tauri::Result<()> {
    if lock(&app.state::<Arc<Shared>>().settings).mini_compact || !height.is_finite() {
        return Ok(());
    }
    let (min, max) = MINI_HEIGHT_RANGE;
    let height = height.ceil().clamp(f64::from(min), f64::from(max)) as u32;
    if EXPANDED_HEIGHT.swap(height, Ordering::Relaxed) == height {
        return Ok(());
    }
    resize_mini(app, (MINI_SIZE.0, f64::from(height)))
}

/// Nasconde il mini-widget (chiude la finestra, ricordandone la posizione); l'app resta
/// nell'area di notifica e la voce «Mini-widget» del menu dell'icona lo riapre.
pub fn hide_mini(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MINI) {
        remember_mini(app, w.outer_position(), true);
        let _ = w.close();
    }
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
    fn window_is_kept_inside_its_monitor() {
        // Cresce oltre il bordo basso: sale.
        assert_eq!(clamp_into((1616, 900), (280, 300), &[FHD]), (1616, 780));
        // Già dentro: resta.
        assert_eq!(clamp_into((100, 100), (280, 300), &[FHD]), (100, 100));
        // Secondo monitor: si usa quello che contiene l'angolo.
        assert_eq!(
            clamp_into((3000, 1300), (280, 300), &[FHD, RIGHT]),
            (3000, 1140)
        );
        // Nessun monitor contiene l'angolo: si usa il primo.
        assert_eq!(clamp_into((5000, 5000), (280, 300), &[FHD]), (1640, 780));
        assert_eq!(clamp_into((7, 9), (280, 300), &[]), (7, 9));
    }

    #[test]
    fn mini_size_follows_the_compact_state() {
        assert_eq!(mini_size(true), (280.0, 28.0));
        assert_eq!(mini_size(false).0, 280.0);
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
