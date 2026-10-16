//! Notifiche dell'app (F2, F18): decide il modulo `swapdeck_core::notify`, l'app le mostra solo
//! da mittente unico, cioè quando tiene il lock del motore (senza cartella dati non c'è lock e
//! le manda lei). Il giro del motore arriva da `engine_loop` prima dell'esito della lettura.

use std::path::Path;

use swapdeck_core::cswap::ListResponse;
use swapdeck_core::engine::{Decision, EngineRules, SwitchCause};
use swapdeck_core::headless::Step;
pub use swapdeck_core::notify::display_name;
use swapdeck_core::notify::{
    EngineContext, Notification, NotificationConfig, Notifier, SwitchDone, SwitchPreview,
};
use swapdeck_core::timeutil::now_unix;

/// Ultimo giro del motore visto dall'app.
#[derive(Debug, Default)]
struct EngineTick {
    holder: bool,
    rules: EngineRules,
    decision: Option<Decision>,
    /// Candidato del motore (preavviso di switch).
    preview: Option<SwitchPreview>,
    /// Cambio del giro, consumato dalla prima valutazione.
    switched: Option<SwitchDone>,
}

#[derive(Debug)]
pub struct AppNotifier {
    inner: Notifier,
    has_data_dir: bool,
    tick: EngineTick,
}

fn cause(d: &Decision) -> Option<SwitchCause> {
    match d {
        Decision::Switch { cause, .. } | Decision::ReturnToPrimary { cause, .. } => Some(*cause),
        _ => None,
    }
}

impl AppNotifier {
    pub fn new(data_dir: Option<&Path>) -> Self {
        Self {
            inner: Notifier::new(data_dir),
            has_data_dir: data_dir.is_some(),
            tick: EngineTick::default(),
        }
    }

    /// Giro del motore: titolarità del lock, decisione e cambio eseguito.
    pub fn on_step(&mut self, rules: &EngineRules, step: &Step) {
        let switched = step.switched_to.and_then(|to| {
            Some(SwitchDone {
                from: step.list.as_ref().ok()?.active_account_number,
                to,
                cause: cause(step.decision.as_ref()?)?,
            })
        });
        self.tick = EngineTick {
            holder: step.holder,
            rules: rules.clone(),
            decision: step.decision.clone(),
            preview: step.candidate.as_ref().map(SwitchPreview::from),
            switched: switched.or(self.tick.switched.take()),
        };
    }

    /// `true` se l'app è il mittente.
    pub fn is_sender(&self) -> bool {
        !self.has_data_dir || self.tick.holder
    }

    /// Notifiche di una lettura (`None` = fallita). Vuoto se il mittente è un altro.
    pub fn observe(
        &mut self,
        list: Option<&ListResponse>,
        (rules, profile): (&NotificationConfig, Option<&str>),
        installed: &mut dyn FnMut() -> Option<String>,
    ) -> Vec<Notification> {
        self.observe_at(list, (rules, profile), now_unix(), installed)
    }

