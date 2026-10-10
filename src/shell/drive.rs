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
/// attempts, sleeping briefly between attempts. `wait-row`/`wait-status`/
/// `wait-file` are "sticky" over the SHARED taproot scenario loop's own
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
            .node_sessions_mut()
            .filter_map(|(node, session)| (!session.settled()).then_some(node))
            .collect::<Vec<_>>();
        unsettled_sessions.sort_unstable();
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
        f(&surfaces)
    }

    fn selector_target(&self, selector: &taproot::Selector) -> taproot::SelectorTarget {
        match selector.surface.as_deref() {
            None => taproot::SelectorTarget::Unsupported,
            Some("workbench") => {
                let plan = self.surface_plan();
                let mut selected = None;
                for surface in &plan {
                    let crate::surface::SurfaceKind::Pane(id) = surface.kind else {
                        continue;
                    };
                    if self.pane_content(id) != Some(PaneContent::Workbench) {
                        continue;
                    }
                    let Some(pane) = self.renderers.workbench.get(&id) else {
                        continue;
                    };
                    let point = match pane.selector_point(selector) {
                        Ok(Some((x, y))) => (surface.rect.x + x, surface.rect.y + y),
                        Ok(None) => continue,
                        Err(_) => return taproot::SelectorTarget::Miss,
                    };
                    if crate::surface::hit_test(&plan, self.app.focus, point.0, point.1)
                        .is_none_or(|hit| hit.id != surface.id)
                        || pane
                            .tab_at(
                                point.0 - surface.rect.x,
                                point.1 - surface.rect.y,
                                surface.rect.w as u32,
                                surface.rect.h as u32,
                            )
                            .is_none()
                        || selected.is_some()
                    {
                        return taproot::SelectorTarget::Miss;
                    }
                    selected = Some(taproot::Hit {
                        surface: "workbench",
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
            // Other ordinary pane scopes retain the shared DOM probe fallback.
            Some(_) => taproot::SelectorTarget::Unsupported,
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
        let kept = snap.focused.as_ref().is_some_and(|node| node.kept);
        let mut out = taproot::ProbeSnapshot::default()
            .with_field("focus", snap.focus)
            .with_field(
                "host-surface-producers",
                self.content_sessions.surface_count().to_string(),
            )
            .with_field("node-count", snap.node_count.to_string())
            .with_field("roster-tab", snap.roster_tab)
            // The panes and surfaces as joined tags, so a generic scenario can
            // `assert snap panes ~ roster` without an app-specific verb. This is
            // minimal-shared-and-grow: the app adds the fields its scenarios name.
            .with_field("panes", snap.panes.join(","))
            .with_field("surfaces", snap.surfaces.join(","))
            .with_field("floats", snap.floating_panes.join(","))
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
            .with_field("commands-removed", self.app.command_choices.removed.join(","));
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
                .with_field("host-servo-profile", self.servo_factory.as_ref()
                    .map_or("", |factory| factory.profile_name()))
                .with_field("host-servo-a11y-focus", self.servo_factory.as_ref()
                    .map_or("", |factory| factory.a11y_focus_policy()));
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
    /// the three kinds of work a scenario must not race is outstanding:
    ///
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
                .sessions_mut()
                .any(|session| !session.settled()),
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
            Step::Omnibar { command } => self.act(Action::OmnibarOpen { command: *command }),
            Step::Type(text) => {
                if self.app.user_agent_decision.is_open() {
                    if self.app.user_agent_decision.accepts_text() {
                        self.act(Action::InsertAuthentication(text.clone()));
                    } else {
                        return Err("type: the active decision has no text field".into());
                    }
                } else if self.app.document_find.open {
                    self.act(Action::InsertDocumentFind(text.clone()));
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
                    self.act(Action::InsertDocumentFind(text.clone()));
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
            Step::Key(key) => {
                // Route through the SAME key seam winit uses, so `key` drives
                // whatever holds focus (the omnibar, a focused page, the
                // canvas) exactly as a real press would — one description, two
                // runners, for keys as well as pointers. The former direct
                // EditKey->omnibar-Action map only ever reached the omnibar.
                let (winit_key, ctrl) = match key {
                    EditKey::Enter => (WinitKey::Named(WinitNamedKey::Enter), false),
                    EditKey::Escape => (WinitKey::Named(WinitNamedKey::Escape), false),
                    EditKey::Tab => (WinitKey::Named(WinitNamedKey::Tab), false),
                    EditKey::Backspace => (WinitKey::Named(WinitNamedKey::Backspace), false),
                    EditKey::Delete => (WinitKey::Named(WinitNamedKey::Delete), false),
                    EditKey::Up => (WinitKey::Named(WinitNamedKey::ArrowUp), false),
                    EditKey::Down => (WinitKey::Named(WinitNamedKey::ArrowDown), false),
                    EditKey::Left => (WinitKey::Named(WinitNamedKey::ArrowLeft), false),
                    EditKey::Right => (WinitKey::Named(WinitNamedKey::ArrowRight), false),
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
                self.ctrl |= ctrl;
                // Match the text accompanying a native winit key press. In
                // Chromium, Enter's carriage-return char event activates the
                // focused button after rawKeyDown.
                let text = match winit_key {
                    WinitKey::Named(WinitNamedKey::Enter) => Some("\r"),
                    WinitKey::Named(WinitNamedKey::Space) => Some(" "),
                    _ => None,
                };
                if self.deliver_surface_key(&winit_key, true, text) {
                    self.deliver_surface_key(&winit_key, false, None);
                } else {
                    self.on_key(&winit_key);
                }
                self.ctrl = previous_ctrl;
            },
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
                    diagnostic["servo_focus_policy"] = serde_json::json!(factory.a11y_focus_policy());
                }
                let bytes = serde_json::to_vec_pretty(&diagnostic)
                .map_err(|error| error.to_string())?;
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
                let (tree, _, _) = self.projected_a11y_tree();
                let lines = crate::a11y::tree_lines(&tree);
                if !lines.iter().any(|l| l.contains(substr)) {
                    return Err(format!(
                        "assert a11y '{substr}': {} lines, none match (first 12: {:?})",
                        lines.len(),
                        lines.iter().take(12).collect::<Vec<_>>()
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
                let expected = (
                    appearance.rect.w.round().max(1.0) as u32,
                    appearance.rect.h.round().max(1.0) as u32,
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
                let ok = self.wait_frames(*frames, |shell| {
                    crate::observe::snapshot(&shell.app)
                        .place_status
                        .iter()
                        .any(|line| line.contains(substr.as_str()))
                });
                if !ok {
                    let status = crate::observe::snapshot(&self.app).place_status;
                    return Err(format!(
                        "wait-status '{substr}' after {frames} frames: place status is {status:?}"
                    ));
                }
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
            Step::WaitFile(frames, path) => {
                let p = std::path::Path::new(path.as_str());
                let ok = self.wait_frames(*frames, |_| {
                    std::fs::metadata(p).is_ok_and(|meta| meta.len() > 0)
                });
                if !ok {
                    return Err(format!(
                        "wait-file '{path}' after {frames} frames: still missing or empty"
                    ));
                }
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
