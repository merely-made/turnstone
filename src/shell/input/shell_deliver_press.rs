// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    /// Route a pointer press to the surface under `(x, y)` and capture it until
    /// release (rung 5 slice B). A press on content focuses it and delivers the
    /// click: a link resolves to a durable navigation and goes through
    /// `Action::OpenAddress`, growing the graph; a press on the canvas begins a
    /// canvas gesture. Shared by winit and the scenario runner.
    pub(in super::super) fn deliver_press(&mut self, x: f32, y: f32, button: MouseButton) {
        self.surface_pointer_buttons.0 |= surface_button_mask(button).0;
        self.window_mouse_input(
            0,
            None,
            (x, y),
            inker::PointerPhase::Down,
            surface_mouse_button(button),
        );
        if self.contributed_pointer.is_some() {
            return;
        }
        let chrome_intent = self
            .chrome
            .click(0, x, y, self.width.max(1), self.height.max(1))
            .into_iter()
            .next();
        tracing::debug!(
            x,
            y,
            ?button,
            ?chrome_intent,
            omnibar = self.app.omnibar.open,
            "physical pane press admission"
        );
        if dispatch_primary_chrome_press(self, chrome_intent) {
            return;
        }
        // A held user-agent callback is modal with respect to the page that
        // requested it. A click outside the retained decision card is consumed
        // rather than leaking through to content behind the prompt.
        if reject_primary_modal_press(self) {
            return;
        }
        // A press while the omnibar is open dismisses it and is swallowed, so
        // the surface beneath never also reacts to the same press.
        if reject_primary_omnibar_press(self) {
            return;
        }
        if button != MouseButton::Left {
            self.leave_frame_focus();
        }
        if button == MouseButton::Left
            && self.outer_press(0, (self.width.max(1), self.height.max(1)), x, y)
        {
            self.pointer_capture = None;
            self.request_redraw();
            return;
        }
        let plan = self.surface_plan();
        let hit = crate::surface::hit_test(&plan, self.app.focus, x, y);
        tracing::debug!(x, y, ?hit, "physical surface press selected");
        // Right-click is the context menu the palette registry names: the
        // command palette (the `>` actions lane). On the canvas the press goes
        // to the canvas, so a right-drag selects (Scenograph editor plan
        // SE26); the menu opens on the release the canvas reports as a click,
        // in `deliver_release`. Elsewhere the menu opens on the press, as
        // before.
        if dispatch_primary_context_press(self, button, hit) {
            return;
        }
        if button == MouseButton::Left
            && self.pointer_capture.is_none()
            && super::super::window_input::captured_mouse(&self.content_sessions.pool).is_none()
            && let Some((pane, divider, origin)) =
                workbench_resize_target(&self.app, &plan, hit, x, y)
        {
            self.app.active_pane = Some(pane);
            self.pointer_capture = Some(crate::surface::SurfaceKind::Pane(pane));
            self.wb_divider_drag = Some((divider, origin));
            self.request_redraw();
            return;
        }
        if let Some(hit) = dispatch_primary_press(self, hit, x, y, button) {
            match hit.kind {
                // Content is fully dispatched by the shared native caller.
                crate::surface::SurfaceKind::Content(_) => unreachable!(),
                // A press on a pane makes it the active pane (the anchor for
                // close/maximize/divider). A Trail pane also routes the click to
                // its row (slice D): a navigable row lowers Action::OpenAddress
                // through the same spine as a keypress. Other panes are still
                // placeholders (slice C), so the press is otherwise swallowed.
                crate::surface::SurfaceKind::Pane(id) => {
                    self.app.active_pane = Some(id);
                    // A5 floats share this ordinary pane surface path. Raising
                    // happens before the pane's retained DOM receives the
                    // click, so the next frame's paint and hit order agree.
                    self.raise_shared_float(id);
                    if button == MouseButton::Left {
                        if let Some(pane) = self.renderers.contributed.get_mut(id) {
                            self.app.focus = crate::surface::FocusTarget::Pane(id);
                            pane.focus_at(hit.local.0, hit.local.1);
                            self.contributed_pointer = plan
                                .iter()
                                .find(|surface| surface.id == hit.id)
                                .copied()
                                .map(|surface| ContributedPointer {
                                    surface,
                                    generation: pane.generation(),
                                    window: 0,
                                });
                            pane.mouse_press(hit.local.0, hit.local.1);
                            self.sync_ime_allowed();
                            self.request_redraw();
                            return;
                        }
                        if self.renderers.inline_mount(id).is_some() {
                            self.app.focus = crate::surface::FocusTarget::Pane(id);
                            self.input_window = 0;
                        }
                        match self.pane_content(id) {
                            Some(PaneContent::Trail) => {
                                // The same cambium round trip as the Roster.
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                let actions = match (dims, self.renderers.trail.get_mut(&id)) {
                                    (Some((rw, rh)), Some(pane)) => {
                                        pane.click(hit.local.0, hit.local.1, rw, rh)
                                    },
                                    _ => Vec::new(),
                                };
                                for action in actions {
                                    match action {
                                        crate::trail_pane::TrailPaneAction::Navigate(url) => {
                                            self.act(Action::OpenAddress(url))
                                        },
                                        crate::trail_pane::TrailPaneAction::RecoverSession(id) => {
                                            // A Removed-sessions row: restore the
                                            // trashed session and switch (O3).
                                            if let Ok(id) = id.parse::<uuid::Uuid>() {
                                                self.act(Action::RecoverSession(
                                                    crate::panes::SessionId::from_uuid(id),
                                                ));
                                            }
                                        },
                                        crate::trail_pane::TrailPaneAction::Recover(id) => {
                                            // The Removed row carries the staged
                                            // node's ORIGINAL uuid; recovery
                                            // restores that identity.
                                            match id.parse::<uuid::Uuid>() {
                                                Ok(id) => self.act(Action::RecoverDeletedNode(id)),
                                                Err(_) => self.app.note(
                                                    crate::observe::AppEvent::InteractionMissed {
                                                        what: "recover",
                                                        target: id,
                                                    },
                                                ),
                                            }
                                        },
                                    }
                                }
                            },
                            Some(PaneContent::Roster) => {
                                // Route into the cambium grid: hit-test its DOM
                                // at the pane's size and dispatch, then lower
                                // whatever the view emitted through the spine —
                                // the same path a keypress takes. This is the
                                // general cambium pane-event round trip.
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                let actions = match (dims, self.renderers.roster.get_mut(&id)) {
                                    (Some((rw, rh)), Some(grid)) => {
                                        let actions = grid.click(hit.local.0, hit.local.1, rw, rh);
                                        // The strip emits no action — switching a
                                        // tab is a state change in the widget's
                                        // own state. Mirror it out so the rest of
                                        // the host can see which tab is showing.
                                        self.app.roster_tab = grid.selected_tab().0;
                                        actions
                                    },
                                    _ => Vec::new(),
                                };
                                for action in actions {
                                    match action {
                                        crate::cambium_pane::RosterAction::Binding(intent) => {
                                            self.act(Action::SetPaneBinding(intent));
                                        },
                                        crate::cambium_pane::RosterAction::Navigate(url) => {
                                            self.act(Action::OpenAddress(url))
                                        },
                                    }
                                }
                            },
                            Some(PaneContent::Gloss(_)) => {
                                // Same hit-test round trip; the outcome arrives
                                // as drained intents (the swatch mutates state
                                // rather than bubbling), lowered here.
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                let intents = match (dims, self.renderers.gloss.get_mut(&id)) {
                                    (Some((rw, rh)), Some(pane)) => {
                                        pane.click(hit.local.0, hit.local.1, rw, rh)
                                    },
                                    _ => Vec::new(),
                                };
                                for intent in intents {
                                    match intent {
                                        crate::swatch_pane::SwatchIntent::Activate(
                                            crate::swatch_pane::SwatchActivate::Open(url),
                                        ) => self.act(Action::OpenAddress(url)),
                                        crate::swatch_pane::SwatchIntent::Activate(
                                            crate::swatch_pane::SwatchActivate::Switch(id),
                                        ) => self.act(Action::SelectScopeMere(id)),
                                        // A composed Removed row: recover the
                                        // node under its ORIGINAL id.
                                        crate::swatch_pane::SwatchIntent::Activate(
                                            crate::swatch_pane::SwatchActivate::Recover(id),
                                        ) => self.act(Action::RecoverDeletedNode(id)),
                                        crate::swatch_pane::SwatchIntent::Expand => {
                                            self.app.focus = crate::surface::FocusTarget::Graph(
                                                self.app.default_graph_pane(),
                                            );
                                        },
                                    }
                                }
                            },
                            Some(PaneContent::Inspector) => {
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                let intents = match (dims, self.renderers.inspector.get_mut(&id)) {
                                    (Some((rw, rh)), Some(pane)) => {
                                        pane.click(hit.local.0, hit.local.1, rw, rh)
                                    },
                                    _ => Vec::new(),
                                };
                                for intent in intents {
                                    match intent {
                                        crate::inspector_pane::InspectorIntent::ClipToKnot { offer } => {
                                            self.clip_document_to_knot(offer)
                                        },
                                        crate::inspector_pane::InspectorIntent::Binding(intent) => self.act(Action::SetPaneBinding(intent)),
                        crate::inspector_pane::InspectorIntent::NewResearchNote => self.act(Action::NewResearchNote),
                                        crate::inspector_pane::InspectorIntent::OpenResearchNote => self.act(Action::OpenResearchNote),
                                        crate::inspector_pane::InspectorIntent::ChooseResearchNote { choice } => self.act(Action::ChooseResearchNote { choice }),
                                        crate::inspector_pane::InspectorIntent::CaptureSourceDocument { target } => self.act(Action::CaptureSourceDocument { target }),
                                        crate::inspector_pane::InspectorIntent::SetViewer {
                                            member,
                                            viewer,
                                        } => self.act(Action::SetViewerOverride { member, viewer }),
                                    }
                                }
                            },
                            Some(PaneContent::Registered(kind))
                                if kind.as_str() == crate::panes::kind::TRANSCRIPT =>
                            {
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                let actions = match (dims, self.renderers.transcript.get_mut(&id)) {
                                    (Some((rw, rh)), Some(pane)) => {
                                        pane.click(hit.local.0, hit.local.1, rw, rh)
                                    },
                                    _ => Vec::new(),
                                };
                                for action in actions {
                                    match action {
                                        crate::transcript_pane::TranscriptPaneAction::Repeat(
                                            entry,
                                        ) => {
                                            // The lane already lowers this
                                            // Action; the pane only names the
                                            // entry to repeat.
                                            self.act(Action::RepeatShellEntry(entry));
                                        },
                                    }
                                }
                            },
                            Some(PaneContent::Registered(kind))
                                if kind.as_str() == crate::panes::kind::ARRANGE =>
                            {
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                let intents = match (dims, self.renderers.arrange.get_mut(&id)) {
                                    (Some((rw, rh)), Some(pane)) => {
                                        pane.click(hit.local.0, hit.local.1, rw, rh)
                                    },
                                    _ => Vec::new(),
                                };
                                for intent in intents {
                                    if let Some(action) = intent.into_action() {
                                        self.act(action);
                                    }
                                }
                            },
                            Some(PaneContent::Registered(kind))
                                if kind.as_str() == crate::panes::kind::SETTINGS =>
                            {
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                if let (Some((rw, rh)), Some(pane)) =
                                    (dims, self.renderers.settings.get_mut(&id))
                                {
                                    self.app.focus = crate::surface::FocusTarget::Pane(id);
                                    pane.click(hit.local.0, hit.local.1, rw, rh);
                                }
                            },
                            Some(PaneContent::Registered(kind))
                                if kind.as_str() == crate::panes::kind::PUBLISHING =>
                            {
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                if let (Some((rw, rh)), Some(pane)) =
                                    (dims, self.renderers.publish.get_mut(&id))
                                {
                                    pane.click(hit.local.0, hit.local.1, rw, rh);
                                }
                            },
                            Some(PaneContent::Registered(kind))
                                if kind.as_str() == crate::panes::kind::SHARED_KNOT =>
                            {
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                if let (Some((rw, rh)), Some(pane)) =
                                    (dims, self.renderers.shared_knot.get_mut(&id))
                                {
                                    pane.click(hit.local.0, hit.local.1, rw, rh);
                                }
                            },
                            Some(PaneContent::Registered(kind))
                                if kind.as_str() == crate::panes::kind::DEVICE_RECEIPTS =>
                            {
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                if let (Some((rw, rh)), Some(pane)) =
                                    (dims, self.renderers.device_receipts.get_mut(&id))
                                {
                                    pane.click(hit.local.0, hit.local.1, rw, rh);
                                }
                            },
                            Some(PaneContent::Overmap(_)) => {
                                // A session-node click adopts that session:
                                // navigating to a container IS the switch
                                // (overmap v0), through the ordinary spine.
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect.w.round().max(1.0) as u32,
                                        s.rect.h.round().max(1.0) as u32,
                                    )
                                });
                                let intents = match (dims, self.renderers.overmap.get_mut(&id)) {
                                    (Some((rw, rh)), Some(pane)) => {
                                        pane.click(hit.local.0, hit.local.1, rw, rh)
                                    },
                                    _ => Vec::new(),
                                };
                                for intent in intents {
                                    match intent {
                                        crate::swatch_pane::SwatchIntent::Activate(
                                            crate::swatch_pane::SwatchActivate::Open(url),
                                        ) => self.act(Action::OpenAddress(url)),
                                        crate::swatch_pane::SwatchIntent::Activate(
                                            crate::swatch_pane::SwatchActivate::Switch(id),
                                        ) => self.act(Action::SelectScopeMere(id)),
                                        // A composed Removed row: recover the
                                        // node under its ORIGINAL id.
                                        crate::swatch_pane::SwatchIntent::Activate(
                                            crate::swatch_pane::SwatchActivate::Recover(id),
                                        ) => self.act(Action::RecoverDeletedNode(id)),
                                        crate::swatch_pane::SwatchIntent::Expand => {
                                            self.app.focus = crate::surface::FocusTarget::Graph(
                                                self.app.default_graph_pane(),
                                            );
                                        },
                                    }
                                }
                            },
                            Some(PaneContent::Workbench) => {
                                // A press here begins a gesture, resolved on
                                // RELEASE (a tab click activates; a tab drag
                                // onto another cell stacks; a seam drag
                                // re-weights) — so record what was pressed and
                                // decide in deliver_release / deliver_move.
                                let dims = plan.iter().find(|s| s.id == hit.id).map(|s| {
                                    (
                                        s.rect,
                                        (
                                            s.rect.w.round().max(1.0) as u32,
                                            s.rect.h.round().max(1.0) as u32,
                                        ),
                                    )
                                });
                                if let (Some((rect, (rw, rh))), Some(pane)) =
                                    (dims, self.renderers.workbench.get_mut(&id))
                                {
                                    let (lx, ly) = hit.local;
                                    if let Some(div) = pane.tiling().divider_at(lx, ly).cloned() {
                                        self.wb_divider_drag = Some((div, (rect.x, rect.y)));
                                    } else if let Some(member) = pane.tab_at(lx, ly, rw, rh) {
                                        self.app.focus = crate::surface::FocusTarget::Pane(id);
                                        self.app.publish_member_context(id, Some(member));
                                        self.wb_tab_drag = Some((id, member));
                                    } else {
                                        // Controls are ordinary pane input. Only tabs
                                        // and dividers enter the tiling gesture path.
                                        let member = pane
                                            .tiling()
                                            .cell_at(lx, ly)
                                            .and_then(|cell| cell.active_member());
                                        if !pane.is_close_at(lx, ly) {
                                            self.app.focus = crate::surface::FocusTarget::Pane(id);
                                            self.app.active_pane = Some(id);
                                            self.app.publish_member_context(id, member);
                                        }
                                        let actions = self.pane_click_actions(
                                            id,
                                            &PaneContent::Workbench,
                                            (lx, ly),
                                            (rw, rh),
                                        );
                                        for action in actions {
                                            self.act(action);
                                        }
                                    }
                                }
                            },
                            _ => {},
                        }
                    }
                    self.sync_ime_allowed();
                    self.request_redraw();
                    return;
                },
                crate::surface::SurfaceKind::Divider(index) => {
                    let area = Rect::full(self.width.max(1), self.height.max(1));
                    let tiling =
                        crate::pane::place_panes(&self.app.frisket, area, self.app.maximized);
                    self.divider_drag = tiling.dividers.into_iter().find(|d| d.index == index);
                    self.request_redraw();
                    return;
                },
                // A graph pane owns the view gesture. The local point is
                // essential for side-by-side graph panes.
                crate::surface::SurfaceKind::Graph(pane) => {
                    self.raise_shared_float(pane);
                    self.app.focus = crate::surface::FocusTarget::Graph(pane);
                    if let Some(button) = pointer_button(button)
                        && self
                            .app
                            .graph_pane_pointer_down(pane, button, hit.local.0, hit.local.1)
                    {
                        self.request_redraw();
                    }
                },
                crate::surface::SurfaceKind::Chrome
                | crate::surface::SurfaceKind::Frame
                | crate::surface::SurfaceKind::FloatFrame(_) => {},
            }
        }
    }
}
