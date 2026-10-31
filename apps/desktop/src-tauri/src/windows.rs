//! Finestre: la principale si nasconde alla chiusura; il mini-widget (senza cornice, sempre in primo
//! piano) si apre e si chiude dal menu e ricorda nella config dove l'utente ha messo la striscia
//! compressa e il widget esteso (`miniWidget.compactPosition` ed `expandedPosition`).
//!
//! Le coordinate sono tutte in pixel fisici del desktop virtuale; le dimensioni del mini-widget
//! sono logiche e si convertono con la scala del monitor che lo contiene.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use swapdeck_core::config::Theme;
use tauri::{
    AppHandle, LogicalSize, Manager, Monitor, PhysicalPosition, Theme as WindowTheme, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, Window, WindowEvent,
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
/// Margine dal bordo dell'area di lavoro per la posizione di default.
const MARGIN: i32 = 24;
/// Parte minima del mini-widget che deve restare su uno schermo.
const VISIBLE_MIN: i32 = 40;
/// Pausa minima tra due salvataggi della posizione durante il trascinamento.
const SAVE_EVERY: Duration = Duration::from_secs(2);
/// Dopo uno spostamento fatto dal programma, gli eventi `Moved` di questo intervallo non sono
/// dell'utente e non cambiano le posizioni salvate.
const PROGRAM_MOVE_GUARD: Duration = Duration::from_millis(800);
static PROGRAM_MOVE_UNTIL: Mutex<Option<Instant>> = Mutex::new(None);
/// Una sistemazione del mini-widget alla volta (compressione e adattamento dell'altezza).
static PLACING: Mutex<()> = Mutex::new(());

/// Rettangolo in pixel fisici: x, y, larghezza, altezza.
pub type Rect = (i32, i32, u32, u32);

/// Monitor collegato: area intera, area di lavoro (senza la barra delle applicazioni) e scala.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Screen {
    pub bounds: Rect,
    pub work: Rect,
    pub scale: f64,
}

impl Screen {
    fn of(m: &Monitor) -> Self {
        let work = m.work_area();
        let mut s = Self {
            bounds: (
                m.position().x,
                m.position().y,
                m.size().width,
                m.size().height,
            ),
            work: (
                work.position.x,
                work.position.y,
                work.size.width,
                work.size.height,
            ),
            scale: m.scale_factor(),
        };
        // Area di lavoro assente o incoerente (alcuni gestori di finestre su Linux): monitor intero.
        if s.work.2 == 0 || s.work.3 == 0 || !contains(s.bounds, (s.work.0, s.work.1)) {
            s.work = s.bounds;
        }
        if !s.scale.is_finite() || s.scale <= 0.0 {
            s.scale = 1.0;
        }
        s
    }
}

fn contains((x, y, w, h): Rect, p: (i32, i32)) -> bool {
    p.0 >= x && p.0 < x + w as i32 && p.1 >= y && p.1 < y + h as i32
}

fn center((x, y, w, h): Rect) -> (i32, i32) {
    (x + (w / 2) as i32, y + (h / 2) as i32)
}

/// Dimensioni del mini-widget: logiche (area della pagina) più il bordo fisico che il sistema
/// aggiunge attorno alla finestra senza cornice (su Windows 11 i bordi invisibili), così il
/// rettangolo esterno calcolato coincide con quello della finestra.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WinSize {
    pub logical: (f64, f64),
    pub frame: (u32, u32),
}

impl From<(f64, f64)> for WinSize {
    fn from(logical: (f64, f64)) -> Self {
        Self {
            logical,
            frame: (0, 0),
        }
    }
}

/// Dimensioni fisiche esterne di una finestra di dimensioni `size` su un monitor con `scale`.
fn physical(size: WinSize, scale: f64) -> (u32, u32) {
    (
        (size.logical.0 * scale).round() as u32 + size.frame.0,
        (size.logical.1 * scale).round() as u32 + size.frame.1,
    )
}

