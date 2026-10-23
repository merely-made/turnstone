// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The automation surface: what a scenario drives turnstone through.
//!
//! `Automatable` grants the shared `resolve`, `click`, and `select-text` verbs,
//! so the app implements only what it alone knows: its DOMs (via a visitor,
//! since they sit behind `RefCell`), its retained text targets, its snapshot,
//! its labelled actions, and whether it is still busy. `Driveable` adds the two
//! the generic loop cannot do: a screenshot and turnstone's own verbs.

use super::{NodeSessions, NodeSurfaces};
use winit::event::MouseButton;
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};

use crate::action::Action;
use crate::panes::PaneContent;

use super::Shell;

/// A scenario wait polled once per normal rendered frame. Neither a remote
/// status change nor an exchange file may hold up presentation/event delivery.
pub(super) struct PendingScenarioWait {
    condition: WaitCondition,
    remaining: u32,
}
enum WaitCondition {
    File(std::path::PathBuf),
    Status(String),
}
impl PendingScenarioWait {
    pub(super) fn file(path: std::path::PathBuf, frames: u32) -> Self {
        Self {
            condition: WaitCondition::File(path),
            remaining: frames.max(1),
        }
    }
    pub(super) fn status(substring: String, frames: u32) -> Self {
        Self {
            condition: WaitCondition::Status(substring),
            remaining: frames.max(1),
        }
    }
    pub(super) fn poll(&mut self, status: &[String]) -> Result<bool, String> {
        let ready = match &self.condition {
            WaitCondition::File(path) => std::fs::metadata(path).is_ok_and(|meta| meta.len() > 0),
            WaitCondition::Status(substring) => status.iter().any(|line| line.contains(substring)),
        };
        if ready {
            return Ok(true);
        }
        if self.remaining == 0 {
            return Err(match &self.condition {
                WaitCondition::File(path) => {
                    format!("wait-file '{}': still missing or empty", path.display())
                },
                WaitCondition::Status(substring) => {
                    format!("wait-status '{substring}': place status is {status:?}")
                },
            });
        }
        self.remaining -= 1;
        Ok(false)
    }
}
#[cfg(test)]
mod pending_wait_tests {
    use super::PendingScenarioWait;
    #[test]
    fn pending_file_wait_observes_empty_then_populated_file_on_later_frames() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ready");
        let mut wait = PendingScenarioWait::file(path.clone(), 3);
        assert!(!wait.poll(&[]).unwrap());
        std::fs::write(&path, b"").unwrap();
        assert!(!wait.poll(&[]).unwrap());
        std::fs::write(&path, b"ready").unwrap();
        assert!(wait.poll(&[]).unwrap());
    }
    #[test]
    fn pending_file_wait_refuses_an_exhausted_budget_but_accepts_a_ready_last_frame() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ready");
        let mut wait = PendingScenarioWait::file(path.clone(), 1);
        assert!(!wait.poll(&[]).unwrap());
        assert!(wait.poll(&[]).is_err());
        std::fs::write(&path, b"ready").unwrap();
        assert!(wait.poll(&[]).unwrap());
    }
    #[test]
    fn status_wait_returns_between_frames_and_names_the_last_status_on_failure() {
        let mut wait = PendingScenarioWait::status("3 messages".into(), 1);
        assert!(!wait.poll(&["2 messages".into()]).unwrap());
        let error = wait.poll(&["still 2 messages".into()]).unwrap_err();
        assert!(error.contains("3 messages") && error.contains("still 2 messages"));
        assert!(
            wait.poll(&["Retained history: 1 shared nodes, 3 messages".into()])
                .unwrap()
        );
    }
}

fn parse_public_fixture_a11y_receipt(value: Option<&str>) -> Result<bool, String> {
    match value {
        None | Some("0") => Ok(false),
        Some("1") => Ok(true),
        Some(_) => Err("TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT must be 0 or 1".into()),
    }
}

fn public_fixture_a11y_receipt() -> Result<bool, String> {
    match std::env::var("TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT") {
        Ok(value) => parse_public_fixture_a11y_receipt(Some(&value)),
        Err(std::env::VarError::NotPresent) => Ok(false),
        Err(_) => Err("TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT is not valid Unicode".into()),
    }
}

#[cfg(test)]
mod foreign_a11y_receipt_tests {
    use super::parse_public_fixture_a11y_receipt;

    #[test]
    fn page_text_requires_exact_public_fixture_optin() {
        assert!(!parse_public_fixture_a11y_receipt(None).unwrap());
        assert!(!parse_public_fixture_a11y_receipt(Some("0")).unwrap());
        assert!(parse_public_fixture_a11y_receipt(Some("1")).unwrap());
        for invalid in ["", "true", "yes", "2", " 1"] {
            assert!(parse_public_fixture_a11y_receipt(Some(invalid)).is_err());
        }
    }
}

struct IdleDiagnosis {
    pending_fetches: bool,
    requested_content: bool,
    graph_settling: bool,
    unsettled_sessions: Vec<uuid::Uuid>,
}

impl IdleDiagnosis {
    fn is_busy(&self) -> bool {
        self.pending_fetches
            || self.requested_content
            || self.graph_settling
            || !self.unsettled_sessions.is_empty()
    }

    fn content_ready(&self) -> bool {
        !self.pending_fetches && !self.requested_content && self.unsettled_sessions.is_empty()
    }

    fn describe(&self) -> String {
        let sessions = self
            .unsettled_sessions
            .iter()
            .map(uuid::Uuid::to_string)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "busy={} pending_fetches={} requested_content={} graph_settling={} unsettled_sessions=[{}]",
            self.is_busy(),
            self.pending_fetches,
            self.requested_content,
            self.graph_settling,
            sessions
        )
    }
}

/// Re-check `condition` once per (simulated) frame for up to `frames`
/// attempts, sleeping briefly between attempts. Legacy row/document waits
/// are "sticky" over the SHARED taproot scenario loop's own
/// frame pump (`Scenario::tick` advances its step index unconditionally once
/// per call — see `taproot::scenario`), so there is no hook to hold a
/// single step across several of ITS frames. This polls within the one call
/// turnstone's `app_step` gets instead: background work (file writes, network
/// I/O) runs on other threads/tasks, so a short sleep between checks still
/// lets it progress. Returns true the moment `condition` holds.
impl Shell {
    /// A wait that holds the frame loop still needs the place worker's answers,
    /// which the loop would otherwise drain; drain them here between checks.
    fn wait_frames(&mut self, frames: u32, mut condition: impl FnMut(&Shell) -> bool) -> bool {
        let attempts = frames.max(1);
        for attempt in 0..attempts {
            // A resolved citation is an App intent, independent of a surface's
            // next viewport sync. Poll the same native drain during waits.
            self.pump_research();
            while let Ok(update) = self.trail_rx.try_recv() {
                let effects = self.app.apply_update(update);
                self.run_effects(effects);
            }
            while let Ok(update) = self.place_rx.try_recv() {
                let effects = self.app.apply_update(update);
                self.run_effects(effects);
            }
            // Retained work must advance even when no window is rendering.
            // Pump folds completion events as well as document clocks; hidden
            // work can settle without requesting a paint.
            let _ = self.pump_visible_documents();
            self.refresh_knot_documents();
            if condition(self) {
                return true;
            }
            if attempt + 1 < attempts {
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        }
        false
    }
}

impl Shell {
    fn idle_diagnosis(&mut self) -> IdleDiagnosis {
        let _ = self.pump_visible_documents();
        let mut unsettled_sessions = self
            .content_sessions
            .pool
            .pending_contents()
            .map(|key: &super::content_pool::ContentKey| match *key {
                super::content_pool::ContentKey::Node(node)
                | super::content_pool::ContentKey::Reader { node, .. } => node,
            })
            .collect::<Vec<_>>();
        unsettled_sessions.sort_unstable();
        unsettled_sessions.dedup();
        IdleDiagnosis {
            pending_fetches: self.pending_fetches.any_in_flight(),
            requested_content: self.app.content.any_requested(),
            graph_settling: self.app.graph_runtimes.is_settling(),
            unsettled_sessions,
        }
    }
}

/// turnstone drives through the shared taproot harness: implementing this
/// small surface grants the `resolve` / `click` verbs (used by the collapsed
/// `click_pane_*` above) for free. `with_surfaces` hands the retained pane DOMs
/// to a visitor — the borrow guards live only for the callback, which is why the
/// trait takes a visitor rather than returning a `Vec` (turnstone's DOMs are behind
/// `RefCell`). Inspector/Workbench panes join by adding their `dom_ref` here when
/// they grow click verbs.
impl Shell {
    fn focused_saved_reader_node(&self) -> Option<uuid::Uuid> {
        let focused = match self.app.focus {
            crate::surface::FocusTarget::Content { node, .. } => Some(node),
            crate::surface::FocusTarget::Graph(pane) => {
                self.app.graph_pane_focused_member(pane)
            }
            _ => None,
        }?;
        self.app
            .saved_readers
            .contains_key(&focused)
            .then_some(focused)
    }

    fn saved_reader_engine_live(&self, node: uuid::Uuid) -> bool {
        matches!(self.app.content.get(node), Some(crate::content::NodeContent::Live))
            && self
                .app
                .content
                .facts(node)
                .is_some_and(|facts| facts.engine == crate::saved_sources::ENGINE_ID)
            && self
                .content_sessions
                .session(&node)
                .is_some_and(|session| {
                    session
                        .as_any_ref()
                        .downcast_ref::<mere_document_lanes::SmolwebDocumentSession>()
                        .is_some()
                })
    }

    fn focused_saved_reader_ready(&self) -> bool {
        let Some(node) = self.focused_saved_reader_node() else {
            return false;
        };
        self.app.saved_readers.get(&node).is_some_and(|reader| {
            reader.request.is_none()
                && reader.error.is_none()
                && reader.content.is_some()
                && self.saved_reader_engine_live(node)
        })
    }

    fn scenario_key_in_window(
        &mut self,
        window: usize,
        key: &crate::scenario::EditKey,
    ) -> Result<(), String> {
        if window != 0
            && !self
                .lens_windows
                .values()
                .any(|lens| lens.ordinal + 1 == window)
        {
            return Err("key-window target is absent".into());
        }
        if window != 0
            && (self.app.omnibar.open
                || self.app.document_find.open
                || self.app.user_agent_decision.is_open())
        {
            return Err("a primary host editor owns keyboard input".into());
        }
        if !super::frame_keys::header_focus_in_window(
            self.outer_focus.as_ref(),
            window as u64,
            self.app.session_id.as_uuid(),
        ) && !self.focus_in_window(window as u64, self.app.focus)
        {
            let focus = self
                .window_focus
                .get(&(window as u64))
                .copied()
                .filter(|focus| self.focus_in_window(window as u64, *focus))
                .or_else(|| self.fallback_window_focus(window as u64))
                .ok_or("window has no current focus target")?;
            self.app.focus = focus;
        }
        use crate::scenario::EditKey;
        use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};

