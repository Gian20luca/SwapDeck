//! Valutazione pura: stessi ingressi e stessa memoria, stesse notifiche. Niente I/O.

use super::text::{self, Texts, Win};
use super::{
    EngineContext, Notification, NotificationConfig, NotificationKind, NotifyInput, NotifyMemory,
    NotifyScope, Severity, display_name,
};
use crate::cswap::{Account, ListResponse, UsageStatus, UsageWindow};
use crate::engine::{Decision, IdleReason, SuspendReason, SwitchCause};
use crate::history::HistoryRecord;
use crate::timeutil::parse_iso;

/// Calo della % che indica una finestra nuova anche senza `resetsAt`.
const RESET_DROP_PCT: f64 = 10.0;
/// Dopo una pausa più lunga i cambi di account visti non si notificano (erano già noti).
const EDGE_GAP_SECS: i64 = 3600;
/// Un reset avvenuto si annuncia solo se è recente.
const RESET_DONE_MAX_AGE: i64 = 3600;
/// Il riepilogo si recupera fino a due ore dopo l'ora scelta (app aperta dopo).
const SUMMARY_CATCH_UP: i64 = 2 * 3600;
/// Le chiavi più vecchie si dimenticano (la finestra più lunga è di 7 giorni).
const SENT_RETENTION: i64 = 8 * 86_400;

/// Stato del riepilogo giornaliero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryState {
    NotDue,
    /// Dovuto: storico da leggere tra `from` e `to` (secondi Unix).
    Due {
        from: i64,
        to: i64,
    },
    /// Ora passata da troppo (app chiusa): si salta il giorno.
    Missed,
}

fn local_day(now: i64, offset: i32) -> (i64, i64) {
    let local = now + i64::from(offset);
    (local.div_euclid(86_400), local.rem_euclid(86_400))
}

/// Il riepilogo va fatto adesso? (Il mittente legge lo storico solo in questo caso.)
pub fn summary_state(
    rules: &NotificationConfig,
    mem: &NotifyMemory,
    now: i64,
    offset: i32,
) -> SummaryState {
    let r = &rules.daily_summary;
    let Some(at) = super::parse_hhmm(&r.time) else {
        return SummaryState::NotDue;
    };
    let (day, secs) = local_day(now, offset);
    let target = i64::from(at) * 60;
    if !r.enabled || mem.summary_day == Some(day) || secs < target {
        return SummaryState::NotDue;
    }
    if secs - target > SUMMARY_CATCH_UP {
        return SummaryState::Missed;
    }
    SummaryState::Due {
        from: day * 86_400 - i64::from(offset),
        to: now,
    }
}

struct Eval<'a> {
    rules: &'a NotificationConfig,
    mem: &'a mut NotifyMemory,
    now: i64,
    offset: i32,
    out: Vec<Notification>,
}

fn reset_key(w: &UsageWindow) -> String {
    w.resets_at
        .as_deref()
        .map_or_else(|| "-".into(), |s| s.chars().take(16).collect())
}

fn windows(a: &Account) -> Vec<(Win, &UsageWindow)> {
    let Some(u) = &a.usage else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(w) = &u.five_hour {
        out.push((Win::FiveHour, w));
    }
    if let Some(w) = &u.seven_day {
        out.push((Win::SevenDay, w));
    }
    for w in u.scoped.iter().flatten() {
        if let Some(n) = w.name.as_deref().filter(|n| !n.trim().is_empty()) {
            out.push((Win::Model(n.trim().to_owned()), w));
        }
    }
    out
}

fn in_scope(scope: NotifyScope, account: u32, active: Option<u32>) -> bool {
    scope == NotifyScope::All || Some(account) == active
}

/// Istante del dato di consumo (da `usageAgeSeconds` o `usageFetchedAt`; assente = adesso).
fn data_time(a: &Account, now: i64) -> i64 {
    if let Some(age) = a.usage_age_seconds.filter(|v| v.is_finite() && *v >= 0.0) {
        return now - age as i64;
    }
    a.usage_fetched_at
        .as_deref()
        .and_then(parse_iso)
        .map_or(now, |t| t.min(now))
}