/// Monitor che contiene `p`, altrimenti il più vicino; `None` solo senza monitor.
pub fn screen_for(p: (i32, i32), screens: &[Screen]) -> Option<&Screen> {
    screens.iter().find(|s| contains(s.bounds, p)).or_else(|| {
        screens.iter().min_by_key(|s| {
            let (x, y, w, h) = s.bounds;
            let dx = i64::from((x - p.0).max(p.0 - (x + w as i32 - 1)).max(0));
            let dy = i64::from((y - p.1).max(p.1 - (y + h as i32 - 1)).max(0));
            dx * dx + dy * dy
        })
    })
}

/// `true` se il mini-widget in `pos` resta visibile almeno in parte su uno dei monitor.
pub fn fits(pos: (i32, i32), screens: &[Screen]) -> bool {
    screens.iter().any(|s| {
        let (x, y, w, h) = s.bounds;
        let (w, h) = (w as i32, h as i32);
        pos.0 >= x - VISIBLE_MIN * 4
            && pos.0 <= x + w - VISIBLE_MIN
            && pos.1 >= y
            && pos.1 <= y + h - VISIBLE_MIN
    })
}

/// `true` se il rettangolo sta tutto dentro uno dei monitor collegati.
pub fn inside(r: Rect, screens: &[Screen]) -> bool {
    screens.iter().any(|s| {
        let (x, y, w, h) = s.bounds;
        r.0 >= x
            && r.1 >= y
            && i64::from(r.0) + i64::from(r.2) <= i64::from(x) + i64::from(w)
            && i64::from(r.1) + i64::from(r.3) <= i64::from(y) + i64::from(h)
    })
}

/// Posizione che tiene una finestra di `size` tutta dentro l'area di lavoro di `screen`.
fn clamp_to_work(pos: (i32, i32), size: (u32, u32), screen: &Screen) -> (i32, i32) {
    let (x, y, w, h) = screen.work;
    let max_x = (x + w as i32 - size.0 as i32).max(x);
    let max_y = (y + h as i32 - size.1 as i32).max(y);
    (pos.0.clamp(x, max_x), pos.1.clamp(y, max_y))
}

/// Posizione di default: in basso a destra dell'area di lavoro, sopra la barra delle applicazioni.
pub fn default_position(work: Rect, size: (u32, u32)) -> (i32, i32) {
    let (x, y, w, h) = work;
    (
        (x + w as i32 - size.0 as i32 - MARGIN).max(x),
        (y + h as i32 - size.1 as i32 - MARGIN).max(y),
    )
}

/// Rettangolo di dimensioni logiche `size` ricavato da `from` (striscia o widget esteso) nel suo
/// monitor: nella metà bassa dell'area di lavoro tiene il bordo inferiore (si espande verso
/// l'alto), nella metà alta quello superiore; sempre il bordo destro. Resta nell'area di lavoro.
pub fn anchored(from: Rect, size: impl Into<WinSize>, screens: &[Screen]) -> Rect {
    let size = size.into();
    let Some(screen) = screen_for(center(from), screens) else {
        let (w, h) = physical(size, 1.0);
        return (from.0, from.1, w, h);
    };
    let (w, h) = physical(size, screen.scale);
    let lower = center(from).1 >= center(screen.work).1;
    let x = from.0 + from.2 as i32 - w as i32;
    let y = if lower {
        from.1 + from.3 as i32 - h as i32
    } else {
        from.1
    };
    let (x, y) = clamp_to_work((x, y), (w, h), screen);
    (x, y, w, h)
}

/// Rettangolo di default nel monitor principale (o nel primo).
fn default_rect(size: WinSize, primary: Option<Screen>, screens: &[Screen]) -> Rect {
    let screen = primary
        .or_else(|| screens.first().copied())
        .unwrap_or(Screen {
            bounds: (0, 0, 1280, 800),
            work: (0, 0, 1280, 800),
            scale: 1.0,
        });
    let (w, h) = physical(size, screen.scale);
    let (x, y) = default_position(screen.work, (w, h));
    (x, y, w, h)
}

/// Rettangolo di una finestra di dimensioni logiche `size` con l'angolo in `pos`.
fn rect_at(pos: (i32, i32), size: WinSize, screens: &[Screen]) -> Rect {
    let scale = screen_for(pos, screens).map_or(1.0, |s| s.scale);
    let (w, h) = physical(size, scale);
    (pos.0, pos.1, w, h)
}