        // Route through the SAME key seam winit uses, so `key` drives
        // whatever holds focus (the omnibar, a focused page, the
        // canvas) exactly as a real press would — one description, two
        // runners, for keys as well as pointers. The former direct
        // EditKey->omnibar-Action map only ever reached the omnibar.
        let (winit_key, ctrl) = match key {
            EditKey::Enter | EditKey::ShiftEnter => (WinitKey::Named(WinitNamedKey::Enter), false),
            EditKey::Escape => (WinitKey::Named(WinitNamedKey::Escape), false),
            EditKey::Tab | EditKey::BackTab => (WinitKey::Named(WinitNamedKey::Tab), false),
            EditKey::FrameFocus => (WinitKey::Named(WinitNamedKey::F6), true),
            EditKey::FrameClose => (WinitKey::Character("w".into()), true),
            EditKey::Backspace => (WinitKey::Named(WinitNamedKey::Backspace), false),
            EditKey::Delete => (WinitKey::Named(WinitNamedKey::Delete), false),
            EditKey::Up => (WinitKey::Named(WinitNamedKey::ArrowUp), false),
            EditKey::Down => (WinitKey::Named(WinitNamedKey::ArrowDown), false),
            EditKey::Left | EditKey::ShiftLeft => (WinitKey::Named(WinitNamedKey::ArrowLeft), false),
            EditKey::Right | EditKey::ShiftRight => {
                (WinitKey::Named(WinitNamedKey::ArrowRight), false)
            },
            EditKey::SelectAll => (WinitKey::Character("a".into()), true),
            EditKey::Home => (WinitKey::Named(WinitNamedKey::Home), false),
            EditKey::End => (WinitKey::Named(WinitNamedKey::End), false),
            EditKey::PageDown => (WinitKey::Named(WinitNamedKey::PageDown), false),
            EditKey::PageUp => (WinitKey::Named(WinitNamedKey::PageUp), false),
            EditKey::Space => (WinitKey::Named(WinitNamedKey::Space), false),
            EditKey::Save => (WinitKey::Character("s".into()), true),
            EditKey::Find => (WinitKey::Character("f".into()), true),
            EditKey::KeepCommand => (WinitKey::Character("d".into()), true),
        };
        let previous_ctrl = self.ctrl;
        let previous_shift = self.shift;
        self.shift |= matches!(
            key,
            EditKey::BackTab | EditKey::ShiftRight | EditKey::ShiftLeft | EditKey::ShiftEnter
        );
        self.ctrl |= ctrl;
        // Match the text accompanying a native winit key press. In
        // Chromium, Enter's carriage-return char event activates the
        // focused button after rawKeyDown.
        let text = match winit_key {
            WinitKey::Named(WinitNamedKey::Enter) => Some("\r"),
            WinitKey::Named(WinitNamedKey::Space) => Some(" "),
            _ => None,
        };
        // Purpose-built self-drive enters after OS admission. Native
        // input keeps its physical activation guard; this fixture
        // validates current window custody without stealing OS focus.
        self.input_window = window as u64;
        if self.deliver_surface_key(&winit_key, true, text) {
            self.deliver_surface_key(&winit_key, false, None);
        } else {
            let size = if window == 0 {
                (self.width, self.height)
            } else {
                self.lens_windows
                    .values()
                    .find(|lens| lens.ordinal + 1 == window)
                    .map(|lens| (lens.width, lens.height))
                    .unwrap()
            };
            self.on_key_in_window(window, size, &winit_key);
        }
        self.ctrl = previous_ctrl;
        self.shift = previous_shift;
        Ok(())
    }
}

