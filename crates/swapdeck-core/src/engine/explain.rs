//! Motivi leggibili delle decisioni (italiano, per log, storico e tooltip). Gli account si
//! nominano con l'alias o con il numero, mai con l'email.

use super::{
    Account, Ctx, Decision, IdleReason, Pick, SuspendReason, SwitchCause, UsageStatus,
    effective_pct, limit_usage,
};

/// Motivo di un ritorno di ripiego (nessuna riserva libera).
pub(super) const FALLBACK_WHY: &str = "nessuna riserva libera: utenza precedente sotto soglia";

/// Percentuale senza decimali se intera, altrimenti con uno.
pub(super) fn pct(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

/// Durata breve: minuti, ore o giorni.
pub(super) fn duration(secs: i64) -> String {
    let secs = secs.max(0);
    match secs {
        s if s < 3600 => format!("{} min", (s + 59) / 60),
        s if s < 48 * 3600 => format!("{} h", s / 3600),
        s => format!("{} g", s / 86_400),
    }
}

pub(super) fn order_why(j: usize) -> String {
    format!("prima libera nella lista (posizione {})", j + 1)
}

fn label(ctx: &Ctx<'_>, number: u32) -> String {
    ctx.account(number)
        .and_then(|a| a.alias.as_deref())
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map_or_else(|| format!("#{number}"), str::to_owned)
}

/// «5h al 40%, 7d al 20%».
fn levels(ctx: &Ctx<'_>, a: Option<&Account>) -> String {
    let Some(u) = a.and_then(limit_usage) else {
        return "consumi sconosciuti".to_owned();
    };
    format!(
        "5h al {}%, 7d al {}%",
        pct(effective_pct(u.five_hour.as_ref(), ctx.now)),
        pct(effective_pct(u.seven_day.as_ref(), ctx.now))
    )
}

/// Perché l'utenza va lasciata: «5h di X al 95%», «7d di X al 98%», «X chiede un nuovo login».
fn trigger(ctx: &Ctx<'_>, number: u32) -> String {
    let name = label(ctx, number);
    let Some(a) = ctx.account(number) else {
        return name;
    };
    if a.usage_status == UsageStatus::ReloginRequired {
        return format!("{name} chiede un nuovo login");
    }
    if let Some(u) = limit_usage(a) {
        let p5 = effective_pct(u.five_hour.as_ref(), ctx.now);
        let p7 = effective_pct(u.seven_day.as_ref(), ctx.now);
        if p5 >= ctx.rules.five_hour_threshold {
            return format!("5h di {name} al {}%", pct(p5));
        }
        if p7 >= ctx.rules.seven_day_limit() {
            return format!("7d di {name} al {}%", pct(p7));
        }
    }
    format!("{name} al limite")
}

pub(super) fn reason(ctx: &Ctx<'_>, decision: &Decision, pick: Option<&Pick>) -> String {
    let active = ctx.list.active_account_number;
    let active_name = || active.map_or_else(|| "?".to_owned(), |n| label(ctx, n));
    let why = pick.map_or("", |p| p.why.as_str());
    match decision {
        Decision::NoAction { reason } => match reason {
            IdleReason::BelowThreshold => format!(
                "{} sotto soglia ({})",
                active_name(),
                levels(ctx, active.and_then(|n| ctx.account(n)))
            ),
            IdleReason::OnReserve => {
                let earlier = if ctx.flow() {
                    "nessuna utenza precedente è di nuovo libera".to_owned()
                } else {
                    format!("{} non è ancora di nuovo libera", label(ctx, ctx.order[0]))
                };
                format!("su {}: {earlier}", active_name())
            }
            IdleReason::Cooldown => {
                let left = ctx.mem.last_switch_at.map_or(0, |t| {
                    t.saturating_add(i64::from(ctx.rules.min_switch_gap_secs)) - ctx.now
                });
                format!("pausa minima tra due cambi: ancora {}", duration(left))
            }
            IdleReason::StaleUsage => format!(
                "consumi di {} sconosciuti o più vecchi di {} s",
                active_name(),
                ctx.rules.max_usage_age_secs
            ),
            IdleReason::ActiveExcluded => {
                format!(
                    "{} è tra gli esclusi: il motore non lo tocca",
                    active_name()
                )
            }
            IdleReason::ActiveOutsideRules => {
                format!("{} non è nella lista delle regole", active_name())
            }
        },
        Decision::Switch { from, to, cause } => match cause {
            SwitchCause::EarlierAvailable => format!(
                "{} viene prima nella lista ed è di nuovo libera ({}): passo da {} a {}",
                label(ctx, *to),
                levels(ctx, ctx.account(*to)),
                label(ctx, *from),
                label(ctx, *to)
            ),
            _ => format!(
                "{}: passo a {}, {why}",
                trigger(ctx, *from),
                label(ctx, *to)
            ),
        },
        Decision::ReturnToPrimary { from, to, cause } => match cause {
            SwitchCause::PrimaryReset => format!(
                "reset di {} passato ({}): torno alla principale",
                label(ctx, *to),
                levels(ctx, ctx.account(*to))
            ),
            _ => format!(
                "{}, nessuna riserva libera: torno a {}, la principale, sotto soglia ({})",
                trigger(ctx, *from),
                label(ctx, *to),
                levels(ctx, ctx.account(*to))
            ),
        },
        Decision::Suspended { reason, detail } => {
            let detail = detail.join("; ");
            match reason {
                SuspendReason::Disabled => "motore spento".to_owned(),
                SuspendReason::InvalidRules => format!("regole non valide: {detail}"),
                SuspendReason::Conflict => format!("altro motore attivo: {detail}"),
                SuspendReason::NoActiveAccount => "nessun account attivo".to_owned(),
                SuspendReason::NoViableTarget => format!(
                    "{}: nessuna utenza utilizzabile",
                    active.map_or_else(|| "?".to_owned(), |n| trigger(ctx, n))
                ),
            }
        }
    }
}
