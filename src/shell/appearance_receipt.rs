// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Receipts observe the production hosts. Selection still belongs to Settings.

use cambium_genet_winit_host::{CloseDisposition, HostHooks};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};
use tabard_workshop::{WorkshopState, WorkshopView, native_host::WorkshopLogic};

pub(super) struct WorkshopReceipt {
    pub completion: Rc<Cell<Option<bool>>>,
    sheet: String,
}

impl mesquite::Product for WorkshopReceipt {
    type State = WorkshopState;
    type Logic = WorkshopLogic;
    type View = WorkshopView;
    const KIND: &'static str = "turnstone-theme-workshop";
    const SURFACE: &'static str = "app";
    const LOG_PREFIX: &'static str = "turnstone-theme";
    fn sheet(&self) -> &str {
        &self.sheet
    }
    fn snapshot(&self, ctx: &mesquite::Ctx<'_, Self>, _: usize, _: f32) -> taproot::ProbeSnapshot {
        let state = ctx.runner.state();
        taproot::ProbeSnapshot::default()
            .with_field("theme", state.draft_theme().id.clone())
            .with_field("name", state.draft_theme().name.clone())
            .with_field("mode", state.mode_key())
            .with_field("dirty", state.is_dirty().to_string())
            .with_field("status", state.status())
            .with_field(
                "preview-errors",
                state.stylesheet_preview().borrow().diagnostics().join(" | "),
            )
    }
    fn busy_mut(&mut self, _: &mut mesquite::Ctx<'_, Self>, capture_pending: bool) -> Option<bool> {
        Some(capture_pending)
    }
    fn complete(
        &mut self,
        _: &mut mesquite::Ctx<'_, Self>,
        outcome: &taproot::Outcome,
    ) -> Result<(), String> {
        self.completion.set(Some(outcome.ok));
        Ok(())
    }
}

pub(super) fn workshop_hooks(
    hooks: HostHooks<WorkshopState, WorkshopLogic, WorkshopView>,
) -> Result<
    (
        HostHooks<WorkshopState, WorkshopLogic, WorkshopView>,
        Option<Rc<Cell<Option<bool>>>>,
    ),
    String,
> {
    workshop_hooks_config(hooks, mesquite::LaneConfig::from_env("TURNSTONE_THEME"))
}

fn workshop_hooks_config(
    mut hooks: HostHooks<WorkshopState, WorkshopLogic, WorkshopView>,
    config: Option<mesquite::LaneConfig>,
) -> Result<
    (
        HostHooks<WorkshopState, WorkshopLogic, WorkshopView>,
        Option<Rc<Cell<Option<bool>>>>,
    ),
    String,
> {
    let Some(config) = config else {
        return Ok((hooks, None));
    };
    let completion = Rc::new(Cell::new(None));
    let lane = mesquite::Lane::from_config(
        config,
        WorkshopReceipt {
            completion: completion.clone(),
            sheet: tabard_workshop::workshop_stylesheet(),
        },
        cambium_genet_winit_host::read_file,
    )?
    .with_frame_limit(Some(1800));
    let lane = Rc::new(RefCell::new(lane));
    let after_lane = lane.clone();
    let mut after_frame = hooks.after_frame;
    hooks.after_frame = Box::new(move |ctx| {
        after_frame(ctx);
        after_lane.borrow_mut().after_frame(ctx);
    });
    let mut close_request = hooks.close_request;
    hooks.close_request = Box::new(move |ctx, request| {
        if !lane.borrow().finished() {
            lane.borrow_mut().request_close();
            CloseDisposition::KeepVisible
        } else {
            close_request(ctx, request)
        }
    });
    Ok((hooks, Some(completion)))
}

#[derive(Default)]
pub(super) struct MainReceipt {
    pub presented: u64,
    pub captures: Vec<(PathBuf, u64, bool)>,
    pub errors: Vec<String>,
    pub workshop: Option<bool>,
    pub last_presented: Option<Instant>,
    deadline_origin: Option<Instant>,
    deadline_suspended: bool,
}

pub(super) struct PendingCapture {
    pub path: PathBuf,
    pub frame: cambium_rootstock::PendingFrame,
    pub started: Instant,
}