impl taproot::Automatable for Shell {
    fn with_surfaces<R>(&self, f: impl FnOnce(&[taproot::ProbeSurface<'_>]) -> R) -> R {
        let plan = self.surface_plan();
        let mut guards: Vec<(
            &'static str,
            [f32; 4],
            std::cell::Ref<'_, genet_scripted_dom::ScriptedDom>,
        )> = Vec::new();
        let mut contributed_guards: Vec<(
            &'static str,
            [f32; 4],
            std::cell::Ref<'_, genet_scripted_dom::ScriptedDom>,
            &str,
        )> = Vec::new();
        let knot_sheet = format!(
            "{} {}",
            crate::ui::CAMBIUM_SHEET,
            crate::knot_authoring::KNOT_SHEET
        );
        let mut chrome = crate::shell_services::ShellChromeConfig::default();
        self.live_settings.snapshot().apply_to(&mut chrome);
        let settings_sheet = crate::ui::cambium_sheet(&chrome.appearance);
        for surface in &plan {
            let rect = [
                surface.rect.x,
                surface.rect.y,
                surface.rect.w,
                surface.rect.h,
            ];
            match surface.kind {
                crate::surface::SurfaceKind::Content(node) => {
                    if let Some(session) = self.content_sessions.session(&node)
                        && let Some(knot) = session
                            .as_any_ref()
                            .downcast_ref::<crate::knot_authoring::KnotDocumentSession>(
                        )
                    {
                        guards.push(("knot", rect, knot.dom_ref()));
                    }
                },
                crate::surface::SurfaceKind::Pane(id)
                    if self.renderers.inline_mount(id).is_some() => {},
                crate::surface::SurfaceKind::Pane(id) => match self.pane_content(id) {
                    Some(PaneContent::Roster) => {
                        if let Some(g) = self.renderers.roster.get(&id) {
                            guards.push(("roster", rect, g.dom_ref()));
                        }
                    },
                    Some(PaneContent::Trail) => {
                        if let Some(pane) = self.renderers.trail.get(&id) {
                            guards.push(("trail", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Inspector) => {
                        if let Some(pane) = self.renderers.inspector.get(&id) {
                            guards.push(("inspector", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Workbench) => {
                        if let Some(pane) = self.renderers.workbench.get(&id) {
                            guards.push(("workbench", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Gloss(_)) => {
                        if let Some(pane) = self.renderers.gloss.get(&id) {
                            guards.push(("gloss", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::TRANSCRIPT =>
                    {
                        if let Some(pane) = self.renderers.transcript.get(&id) {
                            guards.push(("transcript", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::SETTINGS =>
                    {
                        if let Some(pane) = self.renderers.settings.get(&id) {
                            guards.push(("settings", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::ARRANGE =>
                    {
                        if let Some(pane) = self.renderers.arrange.get(&id) {
                            guards.push(("arrange", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::PUBLISHING =>
                    {
                        if let Some(pane) = self.renderers.publish.get(&id) {
                            guards.push(("publishing", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::SHARED_KNOT =>
                    {
                        if let Some(pane) = self.renderers.shared_knot.get(&id) {
                            guards.push(("shared-knot", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::DEVICE_RECEIPTS =>
                    {
                        if let Some(pane) = self.renderers.device_receipts.get(&id) {
                            guards.push(("device-receipts", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::FROZEN_PROJECTION =>
                    {
                        if let Some(pane) = self.renderers.frozen_projection.get(&id) {
                            guards.push(("frozen-projection", rect, pane.dom_ref()));
                        }
                    },
                    Some(PaneContent::Registered(_)) => {
                        if let Some(pane) = self.renderers.contributed.get(id) {
                            contributed_guards.push((
                                "contributed",
                                rect,
                                pane.dom_ref(),
                                pane.stylesheet(),
                            ));
                        }
                    },
                    Some(PaneContent::Overmap(_)) => {
                        if let Some(pane) = self.renderers.overmap.get(&id) {
                            guards.push(("overmap", rect, pane.dom_ref()));
                        }
                    },
                    _ => {},
                },
                _ => {},
            }
        }
        let mut surfaces: Vec<taproot::ProbeSurface> = guards
            .iter()
            .map(|(name, rect, r)| taproot::ProbeSurface {
                name,
                dom: r,
                rect: *rect,
                sheet: if *name == "knot" {
                    &knot_sheet
                } else if *name == "settings" {
                    &settings_sheet
                } else {
                    crate::ui::CAMBIUM_SHEET
                },
            })
            .collect();
        surfaces.extend(contributed_guards.iter().map(|(name, rect, dom, sheet)| {
            taproot::ProbeSurface {
                name,
                dom,
                rect: *rect,
                sheet,
            }
        }));
        // Shared forest targets use selector_target's actual scoped fragments.
        f(&surfaces)
    }

    fn selector_target(&self, selector: &taproot::Selector) -> taproot::SelectorTarget {
        match selector.surface.as_deref() {
            Some("chrome") => self.chrome.selector_point(selector).map_or(taproot::SelectorTarget::Miss,|point|taproot::SelectorTarget::Hit(taproot::Hit{surface:"chrome",point})),
            None => self.retained_pane_selector_target(selector),
            Some("frame") => {
                let mut frames = self.outer_frames.borrow_mut();
                let Some(frame) = frames.get_mut(&0) else {
                    return taproot::SelectorTarget::Miss;
                };
                match frame.selector_point(selector) {
                    Ok(Some(point)) => taproot::SelectorTarget::Hit(taproot::Hit {
                        surface: "frame",
                        point,
                    }),
                    _ => taproot::SelectorTarget::Miss,
                }
            },
            Some("workbench") => self.retained_pane_selector_target(selector),
            Some("settings") => {
                let plan = self.surface_plan();
                let mut selected = None;
                for surface in &plan {
                    let crate::surface::SurfaceKind::Pane(id) = surface.kind else {
                        continue;
                    };
                    if !matches!(self.pane_content(id), Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::SETTINGS)
                    {
                        continue;
                    }
                    let Some(pane) = self.renderers.settings.get(&id) else {
                        continue;
                    };
                    let point = match pane.selector_point(selector) {
                        Ok(Some((x, y))) => (surface.rect.x + x, surface.rect.y + y),
                        Ok(None) => continue,
                        Err(_) => return taproot::SelectorTarget::Miss,
                    };
                    // Preserve ordinary physical routing and attribution:
                    // covered targets and duplicate panes cannot claim a hit.
                    if crate::surface::hit_test(&plan, self.app.focus, point.0, point.1)
                        .is_none_or(|hit| hit.id != surface.id)
                        || selected.is_some()
                    {
                        return taproot::SelectorTarget::Miss;
                    }
                    selected = Some(taproot::Hit {
                        surface: "settings",
                        point,
                    });
                }
                selected.map_or(taproot::SelectorTarget::Miss, taproot::SelectorTarget::Hit)
            },
            Some("inspector") => {
                let plan = self.surface_plan();
                let mut selected = None;
                for surface in &plan {
                    let crate::surface::SurfaceKind::Pane(id) = surface.kind else {
                        continue;
                    };
                    if self.pane_content(id) != Some(PaneContent::Inspector) {
                        continue;
                    }
                    let Some(pane) = self.renderers.inspector.get(&id) else {
                        continue;
                    };
                    let point = match pane.selector_point(selector) {
                        Ok(Some((x, y))) => (surface.rect.x + x, surface.rect.y + y),
                        Ok(None) => continue,
                        Err(_) => return taproot::SelectorTarget::Miss,
                    };
                    // Retained pane geometry still goes through ordinary
                    // physical routing. An overlapping toolbar or another
                    // surface must not receive an attributed Inspector click.
                    if crate::surface::hit_test(&plan, self.app.focus, point.0, point.1)
                        .is_none_or(|hit| hit.id != surface.id)
                    {
                        return taproot::SelectorTarget::Miss;
                    }
                    if selected.is_some() {
                        return taproot::SelectorTarget::Miss;
                    }
                    selected = Some(taproot::Hit {
                        surface: "inspector",
                        point,
                    });
                }
                selected.map_or(taproot::SelectorTarget::Miss, taproot::SelectorTarget::Hit)
            },
            Some("contributed") => match self.contributed_selector(selector) {
                Ok(target) => target
                    .point()
                    .map_or(taproot::SelectorTarget::Miss, |point| {
                        taproot::SelectorTarget::Hit(taproot::Hit {
                            surface: "contributed",
                            point,
                        })
                    }),
                Err(_) => taproot::SelectorTarget::Miss,
            },
            // Other ordinary pane scopes use their retained pane owner.
            Some(_) => self.retained_pane_selector_target(selector),
        }
    }

    fn click_target(&mut self, selector: &taproot::Selector) -> Option<bool> {
        match selector.surface.as_deref() {
            None => None,
            Some("contributed") => Some(self.click_contributed(selector)),
            Some(_) => None,
        }
    }

    fn text_target(&self, text: &str) -> Result<Option<taproot::TextTarget>, String> {
        let plan = self.surface_plan();
        let mut matches = plan.iter().filter_map(|surface| {
            let crate::surface::SurfaceKind::Content(node) = surface.kind else {
                return None;
            };
            self.content_sessions
                .session(&node)
                .and_then(|session| session.text_target(text))
                .map(|target| taproot::TextTarget {
                    anchor: (
                        surface.rect.x + target.anchor[0],
                        surface.rect.y + target.anchor[1],
                    ),
                    focus: (
                        surface.rect.x + target.focus[0],
                        surface.rect.y + target.focus[1],
                    ),
                })
        });
        let first = matches.next();
        if matches.next().is_some() {
            return Err("more than one live content surface matched".into());
        }
        Ok(first)
    }

    fn snapshot(&self) -> taproot::ProbeSnapshot {
        let snap = crate::observe::snapshot(&self.app);
        let appearance = self.live_settings.snapshot();
        let kept = snap.focused.as_ref().is_some_and(|node| node.kept);
        let saved_reader_node = self.focused_saved_reader_node();
        let saved_reader = saved_reader_node.and_then(|node| self.app.saved_readers.get(&node));
        let saved_reader_state = match saved_reader {
            None => "None",
            Some(reader) if reader.error.is_some() => "Error",
            Some(reader)
                if reader.request.is_none()
                    && reader.content.is_some()
                    && saved_reader_node.is_some_and(|node| self.saved_reader_engine_live(node)) =>
            {
                "Ready"
            }
            Some(_) => "Pending",
        };
        let saved_reader_text = saved_reader
            .and_then(|reader| reader.content.as_ref())
            .map(|content| content.text.as_str())
            .unwrap_or_default();
        let saved_reader_quote = saved_reader
            .and_then(|reader| {
                reader.content.as_ref().and_then(|content| {
                    crate::saved_sources::validate_selector(content, &reader.target)
                        .ok()
                        .flatten()
                })
            })
            .unwrap_or_default();
        let saved_reader_manifest = saved_reader
            .map(|reader| reader.target.key.annotation_manifest.as_str())
            .unwrap_or_default();
        let saved_reader_origin = saved_reader
            .and_then(|reader| reader.content.as_ref())
            .map(|content| content.origin.as_str())
            .unwrap_or_default();
        let saved_reader_selector = saved_reader
            .and_then(|reader| reader.target.selector.as_ref())
            .map(|selector| format!("{}..{}", selector.start, selector.end))
            .unwrap_or_default();
        let reading_target=self.app.focused_read_target();
        let mut out = taproot::ProbeSnapshot::default()
            .with_field("reading-text-size",reading_target.map(|target|self.app.reading.preferences(target).text_size.to_string()).unwrap_or_default())
            .with_field("reading-column-em",reading_target.map(|target|self.app.reading.preferences(target).column_em.to_string()).unwrap_or_default())
            .with_field("scope-level",self.app.scope_place().level.label())
            .with_field("scope-focus",self.app.scope_place().focus.map(|m|m.to_string()).unwrap_or_default())
            .with_field("scope-back",self.app.reading.scope.navigation.destination(false).is_some().to_string())
            .with_field("scope-forward",self.app.reading.scope.navigation.destination(true).is_some().to_string())
            .with_field("reading-history-back",reading_target.is_some_and(|target|self.app.reading.trails.get(&target.pane).is_some_and(|trail|trail.can_back())).to_string())
            .with_field("reading-history-forward",reading_target.is_some_and(|target|self.app.reading.trails.get(&target.pane).is_some_and(|trail|trail.can_forward())).to_string())
            .with_field("reading-anchor",reading_target.and_then(|target|self.app.reading.anchors.get(&target)).map(|anchor|format!("{}:{}:{}",anchor.text,anchor.offset,anchor.exact)).unwrap_or_default())
            .with_field("reading-find",reading_target.and_then(|target|self.app.reading.find.get(&target)).filter(|ui|ui.open).map_or("closed",|ui|ui.status.as_str()))
            .with_field("scene-arrangement", snap.graph_scene.arrangement)
            .with_field("scene-saved-seed", snap.graph_scene.saved_seed.to_string())
            .with_field("scene-regions", snap.graph_scene.regions.to_string())
            .with_field("scene-foreground-pins", snap.graph_scene.foreground_pins.to_string())
            .with_field("scene-retained", snap.graph_scene.retained.to_string())
            .with_field("scene-grammar", snap.graph_scene.grammar.to_string())
            .with_field("scene-territory", snap.graph_scene.territory.to_string())
            .with_field("focus", snap.focus)
            .with_field("saved-sources-status", self.app.saved_sources.status.clone())
            .with_field("saved-reader-state", saved_reader_state)
            .with_field("saved-reader-text", saved_reader_text)
            .with_field("saved-reader-quote", saved_reader_quote)
            .with_field("saved-reader-manifest", saved_reader_manifest)
            .with_field("saved-reader-origin", saved_reader_origin)
            .with_field("saved-reader-selector", saved_reader_selector)
            .with_field(
                "host-surface-producers",
                self.content_sessions.surface_count().to_string(),
            )
            .with_field("node-count", snap.node_count.to_string())
            .with_field("network-offline", self.network_offline.to_string())
            .with_field("peer-services", (usize::from(self.publish_service.is_some()) + usize::from(self.shared_knot_service.is_some())).to_string())
            .with_field("offline-acquisition-attempts", self.offline_attempts.load(std::sync::atomic::Ordering::Relaxed).to_string())
            .with_field("producer-retention", self.app.producer_retention_action().map(|(label, _)| label).unwrap_or_else(|| "Unavailable".into()))
            .with_field("retained-voice", self.app.redshank.retained_voice_preview()
                .map(|p| format!("{} {:?} {} ms", p.id, p.state, p.position_ms)).unwrap_or_else(|| "Ready".into()))
            .with_field("inline-roster-tabs", {
                let mut tabs: Vec<_> = self.renderers.roster.iter().map(|(pane, view)|
                    (pane.0, view.selected_tab().1)).collect();
                tabs.sort_by_key(|(pane, _)| *pane);
                tabs.into_iter().map(|(pane, label)| format!("{pane}:{label}"))
                    .collect::<Vec<_>>().join(",")
            })
            .with_field("roster-tab", snap.roster_tab)
            .with_field("native-texture-count", self.renderers.native_images.values().map(|images| images.len()).sum::<usize>().to_string())
            .with_field("inline-component-count", self.surface_plan().iter().filter(|surface|
                self.app.blueprint_space(crate::action::SpaceRef::Primary).is_none() &&
                matches!(surface.kind, crate::surface::SurfaceKind::Pane(pane) if self.renderers.inline_mount(pane).is_some())).count().to_string())
            // The panes and surfaces as joined tags, so a generic scenario can
            // `assert snap panes ~ roster` without an app-specific verb. This is
            // minimal-shared-and-grow: the app adds the fields its scenarios name.
            .with_field("panes", snap.panes.join(","))
            .with_field("surfaces", snap.surfaces.join(","))
            .with_field("floats", snap.floating_panes.join(","))
            .with_field("float-count",snap.floating_panes.len().to_string())
            .with_field("lens-float-count",snap.lens_floating_panes.len().to_string())
            .with_field("lens-floats", snap.lens_floating_panes.join(","))
            // What the app will DO right now, by label — the automation half of
            // a coherent snapshot. `assert snap actions ~ Fit to view` asks whether
            // a verb is on offer before spending a step on it.
            .with_field("actions", snap.available_actions.join(","))
            // The omnibar's offered rows as their display strings, so a
            // recall receipt can name the row it expects ("recall <url>")
            // without a verb of its own — the same minimal-shared-and-grow
            // rule the panes and actions fields follow.
            .with_field("suggestions", snap.omnibar.suggestions.join(","))
            .with_field("kept", kept.to_string())
            // The person's command-menu choices by id, as the view sidecar
            // stores them: the bare `>` lane is bounded by the row limit, so a
            // kept command can be stored and still sit past the visible rows.
            .with_field("commands-added", self.app.command_choices.added.join(","))
            .with_field(
                "commands-removed",
                self.app.command_choices.removed.join(","),
            );
        out = out
            .with_field(
                "appearance-theme",
                appearance.theme_id().unwrap_or_default(),
            )
            .with_field(
                "appearance-mode",
                appearance.theme_mode().unwrap_or_default(),
            )
            .with_field(
                "appearance-workshop-open",
                self.theme_editor.is_some().to_string(),
            )
            .with_field(
                "appearance-workshop-ok",
                self.main_receipt
                    .workshop
                    .map(|ok| ok.to_string())
                    .unwrap_or_default(),
            )
            .with_field("presented-frames", self.main_receipt.presented.to_string())
            .with_field(
                "capture-count",
                self.main_receipt.captures.len().to_string(),
            )
            .with_field("capture-errors", self.main_receipt.errors.len().to_string());
        let appearance_sheet = match self
            .app
            .shell_chrome_config()
            .appearance
            .theme_presentation
            .as_ref()
        {
            Some(tabard::ThemePresentation::AuthoredStylesheet(sheets)) => sheets.join("\n"),
            _ => String::new(),
        };
        out = out
            .with_field("appearance-css", appearance_sheet)
            .with_field(
                "frame-focus-slot",
                self.outer_focus
                    .as_ref()
                    .map(|f| f.slot.to_string())
                    .unwrap_or_else(|| "none".into()),
            )
            .with_field("a11y-lens-windows", self.lens_windows.len().to_string())
            .with_field(
                "frame-control",
                self.outer_focus
                    .as_ref()
                    .and_then(|focus| {
                        self.outer_frames
                            .borrow()
                            .get(&focus.slot)
                            .and_then(|frame| {
                                frame.controls.focus().map(|control| format!("{control:?}"))
                            })
                    })
                    .unwrap_or_else(|| "none".into()),
            );
        if self
            .app
            .blueprint_space(crate::action::SpaceRef::Primary)
            .is_none()
        {
            if let Some(frame) = self.outer_frames.borrow().get(&0) {
                out = out
                    .with_field("frame-css", frame.stylesheet())
                    .with_field("frame-header-height", frame.header_height().to_string());
            }
        }
        #[cfg(all(feature = "scry", windows))]
        {
            let ready = self
                .scry_frame_importers
                .values()
                .filter(|importer| importer.stats().frames > 0)
                .count();
            out = out
                .with_field(
                    "host-scry-producers",
                    self.scry_frame_importers.len().to_string(),
                )
                .with_field("host-scry-imported-pages", ready.to_string());
        }
        #[cfg(all(feature = "servo", windows))]
        {
            out = out
                .with_field("host-servo-views", super::servo::active_views().to_string())
                .with_field(
                    "host-servo-profile",
                    self.servo_factory
                        .as_ref()
                        .map_or("", |factory| factory.profile_name()),
                )
                .with_field(
                    "host-servo-a11y-focus",
                    self.servo_factory
                        .as_ref()
                        .map_or("", |factory| factory.a11y_focus_policy()),
                );
        }
        let (foreign_surfaces, foreign_trees, foreign_nodes) = self.surface_a11y.published_counts();
        out = out
            .with_field("host-foreign-a11y-surfaces", foreign_surfaces.to_string())
            .with_field("host-foreign-a11y-trees", foreign_trees.to_string())
            .with_field("host-foreign-a11y-nodes", foreign_nodes.to_string())
            // These observe successful in-process composition. The OS adapter
            // may be inactive; neither the counts nor text qualify native AT.
            .with_field(
                "host-foreign-a11y-text",
                if public_fixture_a11y_receipt().unwrap_or(false) {
                    self.surface_a11y.published_text()
                } else {
                    String::new()
                },
            );
        if let Some(find) = snap.document_find {
            out = out
                .with_field("document-find-query", find.query)
                .with_field("document-find-caret", find.caret.to_string())
                .with_field("document-find-selection", find.selection)
                .with_field("document-find-count", find.count.to_string())
                .with_field(
                    "document-find-current",
                    find.current
                        .map_or_else(String::new, |index| (index + 1).to_string()),
                )
                .with_field("document-find-status", find.status);
        }
        if let Some(capabilities) = snap.document_capabilities {
            out = out
                .with_field("document-find-capability", capabilities.find_in_page)
                .with_field("page-zoom-capability", capabilities.page_zoom)
                .with_field("page-capture-capability", capabilities.page_capture)
                .with_field("navigation-capability", capabilities.navigation);
        }
        // The requested scale is always known for a focused node; the applied
        // half appears only for engines that read their effective value back,
        // so a scenario asserting `page-zoom-applied` is also asserting the
        // read-back seam exists.
        if let Some(node) = snap.focused.as_ref() {
            out = out.with_field("page-zoom-requested", node.page_zoom_percent.to_string());
            out = out.with_field(
                "host-focused-engine",
                self.app
                    .content
                    .facts(node.member)
                    .map(|facts| facts.engine.clone())
                    .unwrap_or_default(),
            );
            if let Some(applied) = node.applied_page_zoom_percent {
                out = out.with_field("page-zoom-applied", applied.to_string());
            }
        }
        // Page zoom is a DOCUMENT scale. The other two scales a receipt could
        // confuse it with are the chrome's UI zoom and the graph camera's
        // zoom, so both are projected here: an E0.2 receipt asserts they held
        // across a page-zoom step rather than trusting that they did. Chrome
        // zoom is a percentage like the page-zoom pair; the camera keeps three
        // decimals, because it is a continuous gesture value and not a ladder.
        out = out.with_field(
            "ui-zoom",
            ((self.app.shell_chrome_config().appearance.ui_zoom.max(0.0) * 100.0).round() as i64)
                .to_string(),
        );
        if let Some(zoom) = self
            .app
            .graph_for_pane(self.app.default_graph_pane())
            .and_then(|graph| self.app.graph_runtimes.canvas(graph))
            .map(|canvas| canvas.viewport().zoom)
        {
            out = out.with_field("canvas-zoom", format!("{zoom:.3}"));
        }
        if let Some(decision) = snap.user_agent_decision {
            out = out
                .with_field("decision-kind", decision.kind)
                .with_field("decision-node", decision.node.to_string())
                .with_field("decision-request", decision.request.to_string())
                .with_field("decision-prompt", decision.prompt)
                .with_field("decision-queued", decision.queued.to_string())
                .with_field("decision-submitting", decision.submitting.to_string())
                .with_field(
                    "decision-auth-field",
                    decision.authentication_field.unwrap_or_default(),
                )
                .with_field(
                    "decision-process-memory",
                    decision.remember_for_process.to_string(),
                );
        }
        // Fold the url in with the caption, so `assert snap focused ~ example.com`
        // can name the navigated address, not only the display caption.
        out.focused = snap.focused.map(|n| format!("{}  {}", n.caption, n.url));
        // Browser commands and page input have a pane/member owner independent
        // of the graph cursor. Keep the graph snapshot's meaning intact.
        if let Some((member, url)) = self.app.browser_command_target() {
            let title = self
                .app
                .graph_runtimes
                .graph_containing_member(member)
                .and_then(|graph| self.app.graph_runtimes.canvas(graph))
                .and_then(|canvas| canvas.graph().get_node_by_id(member))
                .map(|(_, node)| node.title.clone())
                .unwrap_or_default();
            let state = self
                .app
                .content
                .get(member)
                .map_or("closed", |state| match state {
                    crate::content::NodeContent::Live => "live",
                    crate::content::NodeContent::Requested => "requested",
                    crate::content::NodeContent::Failed(_) => "failed",
                    _ => "awaiting-input",
                });
            out = out
                .with_field("browser-current", format!("{title}  {url}"))
                .with_field("browser-current-content", state)
                .with_field(
                    "browser-page-zoom-requested",
                    crate::app::page_zoom_percent(
                        self.app
                            .browser
                            .get(member)
                            .and_then(|state| state.page_scale),
                    )
                    .to_string(),
                );
            if let Some(facts) = self.app.content.facts(member) {
                out = out
                    .with_field("browser-current-engine", facts.engine.clone())
                    .with_field(
                        "browser-find-capability",
                        facts.capabilities.find_in_page.describe(),
                    )
                    .with_field(
                        "browser-zoom-capability",
                        facts.capabilities.page_zoom.describe(),
                    );
            }
        }
        out
    }

    fn drain_events(&mut self) -> Vec<String> {
        self.observed_events.drain(..).collect()
    }

    fn act(&mut self, label: &str) -> bool {
        // Resolve against THE catalog the palette offers, in its order, so a
        // scenario acts on exactly what a person would see and pick. An exact
        // label wins anywhere before any prefix is considered; ties inside each
        // pass go to the earlier row, which is the contextual one. (This used to
        // resolve static-first while the palette showed dynamic-first — the two
        // could have disagreed about a shadowed label.) The prefix pass keeps
        // `act Switch to session` working without spelling out the whole row.
        let rows = self.app.available_actions();
        let action = rows
            .iter()
            .find(|(l, _)| l == label)
            .or_else(|| rows.iter().find(|(l, _)| l.starts_with(label)))
            .map(|(_, action)| action.clone());
        match action {
            Some(action) => {
                Shell::act(self, action);
                true
            },
            None => false,
        }
    }

    /// Turnstone's quiescence report, driving the `wait` verb. Busy while any of
    /// the work a scenario must not race is outstanding:
    ///
    /// - a workshop is requested or owns its native child window,
    /// - a capture is queued or its owned readback is pending,
    /// - a page, favicon, subresource, or submission FETCH is in flight (the
    ///   port has not answered; favicon failures remain UI-silent),
    /// - a content spawn is `Requested` (the effect is out, no session yet),
    /// - a live session is not `settled()` (script work or layout pending).
    ///
    /// Deliberately conservative in both directions. It reports `Some` always
    /// (turnstone DOES report quiescence, even when the honest answer is "idle"),
    /// and it counts a spawn as busy from the effect rather than from the
    /// session, so the gap between them cannot read as quiet.
    fn busy(&mut self) -> Option<bool> {
        if super::appearance_editor::workshop_pending(
            self.theme_editor_requested,
            self.theme_editor.is_some(),
        ) {
            // Shared `wait` yields to winit while the child owns native work.
            // The legacy synchronous file wait cannot drive a child window.
            return Some(true);
        }
        if self.pending_capture.is_some()
            || self.pending_lens_capture.is_some()
            || self.pending_capture_readback.is_some()
        {
            return Some(true);
        }
        // Keep the hot `wait` poll allocation-free and short-circuiting. The
        // explicit `record-idle` and content-ready assertions call
        // `idle_diagnosis` when a receipt needs every concurrent cause.
        if self.pending_fetches.any_in_flight() {
            return Some(true);
        }
        if self.app.content.any_requested() {
            return Some(true);
        }
        if self.app.graph_runtimes.is_settling() {
            return Some(true);
        }
        // Wait polls also advance clocks when a hidden finite animation has
        // stopped asking the render loop for frames.
        let _ = self.pump_visible_documents();
        Some(
            self.content_sessions
                .pool
                .pending_contents()
                .next()
                .is_some(),
        )
    }

    fn press(&mut self, x: f32, y: f32) {
        self.deliver_press(x, y, MouseButton::Left);
    }

    fn moved(&mut self, x: f32, y: f32) {
        self.deliver_move(x, y);
    }

    fn release(&mut self, x: f32, y: f32) {
        self.deliver_release(x, y, MouseButton::Left);
    }
}

/// The `Driveable` half: the two things the shared taproot scenario loop
/// cannot do itself. `capture` queues a screenshot the next render fulfills (into
/// the active shared run's dir); `app_step` is left at its default (unknown verb
/// fails loudly) — turnstone's ~30 app-specific verbs are the coordinated
/// follow-on, homed here when the harness fully retires `scenario.rs`. Until
/// then the shared loop drives turnstone through its generic verbs, proving the
/// two grammars are one loop.
impl taproot::Driveable for Shell {
    fn capture(&mut self, name: &str) -> bool {
        self.pending_capture = Some(self.shared_out_dir.join(format!("{name}.png")));
        self.request_redraw();
        true
    }

    /// turnstone's app-specific verbs, reached when the shared grammar passes a
    /// line through. The whole vocabulary now: parse the line with turnstone's own
    /// parser and run it against the Shell via `run_scenario_step`. An unknown
    /// verb fails loudly (parse returns Err), never a silent skip.
    fn app_step(&mut self, line: &str) -> Result<(), String> {
        if let Some(args) = line.strip_prefix("inline-a11y-window ") {
            let (slot, action, selector) = crate::frame_a11y::fixture_body_request(args)?;
            self.push_a11y_tree();
            let (nodes, routes) = if slot == 0 {
                let projected = self.projected_a11y_tree();
                (projected.tree.nodes, projected.routes)
            } else {
                let lens = self
                    .lens_windows
                    .values()
                    .find(|lens| lens.ordinal + 1 == slot)
                    .ok_or("inline window is absent")?;
                (
                    lens.a11y_tree
                        .clone()
                        .ok_or("inline window tree is unpublished")?
                        .nodes,
                    lens.a11y_routes.clone(),
                )
            };
            let (handle, layout) = {
                let frames = self.outer_frames.borrow();
                let frame = frames.get(&slot).ok_or("inline frame is absent")?;
                (frame.controls.dom.clone(), frame.layout.clone())
            };
            let dom = handle.borrow();
            let matches = layout.borrow().matching_nodes(&dom, &selector);
            let targets = routes
                .iter()
                .filter_map(|(id, route)| match route {
                    crate::a11y::A11yRoute::Frame(crate::frame_controls::FrameRoute {
                        control: crate::frame_controls::Control::Body { node, .. },
                        ..
                    }) if matches.contains(node)
                        && nodes
                            .iter()
                            .any(|(n, current)| n == id && current.supports_action(action)) =>
                    {
                        Some(*id)
                    },
                    _ => None,
                })
                .collect::<Vec<_>>();
            drop(dom);
            if targets.len() != 1 {
                return Err("inline target is absent, ambiguous or unsupported".into());
            }
            let request = accesskit::ActionRequest {
                action,
                target_tree: accesskit::TreeId::ROOT,
                target_node: targets[0],
                data: None,
            };
            if slot == 0 {
                self.a11y_routes = routes;
                self.a11y_actions
                    .lock()
                    .map_err(|_| "inline queue poisoned")?
                    .enqueue(request);
                self.drain_a11y_actions();
            } else {
                self.lens_windows
                    .values_mut()
                    .find(|lens| lens.ordinal + 1 == slot)
                    .unwrap()
                    .a11y_actions
                    .lock()
                    .map_err(|_| "inline queue poisoned")?
                    .enqueue(request);
                self.drain_lens_a11y_actions();
            }
            return Ok(());
        }
        if let Some(args) = line.strip_prefix("frame-a11y-window ") {
            let (slot, action, target) = crate::frame_a11y::fixture_window_request(args)?;
            if slot == 0 {
                return self.app_step(&format!(
                    "frame-a11y {}",
                    args.split_once(char::is_whitespace).unwrap().1
                ));
            }
            self.push_a11y_tree();
            let lens = self
                .lens_windows
                .values_mut()
                .find(|lens| lens.ordinal + 1 == slot)
                .ok_or("frame window is absent")?;
            let tree = lens.a11y_tree.clone().ok_or("lens tree is unpublished")?;
            let matches = lens
                .a11y_routes
                .iter()
                .filter_map(|(id, route)| match route {
                    crate::a11y::A11yRoute::Frame(route)
                        if target.matches(&route.control)
                            && tree
                                .nodes
                                .iter()
                                .any(|(node, n)| node == id && n.supports_action(action)) =>
                    {
                        Some(*id)
                    },
                    _ => None,
                })
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err("lens frame target is absent, ambiguous or unsupported".into());
            }
            lens.a11y_actions
                .lock()
                .map_err(|_| "lens queue poisoned")?
                .enqueue(accesskit::ActionRequest {
                    action,
                    target_tree: accesskit::TreeId::ROOT,
                    target_node: matches[0],
                    data: None,
                });
            self.drain_lens_a11y_actions();
            return Ok(());
        }
        if let Some(args) = line.strip_prefix("frame-a11y ") {
            let (action, target) = crate::frame_a11y::fixture_request(args)?;
            let super::document_a11y::Projected { tree, routes, .. } = self.projected_a11y_tree();
            let matches = routes
                .iter()
                .filter_map(|(id, route)| match route {
                    crate::a11y::A11yRoute::Frame(route)
                        if target.matches(&route.control)
                            && tree
                                .nodes
                                .iter()
                                .any(|(node, n)| node == id && n.supports_action(action)) =>
                    {
                        Some(*id)
                    },
                    _ => None,
                })
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err(
                    "frame-a11y target is absent, ambiguous or does not support that action".into(),
                );
            }
            self.a11y_routes = routes;
            self.a11y_actions
                .lock()
                .map_err(|_| "a11y queue poisoned")?
                .enqueue(accesskit::ActionRequest {
                    action,
                    target_tree: accesskit::TreeId::ROOT,
                    target_node: matches[0],
                    data: None,
                });
            self.drain_a11y_actions();
            return Ok(());
        }

        if let Some(size) = line.strip_prefix("resize ") {
            let mut values = size.split_whitespace();
            let width: f64 = values
                .next()
                .ok_or("resize needs width height")?
                .parse()
                .map_err(|_| "invalid resize width")?;
            let height: f64 = values
                .next()
                .ok_or("resize needs width height")?
                .parse()
                .map_err(|_| "invalid resize height")?;
            if values.next().is_some()
                || !width.is_finite()
                || !height.is_finite()
                || width <= 0.0
                || height <= 0.0
            {
                return Err("resize needs two positive finite dimensions".into());
            }
            let window = self.window.as_ref().ok_or("main window is unavailable")?;
            let _ = window.request_inner_size(winit::dpi::LogicalSize::new(width, height));
            return Ok(());
        }
        tracing::debug!(target: "turnstone::selfdrive", verb = line.split_whitespace().next().unwrap_or(""), "begin app step");
        let step = crate::scenario::parse(line)?
            .into_iter()
            .next()
            .ok_or_else(|| format!("app_step: empty line '{line}'"))?;
        let result = self.run_scenario_step(&step);
        tracing::debug!(target: "turnstone::selfdrive", ok = result.is_ok(), "finished app step");
        result
    }
}

impl Shell {
    /// Execute one turnstone scenario step against the Shell — the app-specific
    /// verbs the shared taproot loop hands to `Driveable::app_step`. This is
    /// turnstone's former `scenario.rs` `tick()` (asserts) and `scenario_pump`'s
    /// `Tick` execution (interactions), unified into one pass: an assert reads
    /// the observation snapshot and returns `Err` on mismatch; an interaction
    /// drives the Shell directly. The generic verbs (act/settle/capture/log,
    /// assert event/text/snap) never arrive — the shared loop owns them; their
    /// arms below are defensive.
    fn run_scenario_step(&mut self, step: &crate::scenario::Step) -> Result<(), String> {
        use crate::action::CaretMove;
        use crate::scenario::{CmpOp, EditKey, Step};

        fn cmp_usize(op: &CmpOp, a: usize, b: usize) -> bool {
            match op {
                CmpOp::Eq => a == b,
                CmpOp::Ge => a >= b,
                CmpOp::Le => a <= b,
            }
        }
        fn cmp_f32(op: &CmpOp, a: f32, b: f32) -> bool {
            match op {
                CmpOp::Eq => (a - b).abs() < 1e-3,
                CmpOp::Ge => a >= b,
                CmpOp::Le => a <= b,
            }
        }

        match step {
            // ---- interactions: drive the Shell (the former Tick execution) ----
            Step::Open(url) => self.act(Action::OpenAddress(url.clone())),
            Step::SelectGraphMember(url) => {
                let pane = self.app.graph_command_pane().ok_or("no graph owner pane")?;
                self.app
                    .with_graph_pane(pane, |canvas| {
                        if url == "none" {
                            canvas.clear_selection();
                            return Ok(());
                        }
                        let id = canvas
                            .graph()
                            .get_node_by_url(url)
                            .map(|(_, node)| node.id)
                            .ok_or("source member absent")?;
                        if canvas.fold_of_member(id).is_some() {
                            return Err("member already folded");
                        }
                        canvas.toggle_select_member(id);
                        Ok(())
                    })
                    .ok_or("graph absent")??;
                self.app.focus = crate::surface::FocusTarget::Graph(pane);
                self.request_redraw();
            },
            Step::ActivateGraphFold(index) => {
                let pane = self.app.graph_command_pane().ok_or("no graph owner pane")?;
                let badge = self
                    .app
                    .with_graph_pane(pane, |canvas| canvas.fold_badges().get(*index).cloned())
                    .flatten()
                    .ok_or("fold badge absent")?;
                self.app.graph_pane_pointer_down(
                    pane,
                    mere::canvas::PointerButton::Left,
                    badge.at.0,
                    badge.at.1,
                );
                self.release_graph_pointer(
                    pane,
                    mere::canvas::PointerButton::Left,
                    badge.at.0,
                    badge.at.1,
                );
                self.request_redraw();
            },
            Step::RecordGraphScene(name) => {
                let pane = self.app.scene_command_pane().ok_or("no graph owner pane")?;
                let graph = self.app.graph_for_pane(pane).ok_or("graph absent")?;
                let value = self.app.with_graph_pane(pane, |canvas| {
                    serde_json::json!({
                        "facts": crate::graph_scenes::facts(canvas),
                        "geometry": canvas.cartography_geometry(),
                        "graph": canvas.graph().to_snapshot_at(0),
                        "folds": canvas.fold_records(),
                        "physics_paused": canvas.physics_paused(),
                        "selected": canvas.selected_members(),
                        "graph_id": graph.as_uuid(),
                    })
                }).ok_or("graph absent")?;
                let runtime = self.app.graph_runtimes.get(graph).ok_or("runtime absent")?;
                let session = runtime.session.ok_or("session absent")?;
                let dir = crate::session::session_dir(&self.app.data_root, session);
                let mut value = value;
                value["main_presented_frames"] = serde_json::json!(self.main_receipt.presented);
                for file in ["facets.json", "view-intent.json"] {
                    if let Ok(bytes) = std::fs::read(dir.join(file)) {
                        value[format!("stored_{file}")] = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                    }
                }
                std::fs::write(self.shared_out_dir.join(format!("{name}.scene.json")),
                    serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            },
            Step::RecordGraphFolds(name, expected) => {
                let pane = self.app.graph_command_pane().ok_or("no graph owner pane")?;
                let graph = self.app.graph_for_pane(pane).ok_or("graph absent")?;
                let value = self.app.with_graph_pane(pane, |canvas| {
                    if canvas.fold_records().len() != *expected {
                        return Err(format!("expected {expected} folds, got {}", canvas.fold_records().len()));
                    }
                    let mut positions: Vec<_> = canvas.graph().nodes().map(|(key, node)| {
                        let pos = canvas.world_position_of(key).map(|p| [p.x, p.y]);
                        (node.id.to_string(), pos)
                    }).collect();
                    positions.sort_by(|a, b| a.0.cmp(&b.0));
                    let routes: Vec<_> = canvas.fold_routes().iter().map(|route| {
                        serde_json::json!({
                            "from": format!("{:?}", route.from),
                            "to": format!("{:?}", route.to),
                            "family": format!("{:?}", route.family),
                            "count": route.count,
                        })
                    }).collect();
                    let relations: Vec<_> = canvas.graph().projected_relations().map(|(_, relation)| {
                        serde_json::json!({
                            "from": format!("{:?}", relation.from),
                            "to": format!("{:?}", relation.to),
                            "family": format!("{:?}", relation.kind.family()),
                        })
                    }).collect();
                    let member_keys: Vec<_> = canvas.graph().nodes().map(|(key, node)| {
                        serde_json::json!({"id": node.id, "key": format!("{:?}", key)})
                    }).collect();
                    Ok(serde_json::json!({
                        "member_keys": member_keys,
                        "graph": canvas.graph().to_snapshot_at(0),
                        "positions": positions,
                        "records": canvas.fold_records(),
                        "routes": routes,
                        "relations": relations,
                        "hidden": canvas.fold_hidden_keys().len(),
                        "physics_paused": canvas.physics_paused(),
                    }))
                }).ok_or("graph absent")??;
                let runtime = self.app.graph_runtimes.get(graph).ok_or("runtime absent")?;
                let session = runtime.session.ok_or("session absent")?;
                let dir = crate::session::session_dir(&self.app.data_root, session);
                let saved: serde_json::Value = serde_json::from_slice(
                    &std::fs::read(dir.join("folds.json")).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                if saved["records"] != value["records"] {
                    return Err("native and stored fold records differ".into());
                }
                let mut value = value;
                value["stored"] = saved;
                value["presented_frames"] = serde_json::json!(self.main_receipt.presented);
                for file in ["graph.json", "facets.json"] {
                    let bytes = std::fs::read(dir.join(file)).map_err(|e| e.to_string())?;
                    value[format!("stored_{file}_blake3")] =
                        serde_json::json!(blake3::hash(&bytes).to_hex().to_string());
                }
                value["graph_id"] = serde_json::json!(graph.as_uuid());
                value["session_id"] = serde_json::json!(session.as_uuid());
                std::fs::write(
                    self.shared_out_dir.join(format!("{name}.folds.json")),
                    serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
            },
            Step::ImportClip(path) => {
                self.act(Action::ImportClipPackage(std::path::PathBuf::from(path)))
            },
            Step::ExportClip(path) => {
                let clip = self.app.clip_export_target()?;
                self.act(Action::ExportClipPackage {
                    clip,
                    path: path.into(),
                    replace: false,
                });
            },
            Step::Omnibar { command } => self.act(Action::OmnibarOpen { command: *command }),
            Step::Type(text) => {
                if self.app.user_agent_decision.is_open() {
                    if self.app.user_agent_decision.accepts_text() {
                        self.act(Action::InsertAuthentication(text.clone()));
                    } else {
                        return Err("type: the active decision has no text field".into());
                    }
                } else if self.app.document_find.open {
                    for character in text.chars() {
                        let key = winit::keyboard::Key::Character(character.to_string().into());
                        if let Some(event) = self.cambium_key(&key) {
                            self.deliver_find_key(event);
                        }
                    }
                } else if !self.app.omnibar.open && self.type_surface_text(text) {
                    // Commit through the same producer key stream as winit.
                } else {
                    for c in text.chars() {
                        self.act(Action::OmnibarChar(c));
                    }
                }
            },
            Step::Insert(text) => {
                if self.app.user_agent_decision.is_open() {
                    if self.app.user_agent_decision.accepts_text() {
                        self.act(Action::InsertAuthentication(text.clone()));
                    } else {
                        return Err("insert: the active decision has no text field".into());
                    }
                } else if self.app.document_find.open {
                    self.deliver_find_key(cambium_winit::ime_event_from_winit(
                        &winit::event::Ime::Commit(text.clone()),
                    ));
                } else if self.app.omnibar.open {
                    self.act(Action::OmnibarInsert(text.clone()));
                } else if self.deliver_contributed_ime(&winit::event::Ime::Commit(text.clone())) {
                    self.request_redraw();
                } else if self.deliver_document_ime(&winit::event::Ime::Commit(text.clone())) {
                    self.request_redraw();
                } else if self.type_surface_text(text) {
                    // Hosted committed text is distinct from OS IME/preedit.
                } else {
                    return Err("insert: no focused text editor".into());
                }
            },
            Step::Key(key) => self.scenario_key_in_window(0, key)?,
            Step::KeyWindow(window, key) => self.scenario_key_in_window(*window, key)?,
            Step::Script(source) => self.run_scenario_script(source),
            Step::Click(x, y) => {
                self.deliver_press(*x, *y, MouseButton::Left);
                self.deliver_release(*x, *y, MouseButton::Left);
            },
            Step::Press(x, y) => self.deliver_press(*x, *y, MouseButton::Left),
            Step::Release(x, y) => self.deliver_release(*x, *y, MouseButton::Left),
            Step::Move(x, y) => self.deliver_move(*x, *y),
            Step::Touch(id, phase, x, y, pressure) => {
                self.deliver_touch_contact(*id, *phase, *x, *y, *pressure, None)
            },
            Step::RightClick(x, y) => {
                self.deliver_press(*x, *y, MouseButton::Right);
                self.deliver_release(*x, *y, MouseButton::Right);
            },
            Step::ClickRow(substr) => self.click_pane_row(substr),
            Step::ClickTab(label) => self.click_pane_tab(label),
            Step::ClickNode(substr) => self.click_pane_node(substr),
            Step::Drag(from, to) => {
                self.deliver_press(from.0, from.1, MouseButton::Left);
                let mid = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
                self.deliver_move(mid.0, mid.1);
                self.deliver_move(to.0, to.1);
                self.deliver_release(to.0, to.1, MouseButton::Left);
            },
            Step::DragTab(from, onto, edge) => {
                self.drag_workbench_tab(from, onto, edge.as_deref());
            },
            Step::DragTabOut(from) => self.drag_workbench_tab_out(from),
            Step::HoverFile(x, y, path) => self.hover_file(*x, *y, std::path::Path::new(path)),
            Step::DropFile(x, y, path) => self.drop_file(*x, *y, std::path::Path::new(path)),
            Step::Scroll(x, y, dx, dy) => self.deliver_wheel(*x, *y, *dx, *dy),
            Step::ScrollReader(role, dy) => {
                let role = super::reader_observe::ReaderAppearanceRole::parse(role)
                    .ok_or_else(|| format!("scroll-reader: unknown Reader role '{role}'"))?;
                self.scroll_reader_appearance(role, *dy)?;
            },
            Step::Divider(ratio) => self.act(Action::SetActivePaneDivider(*ratio)),
            Step::RecordReaderAppearances(name) => {
                let observations = self.reader_appearance_observations();
                let path = self.shared_out_dir.join(format!("{name}.txt"));
                std::fs::write(
                    &path,
                    format!(
                        "RESULT ok\n{}\n",
                        Self::describe_reader_appearances(&observations)
                    ),
                )
                .map_err(|error| {
                    format!(
                        "record-reader-appearances '{}': could not write {}: {error}",
                        name,
                        path.display()
                    )
                })?;
            },
            Step::RecordIdle(name) => {
                let public_fixture = public_fixture_a11y_receipt()?;
                let diagnosis = self.idle_diagnosis();
                let path = self.shared_out_dir.join(format!("{name}.txt"));
                std::fs::write(&path, format!("RESULT ok\n{}\n", diagnosis.describe())).map_err(
                    |error| {
                        format!(
                            "record-idle '{}': could not write {}: {error}",
                            name,
                            path.display()
                        )
                    },
                )?;
                let path = self
                    .shared_out_dir
                    .join(format!("{name}.foreign-a11y.json"));
                let mut diagnostic = self.surface_a11y.publication_diagnostic(public_fixture);
                #[cfg(all(feature = "servo", windows))]
                if let Some(factory) = &self.servo_factory {
                    diagnostic["servo_focus_policy"] =
                        serde_json::json!(factory.a11y_focus_policy());
                }
                let bytes =
                    serde_json::to_vec_pretty(&diagnostic).map_err(|error| error.to_string())?;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|error| {
                        format!(
                            "record-idle '{name}': refusing to overwrite or unable to create {}: {error}",
                            path.display()
                        )
                    })?;
                std::io::Write::write_all(&mut file, &bytes).map_err(|error| error.to_string())?;
                #[cfg(all(any(feature = "scry", feature = "servo"), windows))]
                {
                    let surfaces = self.surface_plan().into_iter().filter_map(|surface| {
                        let crate::surface::SurfaceKind::Content(node) = surface.kind else { return None; };
                        let rect = surface.rect;
                        Some(serde_json::json!({"node":node,"surface_id":surface.id.0,"x":rect.x,"y":rect.y,"width":rect.w,"height":rect.h}))
                    }).collect::<Vec<_>>();
                    #[cfg(feature = "scry")]
                    let stats = self.scry_frame_importers.iter().map(|(node, importer)| {
                        let stats = importer.stats();
                        serde_json::json!({"node":node,"fence_handle":importer.fence_handle(),"frames":stats.frames,"imports":stats.imports,"waits":stats.waits})
                    }).collect::<Vec<_>>();
                    #[cfg(not(feature = "scry"))]
                    let stats: Vec<serde_json::Value> = Vec::new();
                    #[cfg(feature = "servo")]
                    let servo_views = super::servo::active_views();
                    #[cfg(not(feature = "servo"))]
                    let servo_views = 0usize;
                    std::fs::write(self.shared_out_dir.join(format!("{name}.surface-frames.json")),
                        serde_json::to_vec_pretty(&serde_json::json!({"live_producers":self.content_sessions.surface_count(),"cached_frames":self.surface_frames.len(),"scry_importers":stats,"servo_active_views":servo_views,"content_surfaces":surfaces})).map_err(|error|error.to_string())?)
                        .map_err(|error|error.to_string())?;
                }
            },

            // ---- asserts: read the snapshot, Err on mismatch (former tick) ----
            Step::AssertOmnibar(open) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.omnibar.open != *open {
                    let state = if *open { "open" } else { "closed" };
                    return Err(format!("assert omnibar {state}: it is not"));
                }
            },
            Step::AssertScrolled(want_moved) => match self.content_scroll_moved {
                Some(moved) if moved == *want_moved => {},
                Some(_) => {
                    let (want, got) = if *want_moved {
                        ("moved", "did not")
                    } else {
                        ("still", "moved")
                    };
                    return Err(format!("assert scrolled {want}: the focused page {got}"));
                },
                None => {
                    return Err(
                        "assert scrolled: no content scroll key has been delivered".to_string()
                    );
                },
            },
            Step::AssertText(want) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.omnibar.text != *want {
                    return Err(format!(
                        "assert omnibar-text '{want}': the omnibar holds '{}'",
                        snap.omnibar.text
                    ));
                }
            },
            Step::AssertFocused(substr) => {
                let needle = substr.to_lowercase();
                let snap = crate::observe::snapshot(&self.app);
                let hay = snap
                    .focused
                    .map(|f| format!("{} {}", f.url, f.caption).to_lowercase())
                    .unwrap_or_default();
                if !hay.contains(&needle) {
                    return Err(format!("assert focused '{substr}': focused is '{hay}'"));
                }
            },
            Step::AssertBrowserFetch(substr) => {
                let actual = crate::observe::snapshot(&self.app)
                    .browser_fetch
                    .unwrap_or_else(|| "none".to_string());
                if !actual.contains(substr) {
                    return Err(format!(
                        "assert fetch '{substr}': focused browser fetch is '{actual}'"
                    ));
                }
            },
            Step::AssertLinkPreview(substr) => {
                let actual = crate::observe::snapshot(&self.app)
                    .link_preview
                    .unwrap_or_else(|| "none".to_string());
                if !actual.contains(substr) {
                    return Err(format!(
                        "assert link-preview '{substr}': preview is '{actual}'"
                    ));
                }
            },
            Step::AssertSurface(kind) => {
                let snap = crate::observe::snapshot(&self.app);
                if !snap.surfaces.iter().any(|s| s == kind) {
                    return Err(format!(
                        "assert surface '{kind}': the plan is {:?}",
                        snap.surfaces
                    ));
                }
            },
            Step::AssertFocus(kind) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.focus != *kind {
                    return Err(format!("assert focus '{kind}': focus is '{}'", snap.focus));
                }
            },
            Step::AssertPane(tag) => {
                let snap = crate::observe::snapshot(&self.app);
                if !snap.panes.iter().any(|p| p == tag) {
                    return Err(format!(
                        "assert pane '{tag}': the tree holds {:?}",
                        snap.panes
                    ));
                }
            },
            Step::AssertMaximized(want) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.maximized != *want {
                    let state = if *want { "maximized" } else { "not maximized" };
                    return Err(format!("assert {state}: it is not"));
                }
            },
            Step::AssertNoRow(substr) => {
                let snap = crate::observe::snapshot(&self.app);
                let hit = snap
                    .trail_rows
                    .iter()
                    .chain(snap.roster_rows.iter())
                    .chain(snap.inspector_rows.iter())
                    .find(|r| r.contains(substr));
                if let Some(row) = hit {
                    return Err(format!(
                        "assert no-row '{substr}': a row still has it: '{row}'"
                    ));
                }
            },
            Step::AssertRow(substr) => {
                let snap = crate::observe::snapshot(&self.app);
                let hit = snap
                    .trail_rows
                    .iter()
                    .chain(snap.roster_rows.iter())
                    .chain(snap.inspector_rows.iter())
                    .chain(snap.arrange_rows.iter())
                    .any(|r| r.contains(substr));
                if !hit {
                    return Err(format!(
                        "assert row '{substr}': trail {:?} roster {:?} inspector {:?} arrange {:?}",
                        snap.trail_rows, snap.roster_rows, snap.inspector_rows, snap.arrange_rows
                    ));
                }
            },
            Step::AssertTab(want) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.roster_tab != want {
                    return Err(format!(
                        "assert tab '{want}': the Roster is on '{}'",
                        snap.roster_tab
                    ));
                }
            },
            Step::AssertRatio(op, want) => {
                let snap = crate::observe::snapshot(&self.app);
                let ok = snap.split_ratio.is_some_and(|r| cmp_f32(op, r, *want));
                if !ok {
                    return Err(format!(
                        "assert ratio {op:?} {want}: the root split is {:?}",
                        snap.split_ratio
                    ));
                }
            },
            Step::AssertActiveRatio(op, want) => {
                let snap = crate::observe::snapshot(&self.app);
                let ok = snap.active_ratio.is_some_and(|r| cmp_f32(op, r, *want));
                if !ok {
                    return Err(format!(
                        "assert active-ratio {op:?} {want}: the active pane's split is {:?}",
                        snap.active_ratio
                    ));
                }
            },
            Step::AssertSuggestions(op, n) => {
                let snap = crate::observe::snapshot(&self.app);
                let len = snap.omnibar.suggestions.len();
                if !cmp_usize(op, len, *n) {
                    return Err(format!(
                        "assert suggestions: have {len} ({:?}), wanted {op:?} {n}",
                        snap.omnibar.suggestions
                    ));
                }
            },
            Step::AssertVisible => {
                if !crate::observe::snapshot(&self.app).graph_visible {
                    return Err("assert visible: every node is off-screen".to_string());
                }
            },
            Step::AssertContentLive => {
                let snap = crate::observe::snapshot(&self.app);
                let focused = snap.focused.as_ref().map(|f| f.member);
                let state = focused
                    .and_then(|id| snap.content.iter().find(|(n, _)| *n == id))
                    .map(|(_, s)| s.clone());
                if state.as_deref() != Some("live") {
                    return Err(format!(
                        "assert content-live: focused node is {}",
                        state.unwrap_or_else(|| "without content state".to_string())
                    ));
                }
            },
            Step::AssertWbCells(op, n) => {
                let snap = crate::observe::snapshot(&self.app);
                if !cmp_usize(op, snap.workbench_cells.len(), *n) {
                    return Err(format!(
                        "assert wb-cells: have {} ({:?}), wanted {op:?} {n}",
                        snap.workbench_cells.len(),
                        snap.workbench_cells
                    ));
                }
            },
            Step::AssertWbCell(substr) => {
                let snap = crate::observe::snapshot(&self.app);
                if !snap.workbench_cells.iter().any(|c| c.contains(substr)) {
                    return Err(format!(
                        "assert wb-cell '{substr}': the cells are {:?}",
                        snap.workbench_cells
                    ));
                }
            },
            Step::AssertWbFraction(op, want) => {
                let snap = crate::observe::snapshot(&self.app);
                let ok = snap
                    .workbench_fractions
                    .first()
                    .is_some_and(|f| cmp_f32(op, *f, *want));
                if !ok {
                    return Err(format!(
                        "assert wb-fraction {op:?} {want}: the root fractions are {:?}",
                        snap.workbench_fractions
                    ));
                }
            },
            Step::AssertWindows(op, n) => {
                let snap = crate::observe::snapshot(&self.app);
                if !cmp_usize(op, snap.windows, *n) {
                    return Err(format!("assert windows {op:?} {n}: have {}", snap.windows));
                }
            },
            Step::AssertSessions(op, n) => {
                let snap = crate::observe::snapshot(&self.app);
                if !cmp_usize(op, snap.session_count, *n) {
                    return Err(format!(
                        "assert sessions {op:?} {n}: have {}",
                        snap.session_count
                    ));
                }
            },
            Step::AssertSession(substr) => {
                let snap = crate::observe::snapshot(&self.app);
                if !snap.session.to_lowercase().contains(&substr.to_lowercase()) {
                    return Err(format!(
                        "assert session '{substr}': the live session is '{}'",
                        snap.session
                    ));
                }
            },
            Step::AssertNodes(op, n) => {
                let snap = crate::observe::snapshot(&self.app);
                if !cmp_usize(op, snap.node_count, *n) {
                    return Err(format!("assert nodes {op:?} {n}: have {}", snap.node_count));
                }
            },
            Step::AssertEnergy(op, n) => {
                let snap = crate::observe::snapshot(&self.app);
                if !cmp_f32(op, snap.physics_energy, *n) {
                    return Err(format!(
                        "assert energy {op:?} {n}: have {:.1}",
                        snap.physics_energy
                    ));
                }
            },
            Step::AssertPhysicsRunning(want) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.physics_paused == *want {
                    return Err(format!(
                        "assert physics {}: it is {}",
                        if *want { "running" } else { "paused" },
                        if snap.physics_paused {
                            "paused"
                        } else {
                            "running"
                        }
                    ));
                }
            },
            Step::AssertOverlaps(op, n) => {
                let snap = crate::observe::snapshot(&self.app);
                if !cmp_usize(op, snap.layout_overlaps, *n) {
                    return Err(format!(
                        "assert overlaps {op:?} {n}: have {}",
                        snap.layout_overlaps
                    ));
                }
            },
            Step::AssertA11y(substr) => {
                self.push_a11y_tree();
                let mut lines = self.surface_a11y.observed_lines();
                let mut lenses: Vec<_> = self.lens_windows.values().collect();
                lenses.sort_by_key(|lens| lens.ordinal);
                for lens in lenses {
                    lines.extend(lens.a11y_forest.observed_lines());
                }
                if !crate::a11y::matches_lines(&lines, substr) {
                    return Err(format!(
                        "assert a11y '{substr}': {} lines, none match (first 12: {:?})",
                        lines.len(),
                        lines.iter().take(12).collect::<Vec<_>>()
                    ));
                }
            },
            Step::CapturesDiffer(first, second) => {
                let digest = |name: &str| {
                    self.main_receipt
                        .captures
                        .iter()
                        .find_map(|(path, digest, _)| {
                            (path.file_stem() == Some(std::ffi::OsStr::new(name)))
                                .then_some(*digest)
                        })
                        .ok_or_else(|| format!("capture '{name}' has no completed native readback"))
                };
                if digest(first)? == digest(second)? {
                    return Err(format!(
                        "native captures '{first}' and '{second}' have identical pixels"
                    ));
                }
            },
            Step::ScrollPaneWindow(window, x, y, dx, dy) => {
                if !self.lens_pane_wheel(*window, *x, *y, *dx, *dy) {
                    return Err(format!(
                        "window {window}: no current pane owns wheel at {x},{y}"
                    ));
                }
            },

            // ---- generic verbs the shared loop owns; never reached, defensive ----
            Step::Act(label) => {
                if !taproot::Automatable::act(self, label) {
                    return Err(format!("act: no palette action labelled '{label}'"));
                }
            },
            Step::Settle(_) | Step::Log(_) => {},
            Step::QueueAct(label) => {
                if self.scenario_queued_action.is_some() {
                    return Err("queue-act: an Action is already waiting for delivery".into());
                }
                let rows = self.app.available_actions();
                let action = rows.iter()
                    .find(|(candidate, _)| candidate == label)
                    .or_else(|| rows.iter().find(|(candidate, _)| candidate.starts_with(label)))
                    .map(|(_, action)| action.clone())
                    .ok_or_else(|| format!("queue-act: no palette action labelled '{label}'"))?;
                self.scenario_queued_action = Some(action);
            },
            Step::DeliverAct => {
                let action = self.scenario_queued_action.take()
                    .ok_or("deliver-act: no catalog Action is waiting")?;
                Shell::act(self, action);
            },
            Step::Capture(name) => {
                self.pending_capture = Some(self.shared_out_dir.join(format!("{name}.png")));
            },
            Step::CaptureLens(name) => {
                // A lens capture lands on the LENS's own redraw, so it cannot
                // report success here. What it can do is refuse the impossible
                // case: with no lens open, the pending path would simply never
                // be consumed and the receipt would pass having written
                // nothing. That silent hole is exactly how a `capture-lens`
                // after a session switch (which closes the outgoing session's
                // windows) looked green while producing no pixels.
                if self.lens_windows.is_empty() {
                    return Err(format!(
                        "capture-lens '{name}': no lens window is open to capture"
                    ));
                }
                self.pending_lens_capture = Some(self.shared_out_dir.join(format!("{name}.png")));
                self.pending_lens_capture_window = None;
                self.bind_lens_capture();
                // The lens presents on its own redraw; nudge every window so
                // the pending capture lands this pump.
                self.request_redraw();
            },
            Step::AssertLensPane(substr) => {
                let snap = crate::observe::snapshot(&self.app);
                if !snap.lens_panes.iter().any(|p| p.contains(substr)) {
                    return Err(format!(
                        "assert lens-pane '{substr}': the lens spaces hold {:?}",
                        snap.lens_panes
                    ));
                }
            },
            Step::AssertNoPane(tag) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.panes.iter().any(|p| p == tag) {
                    return Err(format!(
                        "assert no-pane '{tag}': the primary tree still holds {:?}",
                        snap.panes
                    ));
                }
            },
            Step::AssertLensSurface(kind) => {
                // The first lens window's LIVE plan — the same one its render
                // and input use — so a green assert certifies what that window
                // actually composites.
                let Some((lw, lh, ordinal)) = self
                    .lens_windows
                    .values()
                    .next()
                    .map(|l| (l.width, l.height, l.ordinal))
                else {
                    return Err(format!("assert lens-surface '{kind}': no lens window"));
                };
                let plan = self.lens_plan(ordinal, lw, lh);
                if !plan.iter().any(|s| s.kind.label() == kind) {
                    let kinds: Vec<_> = plan.iter().map(|s| s.kind.label()).collect();
                    return Err(format!(
                        "assert lens-surface '{kind}': the lens plan is {kinds:?}"
                    ));
                }
            },
            Step::AssertNoLensPane(substr) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.lens_panes.iter().any(|p| p.contains(substr)) {
                    return Err(format!(
                        "assert no-lens-pane '{substr}': the lens spaces hold {:?}",
                        snap.lens_panes
                    ));
                }
            },
            Step::AssertNoSurface(kind) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.surfaces.iter().any(|s| s == kind) {
                    return Err(format!(
                        "assert no-surface '{kind}': the primary plan is {:?}",
                        snap.surfaces
                    ));
                }
            },
            Step::AssertReaderAppearances(op, want) => {
                let appearances = self.reader_appearance_observations();
                if appearances
                    .windows(2)
                    .any(|pair| pair[0].id.0 >= pair[1].id.0)
                {
                    return Err(format!(
                        "assert reader-appearances: observations are not sorted: {}",
                        Self::describe_reader_appearances(&appearances)
                    ));
                }
                if !cmp_usize(op, appearances.len(), *want) {
                    return Err(format!(
                        "assert reader-appearances: got {}, expected {:?} {}; {}",
                        appearances.len(),
                        op,
                        want,
                        Self::describe_reader_appearances(&appearances)
                    ));
                }
            },
            Step::AssertReaderSources(op, want) => {
                let appearances = self.reader_appearance_observations();
                let sources = appearances
                    .iter()
                    .map(|appearance| appearance.source_group)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len();
                if !cmp_usize(op, sources, *want) {
                    return Err(format!(
                        "assert reader-sources: got {}, expected {:?} {}; {}",
                        sources,
                        op,
                        want,
                        Self::describe_reader_appearances(&appearances)
                    ));
                }
            },
            Step::AssertReaderRole(role) => {
                let role = super::reader_observe::ReaderAppearanceRole::parse(role)
                    .ok_or_else(|| format!("assert reader-role: unknown Reader role '{role}'"))?;
                let appearances = self.reader_appearance_observations();
                let count = appearances
                    .iter()
                    .filter(|appearance| appearance.role == role)
                    .count();
                if count != 1 {
                    return Err(format!(
                        "assert reader-role {}: got {count}; {}",
                        role.label(),
                        Self::describe_reader_appearances(&appearances)
                    ));
                }
            },
            Step::AssertReaderViewport(role) => {
                let role =
                    super::reader_observe::ReaderAppearanceRole::parse(role).ok_or_else(|| {
                        format!("assert reader-viewport: unknown Reader role '{role}'")
                    })?;
                let appearances = self.reader_appearance_observations();
                let matching = appearances
                    .iter()
                    .filter(|appearance| appearance.role == role)
                    .collect::<Vec<_>>();
                let [appearance] = matching.as_slice() else {
                    return Err(format!(
                        "assert reader-viewport {}: expected one appearance; {}",
                        role.label(),
                        Self::describe_reader_appearances(&appearances)
                    ));
                };
                // The reader is laid out in layout pixels, as pelt sizes it.
                let expected = (
                    (appearance.rect.w / appearance.layout_scale).max(1.0).ceil() as u32,
                    (appearance.rect.h / appearance.layout_scale).max(1.0).ceil() as u32,
                );
                if appearance.viewport != expected {
                    return Err(format!(
                        "assert reader-viewport {}: got {}x{}, plan requires {}x{}; {}",
                        role.label(),
                        appearance.viewport.0,
                        appearance.viewport.1,
                        expected.0,
                        expected.1,
                        Self::describe_reader_appearances(&appearances)
                    ));
                }
            },
            Step::AssertReaderScroll(role, op, want) => {
                let role = super::reader_observe::ReaderAppearanceRole::parse(role)
                    .ok_or_else(|| format!("assert reader-scroll: unknown Reader role '{role}'"))?;
                let appearances = self.reader_appearance_observations();
                let matching = appearances
                    .iter()
                    .filter(|appearance| appearance.role == role)
                    .collect::<Vec<_>>();
                let [appearance] = matching.as_slice() else {
                    return Err(format!(
                        "assert reader-scroll {}: expected one appearance; {}",
                        role.label(),
                        Self::describe_reader_appearances(&appearances)
                    ));
                };
                if !cmp_f32(op, appearance.scroll_y, *want) {
                    return Err(format!(
                        "assert reader-scroll {}: got {:.1}, expected {:?} {:.1}; {}",
                        role.label(),
                        appearance.scroll_y,
                        op,
                        want,
                        Self::describe_reader_appearances(&appearances)
                    ));
                }
            },
            Step::AssertContentSessionsIdle => {
                let diagnosis = self.idle_diagnosis();
                if !diagnosis.unsettled_sessions.is_empty() {
                    return Err(format!(
                        "assert content-sessions-idle: {}",
                        diagnosis.describe()
                    ));
                }
            },
            Step::AssertContentReady => {
                let diagnosis = self.idle_diagnosis();
                if !diagnosis.content_ready() {
                    return Err(format!("assert content-ready: {}", diagnosis.describe()));
                }
            },
            Step::AssertEvent(_) => {},

            // ---- T5a: sticky waits and the place record receipt ----
            Step::WaitRow(frames, substr) => {
                let ok = self.wait_frames(*frames, |shell| {
                    let snap = crate::observe::snapshot(&shell.app);
                    snap.trail_rows
                        .iter()
                        .chain(snap.roster_rows.iter())
                        .chain(snap.inspector_rows.iter())
                        .chain(snap.arrange_rows.iter())
                        .any(|r| r.contains(substr.as_str()))
                });
                if !ok {
                    let snap = crate::observe::snapshot(&self.app);
                    return Err(format!(
                        "wait-row '{substr}' after {frames} frames: trail {:?} roster {:?} inspector {:?} arrange {:?}",
                        snap.trail_rows, snap.roster_rows, snap.inspector_rows, snap.arrange_rows
                    ));
                }
            },
            Step::WaitStatus(frames, substr) => {
                self.pending_scenario_wait = Some(PendingScenarioWait::status(substr.clone(), *frames));
            },
            Step::WaitKnot(frames, substr) => {
                fn knot_lines(app: &crate::app::App) -> Vec<String> {
                    app.knot_documents()
                        .iter()
                        .map(|doc| format!("{} {}", doc.status, doc.address))
                        .collect()
                }
                let ok = self.wait_frames(*frames, |shell| {
                    knot_lines(&shell.app)
                        .iter()
                        .any(|line| line.contains(substr.as_str()))
                });
                if !ok {
                    let lines = knot_lines(&self.app);
                    return Err(format!(
                        "wait-knot '{substr}' after {frames} frames: knot documents are {lines:?}"
                    ));
                }
            },
            Step::WaitSourceCapture(frames, ordinal, phase) => {
                let phase_of = |phase: crate::source_capture::CapturePhase| match phase {
                    crate::source_capture::CapturePhase::Depositing => "depositing",
                    crate::source_capture::CapturePhase::Deposited => "deposited",
                    crate::source_capture::CapturePhase::PendingSave => "pending-save",
                    crate::source_capture::CapturePhase::Failed => "failed",
                    crate::source_capture::CapturePhase::Saved => "saved",
                    crate::source_capture::CapturePhase::Recoverable => "recoverable",
                };
                let ok = self.wait_frames(*frames, |shell| {
                    crate::observe::snapshot(&shell.app).source_captures.iter()
                        .any(|capture| capture.ordinal == *ordinal && phase_of(capture.phase) == phase)
                });
                if !ok {
                    let current = crate::observe::snapshot(&self.app).source_captures.iter()
                        .map(|capture| (capture.ordinal, phase_of(capture.phase)))
                        .collect::<Vec<_>>();
                    return Err(format!("wait-source-capture {ordinal} {phase} after {frames} frames: observed {current:?}"));
                }
            },
            Step::WaitSavedSources(frames) => {
                let ready = self.wait_frames(*frames, |shell| {
                    shell.app.saved_sources.request.is_none()
                        && shell.app.saved_sources.status == "Ready"
                });
                if !ready {
                    return Err(format!(
                        "wait-saved-sources after {frames} frames: status is '{}' with request {:?}",
                        self.app.saved_sources.status,
                        self.app.saved_sources.request,
                    ));
                }
            },
            Step::WaitSavedReader(frames) => {
                if !self.wait_frames(*frames, Shell::focused_saved_reader_ready) {
                    let node = self.focused_saved_reader_node();
                    let state = node.and_then(|node| self.app.saved_readers.get(&node)).map(|reader| {
                        if reader.error.is_some() {
                            "Error"
                        } else if reader.request.is_some() || reader.content.is_none() {
                            "Pending"
                        } else {
                            "Content ready; live saved-source engine unavailable"
                        }
                    }).unwrap_or("None");
                    return Err(format!(
                        "wait-saved-reader after {frames} frames: focused reader state is {state}"
                    ));
                }
            },
            Step::WaitFile(frames, path) => {
                self.pending_scenario_wait = Some(PendingScenarioWait::file(path.into(), *frames));
            },
            Step::RecordPlace(name) => {
                let value = crate::observe::place_record(&self.app);
                let body = serde_json::to_string_pretty(&value).map_err(|error| {
                    format!("record-place '{name}': could not serialize: {error}")
                })?;
                let path = self.shared_out_dir.join(format!("{name}.json"));
                std::fs::write(&path, body).map_err(|error| {
                    format!(
                        "record-place '{name}': could not write {}: {error}",
                        path.display()
                    )
                })?;
            },
            Step::RecordSourceCaptures(name) => {
                let snapshot = crate::observe::snapshot(&self.app);
                let captures: Vec<_> = snapshot.source_captures.iter().map(|capture| {
                    let phase = match capture.phase {
                        crate::source_capture::CapturePhase::Depositing => "depositing",
                        crate::source_capture::CapturePhase::Deposited => "deposited",
                        crate::source_capture::CapturePhase::PendingSave => "pending-save",
                        crate::source_capture::CapturePhase::Failed => "failed",
                        crate::source_capture::CapturePhase::Saved => "saved",
                        crate::source_capture::CapturePhase::Recoverable => "recoverable",
                    };
                    serde_json::json!({
                        "request": capture.request,
                        "ordinal": capture.ordinal,
                        "session": capture.session,
                        "graph": capture.graph,
                        "node": capture.node,
                        "url": capture.url.as_str(),
                        "source": capture.source.as_ref(),
                        "keep": capture.keep,
                        "phase": phase,
                        "error": capture.error.as_deref(),
                        "stored": capture.stored.as_ref(),
                    })
                }).collect();
                let body = serde_json::to_string_pretty(&captures).map_err(|error| {
                    format!("record-source-captures '{name}': could not serialize: {error}")
                })?;
                let path = self.shared_out_dir.join(format!("{name}.source-captures.json"));
                std::fs::write(&path, body).map_err(|error| {
                    format!("record-source-captures '{name}': could not write {}: {error}", path.display())
                })?;
            },
            Step::RecordSavedSources(name) => {
                let mut readers = Vec::new();
                let mut nodes: Vec<_> = self.app.saved_readers.keys().copied().collect();
                nodes.sort();
                for node in nodes {
                    let reader = &self.app.saved_readers[&node];
                    let engine = self.app.content.facts(node).map(|facts| facts.engine.clone());
                    let node_live = matches!(
                        self.app.content.get(node),
                        Some(crate::content::NodeContent::Live)
                    );
                    let rendered_document = if node_live
                        && engine.as_deref() == Some(crate::saved_sources::ENGINE_ID)
                    {
                        self.content_sessions
                            .session_mut(&node)
                            .and_then(|session| {
                                session
                                    .as_any()
                                    .downcast_mut::<mere_document_lanes::SmolwebDocumentSession>()
                            })
                            .map(|session| session.document_mut().document().clone())
                    } else {
                        None
                    };
                    let phase = if reader.error.is_some() {
                        "Error"
                    } else if reader.request.is_some() || reader.content.is_none() {
                        "Pending"
                    } else if node_live
                        && engine.as_deref() == Some(crate::saved_sources::ENGINE_ID)
                        && rendered_document.is_some()
                    {
                        "Ready"
                    } else {
                        "EngineUnavailable"
                    };
                    readers.push(serde_json::json!({
                        "node": node,
                        "url": reader.url,
                        "target": reader.target,
                        "phase": phase,
                        "error": reader.error,
                        "content": reader.content,
                        "engine": engine,
                        "rendered_document": rendered_document,
                    }));
                }
                let mut retained_readers = Vec::new();
                let mut members: Vec<_> = self.app.content.live_nodes().filter(|node| self.app.content.facts(*node).is_some_and(|f| f.engine == crate::retained_clip::ENGINE_ID)).collect();
                members.sort();
                for node in members {
                    if let Some(session) = self.content_sessions.session_mut(&node).and_then(|session| session.as_any().downcast_mut::<mere_document_lanes::SmolwebDocumentSession>()) {
                        retained_readers.push(serde_json::json!({ "node": node, "engine": crate::retained_clip::ENGINE_ID, "rendered_document": session.document_mut().document().clone() }));
                    }
                }
                let value = serde_json::json!({
                    "session": self.app.session_id,
                    "generation": self.app.source_capture_generation,
                    "status": self.app.saved_sources.status,
                    "admissions": self.app.saved_source_admissions(),
                    "entries": self.app.saved_sources.entries,
                    "readers": readers,
                    "retained_admissions": self.app.research_clip_admissions(),
                    "retained_readers": retained_readers,
                });
                let body = serde_json::to_string_pretty(&value).map_err(|error| {
                    format!("record-saved-sources '{name}': could not serialize: {error}")
                })?;
                let path = self
                    .shared_out_dir
                    .join(format!("{name}.saved-sources.json"));
                std::fs::write(&path, body).map_err(|error| {
                    format!(
                        "record-saved-sources '{name}': could not write {}: {error}",
                        path.display()
                    )
                })?;
            },
            Step::TouchFile(path) => {
                let p = std::path::Path::new(path.as_str());
                std::fs::write(p, "touched\n")
                    .map_err(|error| format!("touch '{path}': could not write: {error}"))?;
            },
        }
        self.request_redraw();
        Ok(())
    }
}