/// Dove va il widget esteso di dimensioni logiche `size`: alla posizione salvata dall'utente se
/// è ancora su uno schermo (dentro l'area di lavoro), altrimenti ricavata dalla striscia `strip`
/// (la finestra aperta, o la striscia salvata all'apertura), altrimenti quella di default.
pub fn expanded_target(
    strip: Option<Rect>,
    saved: Option<(i32, i32)>,
    size: impl Into<WinSize>,
    primary: Option<Screen>,
    screens: &[Screen],
) -> Rect {
    let size = size.into();
    if let Some(r) = saved_in_work(saved, size, screens) {
        return r;
    }
    match strip {
        Some(r) => anchored(r, size, screens),
        None => default_rect(size, primary, screens),
    }
}

/// Posizione salvata ancora su uno schermo, tenuta dentro l'area di lavoro del suo monitor.
fn saved_in_work(saved: Option<(i32, i32)>, size: WinSize, screens: &[Screen]) -> Option<Rect> {
    let p = saved.filter(|p| fits(*p, screens))?;
    let screen = screen_for(p, screens)?;
    let (w, h) = physical(size, screen.scale);
    let (x, y) = clamp_to_work(p, (w, h), screen);
    Some((x, y, w, h))
}

/// Dove va la striscia: esattamente alla posizione salvata se sta tutta su uno schermo collegato,
/// altrimenti ricavata dal widget esteso `widget`, altrimenti la salvata tenuta dentro l'area di
/// lavoro, altrimenti quella di default.
pub fn compact_target(
    widget: Option<Rect>,
    saved: Option<(i32, i32)>,
    size: impl Into<WinSize>,
    primary: Option<Screen>,
    screens: &[Screen],
) -> Rect {
    let size = size.into();
    if let Some(p) = saved {
        let r = rect_at(p, size, screens);
        if inside(r, screens) {
            return r;
        }
    }
    if let Some(r) = widget {
        return anchored(r, size, screens);
    }
    saved_in_work(saved, size, screens).unwrap_or_else(|| default_rect(size, primary, screens))
}

/// Nuovo rettangolo quando cambia l'altezza del widget esteso: nella metà bassa resta fermo il
/// bordo inferiore, nella metà alta quello superiore; sempre dentro l'area di lavoro.
pub fn refit(current: Rect, size: impl Into<WinSize>, screens: &[Screen]) -> Rect {
    let size = size.into();
    let Some(screen) = screen_for(center(current), screens) else {
        return current;
    };
    let (w, h) = physical(size, screen.scale);
    let lower = center(current).1 >= center(screen.work).1;
    let y = if lower {
        current.1 + current.3 as i32 - h as i32
    } else {
        current.1
    };
    let (x, y) = clamp_to_work((current.0, y), (w, h), screen);
    (x, y, w, h)
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

/// macOS: icona nel Dock solo con la finestra principale aperta, come le app della barra
/// dei menu (con il mini-widget o la sola icona nella barra resta fuori dal Dock).
pub fn set_dock_visible(app: &AppHandle, visible: bool) {
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy::{Accessory, Regular};
        let _ = app.set_activation_policy(if visible { Regular } else { Accessory });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, visible);
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        set_dock_visible(app, true);
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
            persist_mini(app);
            let _ = w.close();
        }
        None => {
            if let Err(e) = open_mini(app) {
                eprintln!("mini-widget non aperto: {e}");
            }
        }
    }
}

fn screens_of(w: &WebviewWindow) -> tauri::Result<Vec<Screen>> {
    Ok(w.available_monitors()?.iter().map(Screen::of).collect())
}

fn primary_of(w: &WebviewWindow) -> Option<Screen> {
    w.primary_monitor().ok().flatten().as_ref().map(Screen::of)
}

/// Bordo che il sistema aggiunge attorno all'area della pagina (esterno meno interno).
fn frame_of(w: &WebviewWindow) -> tauri::Result<(u32, u32)> {
    let (outer, inner) = (w.outer_size()?, w.inner_size()?);
    Ok((
        outer.width.saturating_sub(inner.width),
        outer.height.saturating_sub(inner.height),
    ))
}