/// Secondi al raggiungimento di `threshold` al ritmo medio della finestra.
fn eta_secs(w: &UsageWindow, len: i64, threshold: f64, now: i64) -> Option<i64> {
    let reset = w.resets_at.as_deref().and_then(parse_iso)?;
    let left = reset - now;
    let elapsed = len - left;
    if left <= 0 || elapsed < 600 || w.pct <= 0.0 || w.pct >= threshold {
        return None;
    }
    let rate = w.pct / elapsed as f64;
    let eta = ((threshold - w.pct) / rate).ceil() as i64;
    (eta < left).then_some(eta)
}

impl Eval<'_> {
    fn name(&self, n: u32) -> String {
        self.mem
            .names
            .get(&n)
            .cloned()
            .unwrap_or_else(|| format!("#{n}"))
    }

    fn sent(&self, key: &str) -> bool {
        self.mem.sent.contains_key(key)
    }

    fn push(
        &mut self,
        kind: NotificationKind,
        severity: Severity,
        key: String,
        account: Option<u32>,
        (title, body): Texts,
    ) {
        self.mem.sent.insert(key.clone(), self.now);
        self.out.push(Notification {
            kind,
            severity,
            key,
            account,
            title,
            body,
        });
    }

    /// Una volta per chiave.
    fn once(
        &mut self,
        kind: NotificationKind,
        severity: Severity,
        key: String,
        account: Option<u32>,
        texts: impl FnOnce() -> Texts,
    ) {
        if !self.sent(&key) {
            self.push(kind, severity, key, account, texts());
        }
    }

    /// Avviso legato a una condizione: una volta finché dura, di nuovo se torna dopo essere
    /// finita.
    fn latch(
        &mut self,
        cond: bool,
        (kind, severity): (NotificationKind, Severity),
        key: String,
        account: Option<u32>,
        texts: impl FnOnce() -> Texts,
    ) {
        if !cond {
            self.mem.sent.remove(&key);
        } else if self.sent(&key) {
            // Rinfresca l'istante: una condizione lunga non scade con la potatura.
            self.mem.sent.insert(key, self.now);
        } else {
            self.push(kind, severity, key, account, texts());
        }
    }

    fn switches(&mut self, list: &ListResponse, engine: Option<&EngineContext<'_>>) {
        let continuity = self
            .mem
            .updated_at
            .is_some_and(|t| self.now - t <= EDGE_GAP_SECS);
        let active = list.active_account_number;
        let profile = engine.and_then(|e| e.profile);
        if let Some(sw) = engine.and_then(|e| e.switched) {
            let from = sw.from.map(|n| self.name(n));
            let to = self.name(sw.to);
            let key = format!("switch:{}:{}", sw.to, self.now);
            // Ritorno alla prima della lista dopo un reset (anche dal criterio «ordine»).
            let first = engine.and_then(|e| {
                e.rules
                    .primary
                    .or_else(|| e.rules.order.as_ref()?.first().copied())
            });
            let returned = sw.cause == SwitchCause::PrimaryReset
                || (sw.cause == SwitchCause::EarlierAvailable && first == Some(sw.to));
            if returned {
                if self.rules.returned_to_primary.enabled {
                    self.push(
                        NotificationKind::ReturnedToPrimary,
                        Severity::Info,
                        key,
                        Some(sw.to),
                        text::returned(&to, profile),
                    );
                }
            } else if self.rules.switched.enabled {
                self.push(
                    NotificationKind::Switched,
                    Severity::Info,
                    key,
                    Some(sw.to),
                    text::switched(from.as_deref(), &to, Some(sw.cause), profile),
                );
            }
            self.mem.last_active = Some(sw.to);
        } else {
            if self.mem.seen_list
                && continuity
                && active != self.mem.last_active
                && let Some(to) = active
                && self.rules.switched.enabled
            {
                let from = self.mem.last_active.map(|n| self.name(n));
                let name = self.name(to);
                self.push(
                    NotificationKind::Switched,
                    Severity::Info,
                    format!("switch:{to}:{}", self.now),
                    Some(to),
                    text::switched(from.as_deref(), &name, None, profile),
                );
            }
            self.mem.last_active = active;
        }
        self.mem.seen_list = true;
    }

    /// Reset passati dall'ultima valutazione (prima di leggere i nuovi `resetsAt`).
    fn resets_done(&mut self) {
        let now = self.now;
        let due: Vec<(String, i64)> = self
            .mem
            .resets
            .iter()
            .filter(|(_, r)| **r <= now)
            .map(|(k, r)| (k.clone(), *r))
            .collect();
        let rule = self.rules.reset_done.clone();
        for (id, r) in due {
            self.mem.resets.remove(&id);
            let Some((n, win)) = parse_id(&id) else {
                continue;
            };
            if rule.enabled
                && in_scope(rule.scope, n, self.mem.last_active)
                && now - r <= RESET_DONE_MAX_AGE
            {
                let name = self.name(n);
                self.once(
                    NotificationKind::ResetDone,
                    Severity::Info,
                    format!("resetdone:{id}:{r}"),
                    Some(n),
                    || text::reset_done(&name, &win),
                );
            }
        }
    }

    fn account(&mut self, a: &Account, active: Option<u32>) {
        let n = a.number;
        if a.usage.is_some() {
            let t = data_time(a, self.now);
            let e = self.mem.fresh_at.entry(n).or_insert(t);
            *e = (*e).max(t);
        }
        for (win, w) in windows(a) {
            self.window(a, &win, w, active);
        }
        let login = self.rules.login.clone();
        if login.enabled && in_scope(login.scope, n, active) {
            let name = display_name(a);
            let relogin = a.usage_status == UsageStatus::ReloginRequired;
            self.latch(
                relogin,
                (NotificationKind::Login, Severity::Critical),
                format!("login:{n}:relogin"),
                Some(n),
                || text::relogin(&name),
            );
            if let Some(e) = a.login_expires_at.as_deref().and_then(parse_iso)
                && !relogin
                && e - self.now <= i64::from(login.days_before) * 86_400
            {
                let at = text::clock(e, self.now, self.offset);
                let left = e - self.now;
                self.once(
                    NotificationKind::Login,
                    Severity::Critical,
                    format!("login:{n}:exp:{}", e / 60),
                    Some(n),
                    || text::login_expiring(&name, left, &at),
                );
            }
        }
    }

    fn window(&mut self, a: &Account, win: &Win, w: &UsageWindow, active: Option<u32>) {
        let n = a.number;
        let id = format!("{n}:{}", win.id());
        let pct = w.pct;
        if !pct.is_finite() {
            return;
        }
        if self
            .mem
            .pct
            .get(&id)
            .is_some_and(|prev| pct + RESET_DROP_PCT < *prev)
        {
            let tag = format!(":{id}:");
            self.mem.sent.retain(|k, _| !k.contains(&tag));
        }
        self.mem.pct.insert(id.clone(), pct);
        let rkey = reset_key(w);
        let name = display_name(a);
        let r = self.rules;
        let (kind, list, rule_on, scope) = match win {
            Win::FiveHour => (
                NotificationKind::Threshold,
                &r.percent.five_hour,
                r.percent.enabled,
                r.percent.scope,
            ),
            Win::SevenDay => (
                NotificationKind::Threshold,
                &r.percent.seven_day,
                r.percent.enabled,
                r.percent.scope,
            ),
            Win::Model(_) => (
                NotificationKind::ModelLimit,
                &r.model_limit.thresholds,
                r.model_limit.enabled,
                r.model_limit.scope,
            ),
        };
        if rule_on && in_scope(scope, n, active) {
            let crossed: Vec<f64> = list.iter().copied().filter(|t| *t <= pct).collect();
            let top = list.last().copied();
            if let Some(&highest) = crossed.last() {
                let key = |t: f64| format!("pct:{id}:{}:{rkey}", text::num(t));
                if !self.sent(&key(highest)) {
                    let sev = if Some(highest) == top {
                        Severity::Warning
                    } else {
                        Severity::Info
                    };
                    self.push(
                        kind,
                        sev,
                        key(highest),
                        Some(n),
                        text::threshold(&name, win, highest, pct),
                    );
                }
                for t in crossed {
                    self.mem.sent.entry(key(t)).or_insert(self.now);
                }
            }
        }
        let reset = w.resets_at.as_deref().and_then(parse_iso);
        let ex = &r.exhaustion;
        if ex.enabled
            && *win != Win::FiveHour
            && in_scope(ex.scope, n, active)
            && let Some(pe) = w.projected_exhaustion_at.as_deref().and_then(parse_iso)
            && pe > self.now
            && reset.is_none_or(|r| pe < r)
        {
            let at = text::clock(pe, self.now, self.offset);
            let rs = reset.map_or_else(|| "?".into(), |r| text::clock(r, self.now, self.offset));
            self.once(
                NotificationKind::Exhaustion,
                Severity::Warning,
                format!("exhaust:{id}:{rkey}"),
                Some(n),
                || text::exhaustion(&name, win, &at, &rs),
            );
        }
        if let Some(rt) = reset.filter(|rt| *rt > self.now) {
            let left = rt - self.now;
            let rs = &r.reset_soon;
            if rs.enabled
                && in_scope(rs.scope, n, active)
                && pct > 0.0
                && left <= i64::from(rs.minutes_before) * 60
            {
                let at = text::clock(rt, self.now, self.offset);
                let minutes = (left + 59) / 60;
                self.once(
                    NotificationKind::ResetSoon,
                    Severity::Info,
                    format!("resetsoon:{id}:{rkey}"),
                    Some(n),
                    || text::reset_soon(&name, win, minutes, &at),
                );
            }
            self.mem.resets.insert(id, rt);
        }
    }

    fn stale(&mut self) {
        let rule = self.rules.stale.clone();
        let limit = i64::from(rule.minutes) * 60;
        let entries: Vec<(u32, i64)> = self.mem.fresh_at.iter().map(|(k, v)| (*k, *v)).collect();
        for (n, t) in entries {
            let cond = rule.enabled
                && in_scope(rule.scope, n, self.mem.last_active)
                && self.now - t > limit;
            let name = self.name(n);
            let minutes = (self.now - t) / 60;
            self.latch(
                cond,
                (NotificationKind::Stale, Severity::Warning),
                format!("stale:{n}"),
                Some(n),
                || text::stale(&name, minutes),
            );
        }
    }

    fn engine(&mut self, ctx: &EngineContext<'_>, list: Option<&ListResponse>) {
        let Some(decision) = ctx.decision else {
            return;
        };
        let on = self.rules.engine_state.enabled;
        match decision {
            Decision::Suspended { reason, detail } => match reason {
                SuspendReason::Disabled => {
                    self.mem.engine_paused = None;
                    self.mem.sent.remove("noreserve");
                }
                SuspendReason::NoViableTarget => {
                    self.mem.engine_paused = Some("noViableTarget".into());
                    let nearest = list.and_then(|l| self.nearest_reset(l));
                    let on = self.rules.no_reserve.enabled;
                    self.latch(
                        on,
                        (NotificationKind::NoReserve, Severity::Critical),
                        "noreserve".into(),
                        None,
                        || {
                            text::no_reserve(
                                nearest
                                    .as_ref()
                                    .map(|(name, at)| (name.as_str(), at.clone())),
                            )
                        },
                    );
                }
                other => {
                    let key = serde_json::to_value(other)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_default();
                    self.mem.sent.remove("noreserve");
                    if self.mem.engine_paused.as_deref() != Some(key.as_str()) {
                        self.mem.engine_paused = Some(key.clone());
                        if on {
                            self.push(
                                NotificationKind::EngineSuspended,
                                Severity::Warning,
                                format!("paused:{key}:{}", self.now),
                                None,
                                text::engine_suspended(*other, detail),
                            );
                        }
                    }
                }
            },
            _ => {
                self.mem.sent.remove("noreserve");
                if self.mem.engine_paused.take().is_some() && on {
                    self.push(
                        NotificationKind::EngineResumed,
                        Severity::Info,
                        format!("resumed:{}", self.now),
                        None,
                        text::engine_resumed(),
                    );
                }
            }
        }
        if let Some(list) = list {
            self.preview(ctx, decision, list);
        }
    }

    /// Reset più vicino tra le utenze abilitate: (nome, ora locale).
    fn nearest_reset(&self, list: &ListResponse) -> Option<(String, String)> {
        list.accounts
            .iter()
            .filter(|a| !a.is_disabled())
            .flat_map(|a| {
                let u = a.usage.as_ref().or(a.last_good_usage.as_ref());
                [
                    u.and_then(|u| u.five_hour.as_ref()),
                    u.and_then(|u| u.seven_day.as_ref()),
                ]
                .into_iter()
                .flatten()
                .filter_map(|w| w.resets_at.as_deref().and_then(parse_iso))
                .filter(|r| *r > self.now)
                .map(move |r| (r, a))
            })
            .min_by_key(|(r, _)| *r)
            .map(|(r, a)| (display_name(a), text::clock(r, self.now, self.offset)))
    }

    fn preview(&mut self, ctx: &EngineContext<'_>, decision: &Decision, list: &ListResponse) {
        let rule = self.rules.switch_preview.clone();
        let er = ctx.rules;
        let idle = matches!(
            decision,
            Decision::NoAction {
                reason: IdleReason::BelowThreshold | IdleReason::OnReserve
            }
        );
        if !rule.enabled || !er.enabled || er.dry_run || !idle {
            return;
        }
        let Some(a) = list
            .active_account_number
            .and_then(|n| list.accounts.iter().find(|a| a.number == n))
        else {
            return;
        };
        let Some(u) = &a.usage else {
            return;
        };
        let target = ctx.preview.and_then(|p| p.target).map(|t| self.name(t));
        for (win, w, t) in [
            (Win::FiveHour, u.five_hour.as_ref(), er.five_hour_threshold),
            (
                Win::SevenDay,
                u.seven_day.as_ref(),
                er.seven_day_threshold.unwrap_or(100.0),
            ),
        ] {
            let Some(w) = w else { continue };
            if !w.pct.is_finite() || w.pct >= t {
                continue;
            }
            let eta = (rule.minutes_before > 0)
                .then(|| eta_secs(w, win.len_secs(), t, self.now))
                .flatten()
                .filter(|e| *e <= i64::from(rule.minutes_before) * 60);
            let by_points = rule.points_before > 0.0 && t - w.pct <= rule.points_before;
            if eta.is_none() && !by_points {
                continue;
            }
            let key = format!("preview:{}:{}:{}", a.number, win.id(), reset_key(w));
            if self.sent(&key) {
                continue;
            }
            let name = display_name(a);
            let texts = text::preview(
                &name,
                target.as_deref(),
                &win,
                (w.pct, t),
                eta.map(|e| ((e + 59) / 60).max(1)),
            );
            self.push(
                NotificationKind::SwitchPreview,
                Severity::Warning,
                key,
                Some(a.number),
                texts,
            );
            return;
        }
    }

    fn summary(&mut self, history: Option<&[HistoryRecord]>) {
        match summary_state(self.rules, self.mem, self.now, self.offset) {
            SummaryState::NotDue => {}
            SummaryState::Missed => {
                self.mem.summary_day = Some(local_day(self.now, self.offset).0);
            }
            SummaryState::Due { from, to } => {
                let Some(h) = history else {
                    return;
                };
                let (rows, switches) = day_usage(h, from, to);
                let rows: Vec<(String, f64, f64)> = rows
                    .into_iter()
                    .map(|(n, f, s)| (self.name(n), f, s))
                    .collect();
                let day = local_day(self.now, self.offset).0;
                self.mem.summary_day = Some(day);
                self.push(
                    NotificationKind::DailySummary,
                    Severity::Info,
                    format!("summary:{day}"),
                    None,
                    text::summary(&rows, switches),
                );
            }
        }
    }

    fn quiet(&self) -> bool {
        let q = &self.rules.quiet_hours;
        let (Some(s), Some(e)) = (super::parse_hhmm(&q.start), super::parse_hhmm(&q.end)) else {
            return false;
        };
        if !q.enabled || s == e {
            return false;
        }
        let m = (local_day(self.now, self.offset).1 / 60) as u32;
        if s < e {
            (s..e).contains(&m)
        } else {
            m >= s || m < e
        }
    }
}

