// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn scenario_assert_focused(
        &mut self,
        step: &crate::scenario::Step,
    ) -> Result<(), String> {
        use super::super::drive::{cmp_f32, cmp_usize};
        use crate::action::CaretMove;
        use crate::scenario::{CmpOp, EditKey, Step};
        match step {
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
                let action = rows
                    .iter()
                    .find(|(candidate, _)| candidate == label)
                    .or_else(|| {
                        rows.iter()
                            .find(|(candidate, _)| candidate.starts_with(label))
                    })
                    .map(|(_, action)| action.clone())
                    .ok_or_else(|| format!("queue-act: no palette action labelled '{label}'"))?;
                self.scenario_queued_action = Some(action);
            },
            Step::DeliverAct => {
                let action = self
                    .scenario_queued_action
                    .take()
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
            _ => unreachable!("dispatch group disagrees with exhaustive routing"),
        }
        Ok(())
    }
}