impl MainReceipt {
    pub fn presented(&mut self) {
        self.presented += 1;
        let now = Instant::now();
        self.last_presented = Some(now);
        self.deadline_origin = Some(now);
    }
    pub fn presentation_stalled(&mut self, child_active: bool) -> bool {
        self.presentation_stalled_at(Instant::now(), child_active)
    }
    fn presentation_stalled_at(&mut self, now: Instant, child_active: bool) -> bool {
        if child_active {
            self.deadline_suspended = true;
            return false;
        }
        if std::mem::take(&mut self.deadline_suspended) {
            // Returning from a child grants the main window its normal budget.
            // This is a deadline reset, never an invented presentation.
            self.deadline_origin = Some(now);
        }
        let origin = self.deadline_origin.get_or_insert(now);
        now.duration_since(*origin) > Duration::from_secs(10)
    }
    pub fn qualify(&self, outcome: &mut taproot::Outcome) {
        if self.presented == 0 {
            outcome.ok = false;
            outcome
                .log
                .push("FAIL: no successful main presentation".into());
        }
        outcome.log.push(format!(
            "presentation frames={} captures={} blank={}",
            self.presented,
            self.captures.len(),
            self.captures.iter().filter(|capture| capture.2).count()
        ));
        for (path, digest, blank) in &self.captures {
            outcome.log.push(format!(
                "capture {} digest={digest:016x} blank={blank}",
                path.display()
            ));
        }
        for error in &self.errors {
            outcome.ok = false;
            outcome.log.push(format!("FAIL: {error}"));
        }
        if self.workshop == Some(false) {
            outcome.ok = false;
            outcome
                .log
                .push("FAIL: workshop scenario did not complete successfully".into());
        }
        if self.captures.iter().any(|capture| capture.2) {
            outcome.ok = false;
            outcome.log.push("FAIL: blank main capture".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreadable_workshop_script_fails_before_constructing_a_native_window() {
        let missing = std::env::temp_dir().join(format!(
            "turnstone-missing-workshop-{}.scn",
            uuid::Uuid::new_v4()
        ));
        let result = workshop_hooks_config(
            tabard_workshop::native_host::native_hooks(|_| None),
            Some(mesquite::LaneConfig {
                scenario: missing,
                capture_dir: None,
                receipt: None,
            }),
        );
        assert!(matches!(result, Err(error) if error.contains("unreadable")));
    }

    #[test]
    fn child_acceptance_suspends_deadline_and_resume_grants_a_fresh_budget() {
        let start = Instant::now();
        let mut receipt = MainReceipt {
            presented: 12,
            last_presented: Some(start),
            deadline_origin: Some(start),
            ..Default::default()
        };
        assert!(!receipt.presentation_stalled_at(start + Duration::from_secs(20), true));
        assert!(!receipt.presentation_stalled_at(start + Duration::from_secs(400), true));
        assert!(!receipt.presentation_stalled_at(start + Duration::from_secs(500), false));
        assert!(!receipt.presentation_stalled_at(start + Duration::from_secs(509), false));
        assert!(receipt.presentation_stalled_at(start + Duration::from_secs(511), false));
        assert_eq!(receipt.presented, 12);
        assert_eq!(
            receipt.last_presented,
            Some(start),
            "suspension cannot create presentation evidence"
        );
    }

    #[test]
    fn completed_script_cannot_hide_capture_failure_or_absent_presentation() {
        let mut outcome = taproot::Outcome {
            ok: true,
            log: Vec::new(),
        };
        MainReceipt::default().qualify(&mut outcome);
        assert!(!outcome.ok);

        let receipt = MainReceipt {
            presented: 8,
            errors: vec!["map callback disconnected".into()],
            ..Default::default()
        };
        let mut outcome = taproot::Outcome {
            ok: true,
            log: Vec::new(),
        };
        receipt.qualify(&mut outcome);
        assert!(!outcome.ok);
        assert!(
            outcome
                .log
                .iter()
                .any(|line| line.contains("map callback disconnected"))
        );
    }

    #[test]
    fn receipt_preserves_capture_identity_and_failed_workshop_result() {
        let mut receipt = MainReceipt {
            presented: 12,
            captures: vec![(PathBuf::from("authored.png"), 0x1234, false)],
            workshop: Some(true),
            ..Default::default()
        };
        let mut outcome = taproot::Outcome {
            ok: true,
            log: Vec::new(),
        };
        receipt.qualify(&mut outcome);
        assert!(outcome.ok);
        assert!(
            outcome
                .log
                .iter()
                .any(|line| line.contains("authored.png digest=0000000000001234 blank=false"))
        );
        receipt.workshop = Some(false);
        receipt.qualify(&mut outcome);
        assert!(!outcome.ok);
    }

    #[test]
    fn shipped_appearance_fixtures_parse_through_the_existing_driver() {
        for source in [
            include_str!("../../scenarios/tabard_application.scn"),
            include_str!("../../scenarios/tabard_application_reopen.scn"),
            include_str!("../../scenarios/tabard_application_same_id.scn"),
            include_str!("../../scenarios/tabard_application_same_id_reopen.scn"),
            include_str!("../../scenarios/tabard_workshop.scn"),
            include_str!("../../scenarios/tabard_workshop_same_id.scn"),
        ] {
            taproot::Scenario::parse(source).expect("production scenario grammar");
        }
    }
}