fn parse_id(id: &str) -> Option<(u32, Win)> {
    let (n, w) = id.split_once(':')?;
    let n = n.parse().ok()?;
    let win = match w {
        "5h" => Win::FiveHour,
        "7d" => Win::SevenDay,
        m => Win::Model(m.strip_prefix("m-")?.to_owned()),
    };
    Some((n, win))
}

/// Consumo del giorno per account (somma degli aumenti di 5h e 7d tra campioni: un calo è un
/// reset e conta il valore nuovo) e numero di cambi riusciti.
pub(super) fn day_usage(h: &[HistoryRecord], from: i64, to: i64) -> (Vec<(u32, f64, f64)>, usize) {
    use std::collections::BTreeMap;
    let mut last: BTreeMap<u32, (Option<f64>, Option<f64>)> = BTreeMap::new();
    let mut total: BTreeMap<u32, (f64, f64)> = BTreeMap::new();
    let mut switches = 0;
    let inc = |prev: Option<f64>, cur: Option<f64>| match (prev, cur) {
        (Some(p), Some(c)) if c >= p => c - p,
        (Some(_), Some(c)) => c,
        _ => 0.0,
    };
    let mut records: Vec<&HistoryRecord> =
        h.iter().filter(|r| (from..=to).contains(&r.ts())).collect();
    records.sort_by_key(|r| r.ts());
    for r in records {
        match r {
            HistoryRecord::Sample(s) => {
                let (p5, p7) = last.get(&s.account).copied().unwrap_or((None, None));
                let t = total.entry(s.account).or_insert((0.0, 0.0));
                t.0 += inc(p5, s.five_hour_pct);
                t.1 += inc(p7, s.seven_day_pct);
                last.insert(s.account, (s.five_hour_pct.or(p5), s.seven_day_pct.or(p7)));
            }
            HistoryRecord::Switch(s) if s.switched => switches += 1,
            _ => {}
        }
    }
    let rows = total
        .into_iter()
        .filter(|(_, (f, s))| *f >= 0.5 || *s >= 0.5)
        .map(|(n, (f, s))| (n, f, s))
        .collect();
    (rows, switches)
}

