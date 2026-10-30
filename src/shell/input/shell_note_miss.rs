// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    /// One aim that resolved to nothing, and why.
    ///
    /// The open omnibar is the cause worth naming: it takes the press before
    /// any pane sees it, so a scenario reads "no row matched" and looks for a
    /// missing row that is right there behind the chrome.
    pub(super) fn note_miss(&mut self, what: &'static str, target: &str, missed: &str) {
        let swallowed = self.app.omnibar.open;
        self.app.note(crate::observe::AppEvent::InteractionMissed {
            what,
            target: target.to_string(),
        });
        if swallowed {
            tracing::warn!(%target, "{what}: the open omnibar swallowed the press");
        } else {
            tracing::warn!(%target, "{what}: {missed}");
        }
    }

    pub(in super::super) fn click_pane_row(&mut self, substr: &str) {
        // Both list panes resolve through the shared driver's `click`: a Trail
        // `list-row` or a grid `roster-cell` whose text contains `substr`, over
        // all surfaces at once (no per-pane dispatch). Short-circuit `||` means a
        // hit presses once; only a total miss is attributable.
        let hit = self.click(&taproot::Selector::class("radio")
            .with_attr("data-engine-id", substr)
            .on_surface("inspector"))
            || self.click(&taproot::Selector::class("roster-cell").containing(substr))
            || self.click(&taproot::Selector::class("list-row").containing(substr))
            // A settings option is a row for receipt purposes (the Inspector
            // pane's radio options).
            || self.click(&taproot::Selector::class("radio").containing(substr))
            || self.click(&taproot::Selector::class("setting-apply").containing(substr))
            // A composed list section's row (the gloss-composite): the same
            // verb addresses it, wherever the section was composed.
            || self.click(&taproot::Selector::class("section-row").containing(substr));
        if !hit {
            self.note_miss("click-row", substr, "no list-pane row matched");
        }
    }

    /// Click the Roster's tab labelled `label` (the scenario's `click-tab`),
    /// through the shared driver: a `.tab` element whose text is `label`. The
    /// strip's geometry is the layout's to know; the host names the target and
    /// the resolver finds it — the same substrate every genet app shares.
    pub(in super::super) fn click_pane_tab(&mut self, label: &str) {
        if !self.click(&taproot::Selector::class("tab").containing(label)) {
            self.note_miss("click-tab", label, "no Roster tab matched");
        }
    }

    /// Click the Gloss minimap's node matching `substr` (the scenario's
    /// `click-node`), through the shared driver. The node buttons carry their url
    /// as `data-key`, so the driver selects on it — unique where the display
    /// label (two "Example Domain" pages) is not.
    pub(in super::super) fn click_pane_node(&mut self, substr: &str) {
        let sel =
            taproot::Selector::class("graph-canvas-swatch-node").with_attr("data-key", substr);
        if !self.click(&sel) {
            self.note_miss("click-node", substr, "no pane node matched");
        }
    }

    /// Route a wheel event to the surface under `(x, y)` (rung 5 slice B). The
    /// page scrolls when the pointer is on it, the canvas pans when it is not.
    /// Ephemeral, so it drives the session's semantic method directly (the
    /// gesture law), never an Action. Shared by winit and the scenario runner.
    pub(in super::super) fn deliver_wheel(&mut self, x: f32, y: f32, dx: f32, dy: f32) {
        if self.app.user_agent_decision.is_open() {
            return;
        }
        let plan = self.surface_plan();
        if let Some(hit) = crate::surface::hit_test(&plan, self.app.focus, x, y)
            && let crate::surface::SurfaceKind::Content(node) = hit.kind
        {
            self.content_wheel(0, hit.id, node, hit.local, (dx, dy));
            return;
        }
        if let Some(crate::surface::HitResult {
            kind: crate::surface::SurfaceKind::Graph(pane),
            ..
        }) = crate::surface::hit_test(&plan, self.app.focus, x, y)
            && self.app.graph_pane_wheel(pane, dx, dy)
        {
            self.request_redraw();
            return;
        }
        // A list pane under the pointer scrolls itself. Without this branch a
        // pane taller than its tile simply hides its overflow: the content is
        // laid out and clipped, and nothing can reach it.
        if let Some(crate::surface::HitResult {
            kind: crate::surface::SurfaceKind::Pane(pane),
            local,
            ..
        }) = crate::surface::hit_test(&plan, self.app.focus, x, y)
        {
            if let Some(surface) = self.renderers.contributed.get_mut(pane) {
                if surface.wheel(dx, dy, local.0, local.1)
                    == crate::contributed_surface::SurfaceRequest::Redraw
                {
                    self.request_redraw();
                }
            } else if self.renderers.scroll_pane(pane, dx, dy) {
                self.request_redraw();
            }
        }
    }
}