fn sized(logical: (f64, f64), frame: (u32, u32)) -> WinSize {
    WinSize { logical, frame }
}

fn current_rect(w: &WebviewWindow) -> tauri::Result<Rect> {
    let (pos, size) = (w.outer_position()?, w.outer_size()?);
    Ok((pos.x, pos.y, size.width, size.height))
}

fn mark_program_move() {
    *lock(&PROGRAM_MOVE_UNTIL) = Some(Instant::now() + PROGRAM_MOVE_GUARD);
}

fn is_program_move() -> bool {
    lock(&PROGRAM_MOVE_UNTIL).is_some_and(|until| Instant::now() < until)
}

fn open_mini(app: &AppHandle) -> tauri::Result<()> {
    let shared = app.state::<Arc<Shared>>();
    let (compact, compact_pos, expanded_pos) = {
        let s = lock(&shared.settings);
        (
            s.mini_compact,
            s.mini_compact_position,
            s.mini_expanded_position,
        )
    };
    let size = mini_size(compact);
    let w = WebviewWindowBuilder::new(app, MINI, WebviewUrl::App("index.html".into()))
        .title("SwapDeck")
        .inner_size(size.0, size.1)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()?;
    let screens = screens_of(&w)?;
    let primary = primary_of(&w);
    let frame = frame_of(&w)?;
    let target = if compact {
        compact_target(None, compact_pos, sized(size, frame), primary, &screens)
    } else {
        let strip = compact_pos
            .filter(|p| fits(*p, &screens))
            .map(|p| rect_at(p, sized(MINI_COMPACT_SIZE, frame), &screens));
        expanded_target(strip, expanded_pos, sized(size, frame), primary, &screens)
    };
    mark_program_move();
    w.set_position(PhysicalPosition::new(target.0, target.1))?;
    w.show()?;
    mark_program_move();
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

/// Porta il mini-widget al rettangolo `target` con dimensioni logiche `size`. Se cresce prima si
/// sposta e poi si allarga, se cala il contrario: la finestra non passa mai fuori dal rettangolo
/// vecchio o nuovo (niente lampo sopra la barra delle applicazioni).
fn place(w: &WebviewWindow, size: (f64, f64), current: Rect, target: Rect) -> tauri::Result<()> {
    if (current.0, current.1) == (target.0, target.1) && current.3 == target.3 {
        return w.set_size(LogicalSize::new(size.0, size.1));
    }
    mark_program_move();
    let pos = PhysicalPosition::new(target.0, target.1);
    let logical = LogicalSize::new(size.0, size.1);
    if target.3 > current.3 {
        w.set_position(pos)?;
        w.set_size(logical)?;
    } else {
        w.set_size(logical)?;
        w.set_position(pos)?;
    }
    mark_program_move();
    Ok(())
}

/// Comprime il mini-widget in una striscia o lo riespande; lo stato si ricorda nella config.
/// Riducendo la striscia torna dove l'utente l'aveva messa; espandendo il widget va dove l'utente
/// l'aveva messo oppure cresce dalla striscia verso l'interno dello schermo.
pub fn set_mini_compact(app: &AppHandle, compact: bool) -> tauri::Result<()> {
    let shared = app.state::<Arc<Shared>>();
    let _placing = lock(&PLACING);
    let (was, compact_pos, expanded_pos) = {
        let mut s = lock(&shared.settings);
        let was = s.mini_compact;
        s.mini_compact = compact;
        (was, s.mini_compact_position, s.mini_expanded_position)
    };
    let window = app.get_webview_window(MINI).filter(|_| was != compact);
    if let Some(w) = window {
        let (current, screens) = (current_rect(&w)?, screens_of(&w)?);
        let primary = primary_of(&w);
        let size = sized(mini_size(compact), frame_of(&w)?);
        let target = if compact {
            compact_target(Some(current), compact_pos, size, primary, &screens)
        } else {
            expanded_target(Some(current), expanded_pos, size, primary, &screens)
        };
        place(&w, size.logical, current, target)?;
    }
    if let Err(e) = shared.save_settings() {
        eprintln!("stato del mini-widget non salvato: {e}");
    }
    Ok(())
}

/// Adatta l'altezza del mini-widget esteso al contenuto (px logici); da compresso non fa nulla.
pub fn fit_mini_height(app: &AppHandle, height: f64) -> tauri::Result<()> {
    if !height.is_finite() {
        return Ok(());
    }
    let _placing = lock(&PLACING);
    if lock(&app.state::<Arc<Shared>>().settings).mini_compact {
        return Ok(());
    }
    let (min, max) = MINI_HEIGHT_RANGE;
    let height = height.ceil().clamp(f64::from(min), f64::from(max)) as u32;
    if EXPANDED_HEIGHT.swap(height, Ordering::Relaxed) == height {
        return Ok(());
    }
    let Some(w) = app.get_webview_window(MINI) else {
        return Ok(());
    };
    let size = (MINI_SIZE.0, f64::from(height));
    let current = current_rect(&w)?;
    let target = refit(current, sized(size, frame_of(&w)?), &screens_of(&w)?);
    place(&w, size, current, target)
}

/// Nasconde il mini-widget (chiude la finestra, salvando le posizioni); l'app resta
/// nell'area di notifica e la voce «Mini-widget» del menu dell'icona lo riapre.
pub fn hide_mini(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MINI) {
        persist_mini(app);
        let _ = w.close();
    }
}