/// Valuta un giro: restituisce le notifiche da mostrare e aggiorna `mem`. Pura (niente I/O,
/// niente orologio): l'ora è `now`, l'ora locale viene da `input.local_offset_secs`.
pub fn evaluate(
    input: &NotifyInput<'_>,
    rules: &NotificationConfig,
    mem: &mut NotifyMemory,
    now: i64,
) -> Vec<Notification> {
    let mut ev = Eval {
        rules,
        mem,
        now,
        offset: input.local_offset_secs,
        out: Vec::new(),
    };
    if let Some(list) = input.list {
        for a in &list.accounts {
            ev.mem.names.insert(a.number, display_name(a));
        }
    }
    ev.resets_done();
    if let Some(list) = input.list {
        ev.switches(list, input.engine.as_ref());
        let active = list.active_account_number;
        for a in list.accounts.iter().filter(|a| !a.is_disabled()) {
            ev.account(a, active);
        }
        // Account tolti o disattivati: niente memoria vecchia.
        let alive = |n: u32| {
            list.accounts
                .iter()
                .any(|a| a.number == n && !a.is_disabled())
        };
        ev.mem.fresh_at.retain(|n, _| alive(*n));
        ev.mem
            .names
            .retain(|n, _| list.accounts.iter().any(|a| a.number == *n));
        let keep = |id: &String| parse_id(id).is_some_and(|(n, _)| alive(n));
        ev.mem.pct.retain(|k, _| keep(k));
        ev.mem.resets.retain(|k, _| keep(k));
    }
    ev.stale();
    if let Some(ctx) = input.engine.as_ref() {
        ev.engine(ctx, input.list);
    }
    if ev.rules.updates.enabled
        && let Some(u) = input.update
        && u.update_available
        && !u.dismissed
    {
        let (latest, installed) = (u.latest.clone(), u.installed.clone());
        ev.once(
            NotificationKind::Update,
            Severity::Info,
            format!("update:{latest}"),
            None,
            || text::update(installed.as_deref(), &latest),
        );
    }
    ev.summary(input.history);
    let quiet = ev.quiet();
    let allow_critical = ev.rules.quiet_hours.allow_critical;
    ev.mem
        .sent
        .retain(|_, t| now.saturating_sub(*t) <= SENT_RETENTION);
    ev.mem.updated_at = Some(now);
    let mut out = ev.out;
    if quiet {
        // Scartati ma già segnati: alla fine della fascia non arrivano tutti insieme.
        out.retain(|n| allow_critical && n.severity == Severity::Critical);
    }
    out
}
