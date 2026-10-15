//! Icona dell'area di notifica disegnata a runtime: un anello che si riempie in senso orario secondo
//! la % delle 5 ore dell'account attivo. Verde, giallo dall'80%, rosso dal 95%, grigio senza dati;
//! con il motore sospeso al centro compare il simbolo di pausa.

/// Soglie dei colori dell'icona (piano: giallo all'80%, rosso al 95%).
pub const WARN_PCT: f64 = 80.0;
pub const CRITICAL_PCT: f64 = 95.0;
/// Lato dell'icona in pixel (il sistema la scala per l'area di notifica).
pub const SIZE: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
    Critical,
    Unknown,
}

impl Level {
    pub fn of(pct: Option<f64>) -> Self {
        match pct {
            Some(p) if p.is_finite() && p >= CRITICAL_PCT => Self::Critical,
            Some(p) if p.is_finite() && p >= WARN_PCT => Self::Warn,
            Some(p) if p.is_finite() => Self::Ok,
            _ => Self::Unknown,
        }
    }

    pub fn rgb(self) -> [u8; 3] {
        match self {
            Self::Ok => [34, 160, 90],
            Self::Warn => [230, 170, 0],
            Self::Critical => [220, 50, 40],
            Self::Unknown => [140, 144, 152],
        }
    }
}

/// Colore della parte vuota dell'anello.
const TRACK: [u8; 4] = [140, 144, 152, 90];

/// Immagine RGBA non premoltiplicata, `SIZE`×`SIZE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

#[cfg(test)]
impl Rgba {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.data[i],
            self.data[i + 1],
            self.data[i + 2],
            self.data[i + 3],
        ]
    }
}

fn coverage(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

/// Disegna l'icona. `pct` è la % delle 5 ore (None = nessun dato), `paused` la variante «sospeso».
pub fn render(pct: Option<f64>, paused: bool) -> Rgba {
    let size = SIZE as f64;
    let center = size / 2.0;
    let outer = center - 1.0;
    let inner = outer - 5.0;
    let level = Level::of(pct);
    let [r, g, b] = level.rgb();
    let filled = match (level, pct) {
        (Level::Unknown, _) | (_, None) => 1.0,
        (_, Some(p)) => (p / 100.0).clamp(0.0, 1.0),
    };
    let mut data = vec![0u8; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f64 + 0.5 - center;
            let dy = y as f64 + 0.5 - center;
            let d = (dx * dx + dy * dy).sqrt();
            let ring = coverage(outer - d + 0.5) * coverage(d - inner + 0.5);
            // Angolo da mezzogiorno in senso orario, in frazione di giro [0, 1).
            let turn = dx.atan2(-dy).rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU;
            let mut px = [0u8; 4];
            if ring > 0.0 {
                px = if turn <= filled {
                    [r, g, b, (255.0 * ring).round() as u8]
                } else {
                    [
                        TRACK[0],
                        TRACK[1],
                        TRACK[2],
                        (TRACK[3] as f64 * ring).round() as u8,
                    ]
                };
            }
            if paused {
                // Due barre verticali al centro.
                let bar = |cx: f64| {
                    coverage(1.25 - (dx - cx).abs() + 0.5) * coverage(5.0 - dy.abs() + 0.5)
                };
                let a = bar(-2.75).max(bar(2.75));
                if a > 0.0 {
                    px = [r, g, b, (255.0 * a).round() as u8];
                }
            }
            let i = ((y * SIZE + x) * 4) as usize;
            data[i..i + 4].copy_from_slice(&px);
        }
    }
    Rgba {
        width: SIZE,
        height: SIZE,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOP: (u32, u32) = (16, 3);
    const LEFT: (u32, u32) = (3, 16);
    const CENTER: (u32, u32) = (16, 16);

    fn rgb(px: [u8; 4]) -> [u8; 3] {
        [px[0], px[1], px[2]]
    }

    #[test]
    fn levels_follow_thresholds() {
        assert_eq!(Level::of(Some(0.0)), Level::Ok);
        assert_eq!(Level::of(Some(79.9)), Level::Ok);
        assert_eq!(Level::of(Some(80.0)), Level::Warn);
        assert_eq!(Level::of(Some(94.9)), Level::Warn);
        assert_eq!(Level::of(Some(95.0)), Level::Critical);
        assert_eq!(Level::of(Some(120.0)), Level::Critical);
        assert_eq!(Level::of(None), Level::Unknown);
        assert_eq!(Level::of(Some(f64::NAN)), Level::Unknown);
    }

    #[test]
    fn arc_fills_clockwise_from_top() {
        let half = render(Some(50.0), false);
        assert_eq!(half.data.len(), (SIZE * SIZE * 4) as usize);
        let top = half.pixel(TOP.0, TOP.1);
        assert_eq!(rgb(top), Level::Ok.rgb());
        assert_eq!(top[3], 255);
        // A 3/4 di giro (sinistra) l'anello è ancora vuoto al 50%.
        let left = half.pixel(LEFT.0, LEFT.1);
        assert_eq!(left, [TRACK[0], TRACK[1], TRACK[2], TRACK[3]]);
        let full = render(Some(85.0), false);
        assert_eq!(rgb(full.pixel(LEFT.0, LEFT.1)), Level::Warn.rgb());
        assert_eq!(full.pixel(CENTER.0, CENTER.1)[3], 0, "centro trasparente");
        assert_eq!(full.pixel(0, 0)[3], 0, "angolo trasparente");
    }

    #[test]
    fn critical_and_unknown_colors() {
        assert_eq!(
            rgb(render(Some(97.0), false).pixel(TOP.0, TOP.1)),
            Level::Critical.rgb()
        );
        let none = render(None, false);
        assert_eq!(rgb(none.pixel(TOP.0, TOP.1)), Level::Unknown.rgb());
        assert_eq!(rgb(none.pixel(LEFT.0, LEFT.1)), Level::Unknown.rgb());
    }

    #[test]
    fn paused_variant_draws_bars() {
        let paused = render(Some(40.0), true);
        let bar = paused.pixel(13, 16);
        assert_eq!(rgb(bar), Level::Ok.rgb());
        assert_eq!(bar[3], 255);
        assert_eq!(
            paused.pixel(CENTER.0, CENTER.1)[3],
            0,
            "spazio tra le barre"
        );
        assert_ne!(paused, render(Some(40.0), false));
    }
}