/// Spostamento fatto dall'utente: la striscia aggiorna `compactPosition` (e la posizione estesa
/// si ricalcolerà da lì), il widget esteso `expandedPosition`. Su disco al massimo ogni
/// [`SAVE_EVERY`]; alla chiusura [`persist_mini`] scrive l'ultima.
fn user_moved(app: &AppHandle, pos: tauri::Result<PhysicalPosition<i32>>) {
    let Ok(pos) = pos else {
        return;
    };
    // Finestra ridotta a icona su Windows: posizione fittizia da non salvare.
    if pos.x <= -30000 || pos.y <= -30000 || is_program_move() {
        return;
    }
    let shared = app.state::<Arc<Shared>>();
    {
        let mut s = lock(&shared.settings);
        if s.mini_compact {
            s.mini_compact_position = Some((pos.x, pos.y));
            s.mini_expanded_position = None;
        } else {
            s.mini_expanded_position = Some((pos.x, pos.y));
        }
    }
    let mut saved = lock(&shared.mini_saved_at);
    if saved.is_none_or(|t| t.elapsed() >= SAVE_EVERY) {
        *saved = Some(Instant::now());
        drop(saved);
        if let Err(e) = shared.save_settings() {
            eprintln!("posizione del mini-widget non salvata: {e}");
        }
    }
}