    fn observe_at(
        &mut self,
        list: Option<&ListResponse>,
        (rules, profile): (&NotificationConfig, Option<&str>),
        now: i64,
        installed: &mut dyn FnMut() -> Option<String>,
    ) -> Vec<Notification> {
        if !self.is_sender() {
            self.tick.switched = None;
            return Vec::new();
        }
        let switched = self.tick.switched.take();
        let engine = self.has_data_dir.then_some(EngineContext {
            rules: &self.tick.rules,
            decision: self.tick.decision.as_ref(),
            switched,
            preview: self.tick.preview.as_ref(),
            profile,
        });
        self.inner.tick(list, engine, rules, now, installed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mock::fixture_list;
    use swapdeck_core::notify::NotificationKind;

    fn step(holder: bool, list: ListResponse, decision: Option<Decision>, to: Option<u32>) -> Step {
        Step {
            list: Ok(list),
            decision,
            reason: None,
            candidate: None,
            holder,
            switched_to: to,
        }
    }

    fn rules() -> NotificationConfig {
        let mut r = NotificationConfig::default();
        r.percent.five_hour = vec![50.0];
        r.stale.enabled = false;
        r.login.enabled = false;
        r.exhaustion.enabled = false;
        r.model_limit.enabled = false;
        r
    }

    #[test]
    fn only_the_lock_holder_sends() {
        let dir = tempfile::tempdir().unwrap();
        let mut n = AppNotifier::new(Some(dir.path()));
        let list = fixture_list();
        let mut none = || None;
        // Prima del primo giro (lettura veloce) e senza lock: niente.
        assert!(
            n.observe(Some(&list), (&rules(), None), &mut none)
                .is_empty()
        );
        n.on_step(
            &EngineRules::default(),
            &step(false, list.clone(), None, None),
        );
        assert!(
            n.observe(Some(&list), (&rules(), None), &mut none)
                .is_empty()
        );
        // Con il lock: soglie dell'attiva (fixture: alias6 al 66% sulle 5 ore).
        n.on_step(
            &EngineRules::default(),
            &step(true, list.clone(), None, None),
        );
        let out = n.observe(Some(&list), (&rules(), None), &mut none);
        assert_eq!(out.len(), 1, "{out:?}");
        assert!(out[0].title.it.starts_with("alias6"));
    }

    #[test]
    fn engine_switch_carries_the_cause_once() {
        let dir = tempfile::tempdir().unwrap();
        let mut n = AppNotifier::new(Some(dir.path()));
        let mut r = rules();
        r.percent.enabled = false;
        let before = fixture_list();
        let mut none = || None;
        n.on_step(
            &EngineRules::default(),
            &step(true, before.clone(), None, None),
        );
        assert!(n.observe(Some(&before), (&r, None), &mut none).is_empty());
        let d = Decision::Switch {
            from: 6,
            to: 3,
            cause: SwitchCause::PrimaryAtLimit,
        };
        n.on_step(
            &EngineRules::default(),
            &step(true, before.clone(), Some(d), Some(3)),
        );
        // Lista riletta dopo il cambio.
        let mut after = before.clone();
        after.active_account_number = Some(3);
        let out = n.observe(Some(&after), (&r, Some("Eco")), &mut none);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, NotificationKind::Switched);
        assert!(out[0].body.it.contains("al limite"), "{}", out[0].body.it);
        assert!(out[0].body.it.contains("Mappa: Eco"), "{}", out[0].body.it);
        assert!(
            n.observe(Some(&after), (&r, Some("Eco")), &mut none)
                .is_empty()
        );
    }

    #[test]
    fn without_data_dir_the_app_sends_and_failed_reads_count() {
        let mut n = AppNotifier::new(None);
        let mut r = rules();
        r.stale.enabled = true;
        r.stale.minutes = 1;
        let list = fixture_list();
        let mut none = || None;
        let t0 = now_unix();
        assert_eq!(
            n.observe_at(Some(&list), (&r, None), t0, &mut none).len(),
            1
        );
        let out = n.observe_at(None, (&r, None), t0 + 600, &mut none);
        assert!(
            out.iter().any(|x| x.kind == NotificationKind::Stale),
            "{out:?}"
        );
    }

    #[test]
    fn switch_preview_names_the_engine_candidate() {
        use swapdeck_core::engine::{Candidate, IdleReason, ReserveCriterion};
        let dir = tempfile::tempdir().unwrap();
        let mut n = AppNotifier::new(Some(dir.path()));
        let mut r = rules();
        r.percent.enabled = false;
        let list = fixture_list();
        let engine = EngineRules {
            enabled: true,
            primary: Some(6),
            five_hour_threshold: 70.0,
            ..EngineRules::default()
        };
        let mut s = step(
            true,
            list.clone(),
            Some(Decision::NoAction {
                reason: IdleReason::BelowThreshold,
            }),
            None,
        );
        s.candidate = Some(Candidate {
            number: 3,
            criterion: ReserveCriterion::LowestWeekly,
            fallback: false,
            why: "settimanale più bassa".into(),
        });
        n.on_step(&engine, &s);
        let mut none = || None;
        let out = n.observe(Some(&list), (&r, None), &mut none);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].kind, NotificationKind::SwitchPreview);
        assert!(
            out[0].body.it.contains("passo a alias3"),
            "{}",
            out[0].body.it
        );
    }
}