/// Scrive subito le posizioni in memoria (si riscrive solo se sono cambiate).
fn persist_mini(app: &AppHandle) {
    let shared = app.state::<Arc<Shared>>();
    *lock(&shared.mini_saved_at) = Some(Instant::now());
    if let Err(e) = shared.save_settings() {
        eprintln!("posizione del mini-widget non salvata: {e}");
    }
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    let app = window.app_handle();
    match (window.label(), event) {
        (MAIN, WindowEvent::CloseRequested { api, .. }) => {
            api.prevent_close();
            let _ = window.hide();
            set_dock_visible(app, false);
        }
        (MINI, WindowEvent::Moved(_)) => user_moved(app, window.outer_position()),
        (MINI, WindowEvent::CloseRequested { .. }) => persist_mini(app),
        (MINI, WindowEvent::Destroyed) => tray::refresh_menu(app, None, true),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1920×1080 a scala 1 con la barra delle applicazioni in basso (40 px).
    const FHD: Screen = Screen {
        bounds: (0, 0, 1920, 1080),
        work: (0, 0, 1920, 1040),
        scale: 1.0,
    };
    /// A destra, 2560×1440 a scala 1,5 con la barra di 48 px.
    const RIGHT: Screen = Screen {
        bounds: (1920, 0, 2560, 1440),
        work: (1920, 0, 2560, 1392),
        scale: 1.5,
    };
    /// A sinistra (coordinate negative), scala 1,25, barra in alto di 50 px.
    const LEFT: Screen = Screen {
        bounds: (-1920, 0, 1920, 1200),
        work: (-1920, 50, 1920, 1150),
        scale: 1.25,
    };
    const EXPANDED: (f64, f64) = (280.0, 230.0);

    #[test]
    fn default_is_bottom_right_of_the_work_area() {
        assert_eq!(default_position(FHD.work, (280, 160)), (1616, 856));
        assert_eq!(default_position((0, 0, 200, 100), (280, 160)), (0, 0));
        assert_eq!(
            expanded_target(None, None, EXPANDED, None, &[FHD]),
            (1616, 786, 280, 230)
        );
        assert_eq!(
            compact_target(None, None, MINI_COMPACT_SIZE, Some(RIGHT), &[FHD, RIGHT]),
            (1920 + 2560 - 420 - 24, 1392 - 42 - 24, 420, 42)
        );
    }

    #[test]
    fn lower_half_expands_upwards_keeping_bottom_and_right_edges() {
        // Striscia appena sopra la barra delle applicazioni, in basso a destra.
        let strip = (1616, 1012, 280, 28);
        let r = expanded_target(Some(strip), None, EXPANDED, None, &[FHD]);
        assert_eq!(r, (1616, 810, 280, 230));
        assert_eq!(
            r.1 + r.3 as i32,
            1040,
            "stesso bordo inferiore, sopra la barra"
        );
        // E tornando indietro senza posizione salvata: la stessa striscia.
        assert_eq!(
            compact_target(Some(r), None, MINI_COMPACT_SIZE, None, &[FHD]),
            strip
        );
    }

    #[test]
    fn upper_half_expands_downwards() {
        let strip = (100, 50, 280, 28);
        assert_eq!(
            expanded_target(Some(strip), None, EXPANDED, None, &[FHD]),
            (100, 50, 280, 230)
        );
        assert_eq!(
            compact_target(
                Some((100, 50, 280, 230)),
                None,
                MINI_COMPACT_SIZE,
                None,
                &[FHD]
            ),
            strip
        );
    }

    #[test]
    fn expansion_stays_inside_the_work_area() {
        // Striscia sopra la barra delle applicazioni (dentro il monitor ma fuori dall'area di
        // lavoro) e oltre il bordo destro: il widget esteso resta nell'area di lavoro.
        let strip = (1700, 1050, 280, 28);
        assert_eq!(
            expanded_target(Some(strip), None, EXPANDED, None, &[FHD]),
            (1640, 810, 280, 230)
        );
        // Più alto del monitor: si ferma al bordo superiore.
        assert_eq!(
            expanded_target(Some(strip), None, (280.0, 2000.0), None, &[FHD]).1,
            0
        );
    }

    #[test]
    fn other_monitors_use_their_scale_and_work_area() {
        // Scala 1,5: striscia 420×42, widget 420×345.
        let strip = (4000, 1340, 420, 42);
        let r = expanded_target(Some(strip), None, EXPANDED, None, &[FHD, RIGHT]);
        assert_eq!(r, (4000, 1037, 420, 345));
        assert_eq!(
            compact_target(Some(r), None, MINI_COMPACT_SIZE, None, &[FHD, RIGHT]),
            strip
        );
        // Monitor a sinistra con la barra in alto: metà alta, il bordo superiore resta.
        let strip = (-400, 60, 350, 35);
        assert_eq!(
            expanded_target(Some(strip), None, EXPANDED, None, &[LEFT, FHD]),
            (-400, 60, 350, 288)
        );
        // Sopra la barra in alto: dentro l'area di lavoro.
        assert_eq!(
            expanded_target(Some((-400, 0, 350, 35)), None, EXPANDED, None, &[LEFT, FHD]).1,
            50
        );
    }

    #[test]
    fn compact_returns_exactly_to_the_saved_strip() {
        // Il widget esteso era stato spinto in su o spostato: la striscia torna dov'era.
        let widget = (1616, 600, 280, 230);
        assert_eq!(
            compact_target(
                Some(widget),
                Some((1616, 1012)),
                MINI_COMPACT_SIZE,
                None,
                &[FHD]
            ),
            (1616, 1012, 280, 28)
        );
        // Anche sopra la barra delle applicazioni: è dove l'utente l'ha messa.
        assert_eq!(
            compact_target(
                Some(widget),
                Some((1616, 1050)),
                MINI_COMPACT_SIZE,
                None,
                &[FHD]
            ),
            (1616, 1050, 280, 28)
        );
    }

    #[test]
    fn saved_strip_off_screen_is_recomputed_from_the_widget() {
        let widget = (3000, 1037, 420, 345);
        // Monitor di sinistra scollegato: la striscia salvata là non vale.
        assert_eq!(
            compact_target(
                Some(widget),
                Some((-400, 60)),
                MINI_COMPACT_SIZE,
                None,
                &[FHD, RIGHT]
            ),
            (3000, 1340, 420, 42)
        );
        // A metà fuori dal bordo destro: ricalcolata.
        assert_eq!(
            compact_target(
                Some((1616, 810, 280, 230)),
                Some((1800, 1012)),
                MINI_COMPACT_SIZE,
                None,
                &[FHD]
            ),
            (1616, 1012, 280, 28)
        );
        // All'apertura, senza widget: la salvata tenuta dentro l'area di lavoro.
        assert_eq!(
            compact_target(None, Some((1800, 1012)), MINI_COMPACT_SIZE, None, &[FHD]),
            (1640, 1012, 280, 28)
        );
    }

    #[test]
    fn saved_expanded_position_wins_while_on_screen() {
        let strip = (1616, 1012, 280, 28);
        assert_eq!(
            expanded_target(Some(strip), Some((200, 300)), EXPANDED, None, &[FHD]),
            (200, 300, 280, 230)
        );
        // Troppo in basso per l'altezza di adesso: sale nell'area di lavoro.
        assert_eq!(
            expanded_target(Some(strip), Some((200, 900)), EXPANDED, None, &[FHD]),
            (200, 810, 280, 230)
        );
        // Su un monitor scollegato: si ricava dalla striscia.
        assert_eq!(
            expanded_target(Some(strip), Some((3000, 300)), EXPANDED, None, &[FHD]),
            (1616, 810, 280, 230)
        );
    }

    #[test]
    fn refit_keeps_the_edge_near_the_taskbar() {
        // Metà bassa: cambia l'altezza, il bordo inferiore resta.
        assert_eq!(
            refit((1616, 810, 280, 230), (280.0, 300.0), &[FHD]),
            (1616, 740, 280, 300)
        );
        // Metà alta: resta il bordo superiore.
        assert_eq!(
            refit((100, 50, 280, 230), (280.0, 180.0), &[FHD]),
            (100, 50, 280, 180)
        );
        assert_eq!(refit((1, 2, 3, 4), (280.0, 180.0), &[]), (1, 2, 3, 4));
    }

    #[test]
    fn invisible_borders_are_part_of_the_outer_rect() {
        // Windows 11 a scala 1,25: 9 px di bordo invisibile ai lati, 12 in tutto in altezza.
        let screen = Screen {
            bounds: (0, 0, 1920, 1080),
            work: (0, 0, 1920, 1020),
            scale: 1.25,
        };
        let frame = (18, 12);
        let strip = (1544, 971, 368, 47);
        let r = expanded_target(Some(strip), None, sized(EXPANDED, frame), None, &[screen]);
        assert_eq!(r, (1544, 718, 368, 300));
        assert_eq!(r.1 + r.3 as i32, strip.1 + strip.3 as i32);
        assert_eq!(
            compact_target(
                Some(r),
                None,
                sized(MINI_COMPACT_SIZE, frame),
                None,
                &[screen]
            ),
            strip
        );
    }

    #[test]
    fn nearest_screen_when_none_contains_the_point() {
        assert_eq!(screen_for((5000, 100), &[FHD, RIGHT]), Some(&RIGHT));
        assert_eq!(screen_for((-10, 500), &[FHD, RIGHT]), Some(&FHD));
        assert_eq!(screen_for((0, 0), &[]), None);
        assert!(inside((0, 0, 1920, 1080), &[FHD]));
        assert!(!inside((1, 0, 1920, 1080), &[FHD]));
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
