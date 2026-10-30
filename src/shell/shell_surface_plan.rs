// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    /// The current surface plan, from app truth plus the window size. The one
    /// place render and input agree on which surfaces exist and where, so a
    /// pointer always hits exactly what the last frame drew. The base layer is
    /// the frisket pane tree (rung 5 slice C): the Orrery leaf is the canvas,
    /// every other leaf a pane. Content insets over the canvas; chrome sits on
    /// top.
    pub(super) fn surface_plan(&self) -> Vec<crate::surface::Surface> {
        let frame = self
            .app
            .blueprint_space(crate::action::SpaceRef::Primary)
            .is_none()
            .then(|| {
                self.outer_rects(
                    0,
                    &self.app.frisket,
                    self.app.maximized,
                    (self.width.max(1), self.height.max(1)),
                )
            });
        primary_content_plan_with_rects(
            &self.app,
            &self.content_sessions,
            self.width,
            self.height,
            frame.as_deref(),
            &self.outer_float_rects(0),
        )
    }

    /// A pane's `PaneContent`, looked up from the frisket tree by id.
    pub(super) fn pane_content(&self, id: crate::panes::PaneId) -> Option<PaneContent> {
        self.app.pane_content(id).cloned()
    }

    /// A pane's display label, looked up from the frisket tree by id.
    pub(super) fn pane_label(&self, id: crate::panes::PaneId) -> String {
        self.pane_content(id)
            .map(|content| pane_display_label(&content))
            .unwrap_or_default()
    }

    /// Click the list-pane (Trail/Roster) row whose text contains `substr`
    /// (scenario `click-row`). The shell owns the pane rects and rows, so it
    /// resolves the row's window position and delivers a real click through the
    /// shared pointer path — a receipt names a row by text, not pixels.

    /// A pane click's resulting Actions, by kind — the cambium round trip
    /// (hit-test the runner's DOM, dispatch, convert what bubbles) packaged
    /// for any window. Lens windows drive this; the primary press arm carries
    /// its own copy of these round trips today (collapsing it here is a
    /// follow-on simplification). Side-mirrors happen here (roster_tab, the
    /// gloss Expand focus, Trail's not-yet-wired Recover note); durable
    /// intents come back as Actions for the caller to lower.
    pub(super) fn pane_click_actions(
        &mut self,
        pane_id: crate::panes::PaneId,
        content: &PaneContent,
        local: (f32, f32),
        dims: (u32, u32),
    ) -> Vec<Action> {
        self.pane_input_actions(pane_id, content, local, dims, None)
    }

    pub(super) fn pane_input_actions(
        &mut self,
        pane_id: crate::panes::PaneId,
        content: &PaneContent,
        local: (f32, f32),
        dims: (u32, u32),
        key: Option<cambium::KeyEvent>,
    ) -> Vec<Action> {
        let (lx, ly) = local;
        let (rw, rh) = dims;
        let mut out = Vec::new();
        macro_rules! input {
            ($pane:expr) => {
                match key.clone() {
                    Some(event) => $pane.key_inline(event),
                    None => $pane.click(lx, ly, rw, rh),
                }
            };
        }
        match content {
            PaneContent::Trail => {
                if let Some(pane) = self.renderers.trail.get_mut(&pane_id) {
                    for action in input!(pane) {
                        match action {
                            crate::trail_pane::TrailPaneAction::Navigate(url) => {
                                out.push(Action::OpenAddress(url))
                            },
                            crate::trail_pane::TrailPaneAction::Recover(id) => {
                                match id.parse::<uuid::Uuid>() {
                                    Ok(id) => out.push(Action::RecoverDeletedNode(id)),
                                    Err(_) => {
                                        self.app.note(crate::observe::AppEvent::InteractionMissed {
                                            what: "recover",
                                            target: id.clone(),
                                        })
                                    },
                                }
                            },
                            crate::trail_pane::TrailPaneAction::RecoverSession(id) => {
                                if let Ok(id) = id.parse::<uuid::Uuid>() {
                                    out.push(Action::RecoverSession(
                                        crate::panes::SessionId::from_uuid(id),
                                    ));
                                }
                            },
                        }
                    }
                }
            },
            PaneContent::Roster => {
                if let Some(grid) = self.renderers.roster.get_mut(&pane_id) {
                    let actions = input!(grid);
                    self.app.roster_tab = grid.selected_tab().0;
                    for action in actions {
                        match action {
                            crate::cambium_pane::RosterAction::Binding(intent) => {
                                out.push(Action::SetPaneBinding(intent));
                            },
                            crate::cambium_pane::RosterAction::Navigate(url) => {
                                out.push(Action::OpenAddress(url))
                            },
                        }
                    }
                }
            },
            PaneContent::Gloss(_) | PaneContent::Overmap(_) => {
                let panes = if matches!(content, PaneContent::Gloss(_)) {
                    &mut self.renderers.gloss
                } else {
                    &mut self.renderers.overmap
                };
                if let Some(pane) = panes.get_mut(&pane_id) {
                    for intent in input!(pane) {
                        match intent {
                            crate::swatch_pane::SwatchIntent::Activate(
                                crate::swatch_pane::SwatchActivate::Open(url),
                            ) => out.push(Action::OpenAddress(url)),
                            crate::swatch_pane::SwatchIntent::Activate(
                                crate::swatch_pane::SwatchActivate::Switch(id),
                            ) => out.push(Action::SelectScopeMere(id)),
                            // A composed Removed row: recover by ORIGINAL id.
                            crate::swatch_pane::SwatchIntent::Activate(
                                crate::swatch_pane::SwatchActivate::Recover(id),
                            ) => out.push(Action::RecoverDeletedNode(id)),
                            crate::swatch_pane::SwatchIntent::Expand => {
                                self.app.focus = crate::surface::FocusTarget::Graph(
                                    self.app.default_graph_pane(),
                                );
                            },
                        }
                    }
                }
            },
            PaneContent::Registered(kind) if kind.as_str() == crate::panes::kind::ARRANGE => {
                if let Some(pane) = self.renderers.arrange.get_mut(&pane_id) {
                    for intent in input!(pane) {
                        if let Some(action) = intent.into_action() {
                            out.push(action);
                        }
                    }
                }
            },
            PaneContent::Registered(kind) if kind.as_str() == crate::panes::kind::TRANSCRIPT => {
                if let Some(pane) = self.renderers.transcript.get_mut(&pane_id) {
                    for action in input!(pane) {
                        let crate::transcript_pane::TranscriptPaneAction::Repeat(entry) = action;
                        out.push(Action::RepeatShellEntry(entry));
                    }
                }
            },
            PaneContent::Workbench => {
                if let Some(pane) = self.renderers.workbench.get_mut(&pane_id) {
                    out.extend(
                        input!(pane)
                            .into_iter()
                            .map(|event| Action::WorkbenchActivate(event.0)),
                    );
                    out.extend(pane.take_read_actions());
                }
            },
            PaneContent::Registered(kind) if kind.as_str() == crate::panes::kind::SETTINGS => {
                if let Some(pane) = self.renderers.settings.get_mut(&pane_id) {
                    input!(pane);
                }
            },
            PaneContent::Registered(kind) if kind.as_str() == crate::panes::kind::PUBLISHING => {
                if let Some(pane) = self.renderers.publish.get_mut(&pane_id) {
                    input!(pane);
                }
            },
            PaneContent::Registered(kind) if kind.as_str() == crate::panes::kind::SHARED_KNOT => {
                if let Some(pane) = self.renderers.shared_knot.get_mut(&pane_id) {
                    input!(pane);
                }
            },
            PaneContent::Registered(kind)
                if kind.as_str() == crate::panes::kind::DEVICE_RECEIPTS =>
            {
                if let Some(pane) = self.renderers.device_receipts.get_mut(&pane_id) {
                    input!(pane);
                }
            },
            PaneContent::Registered(kind)
                if kind.as_str() == crate::panes::kind::FROZEN_PROJECTION =>
            {
                if let Some(pane) = self.renderers.frozen_projection.get_mut(&pane_id) {
                    input!(pane);
                }
            },
            PaneContent::Inspector => {
                let intents = self
                    .renderers
                    .inspector
                    .get_mut(&pane_id)
                    .map(|pane| input!(pane))
                    .unwrap_or_default();
                for intent in intents {
                    match intent {
                        crate::inspector_pane::InspectorIntent::ClipToKnot { offer } => {
                            self.clip_document_to_knot(offer)
                        },
                        crate::inspector_pane::InspectorIntent::Binding(intent) => {
                            out.push(Action::SetPaneBinding(intent))
                        },
                        crate::inspector_pane::InspectorIntent::NewResearchNote => {
                            out.push(Action::NewResearchNote)
                        },
                        crate::inspector_pane::InspectorIntent::OpenResearchNote => {
                            out.push(Action::OpenResearchNote)
                        },
                        crate::inspector_pane::InspectorIntent::ChooseResearchNote { choice } => {
                            out.push(Action::ChooseResearchNote { choice })
                        },
                        crate::inspector_pane::InspectorIntent::CaptureSourceDocument {
                            target,
                        } => out.push(Action::CaptureSourceDocument { target }),
                        crate::inspector_pane::InspectorIntent::SetViewer { member, viewer } => {
                            out.push(Action::SetViewerOverride { member, viewer })
                        },
                    }
                }
            },
            _ => {},
        }
        out
    }

    pub(super) fn deliver_inline_event(
        &mut self,
        pane: crate::panes::PaneId,
        event: cambium::KeyEvent,
    ) -> bool {
        if self.renderers.inline_mount(pane).is_none()
            || !self.focus_in_window(self.input_window, self.app.focus)
        {
            return false;
        }
        let plan = if self.input_window == 0 {
            self.surface_plan()
        } else {
            let Some(lens) = self
                .lens_windows
                .values()
                .find(|lens| lens.ordinal as u64 + 1 == self.input_window)
            else {
                return false;
            };
            self.lens_plan(lens.ordinal, lens.width, lens.height)
        };
        let Some(surface) = plan
            .iter()
            .find(|s| s.kind == crate::surface::SurfaceKind::Pane(pane))
        else {
            return false;
        };
        let Some(content) = self.pane_content(pane) else {
            return false;
        };
        self.app.active_pane = Some(pane);
        let dims = (
            surface.rect.w.round().max(1.0) as u32,
            surface.rect.h.round().max(1.0) as u32,
        );
        for action in self.pane_input_actions(pane, &content, (0.0, 0.0), dims, Some(event)) {
            self.act(action);
        }
        if let Some(frame) = self
            .outer_frames
            .borrow_mut()
            .get_mut(&(self.input_window as usize))
        {
            frame.body_focus = self.renderers.inline_focus(pane);
            frame.controls.observe_body_mutations();
        }
        self.request_redraw();
        true
    }

    pub(super) fn projected_a11y_tree(&mut self) -> document_a11y::Projected {
        let pane_rects = self
            .surface_plan()
            .into_iter()
            .filter_map(|surface| match surface.kind {
                crate::surface::SurfaceKind::Pane(pane) => Some((pane, surface.rect)),
                _ => None,
            })
            .collect();
        let focused_pane = match self.app.focus {
            crate::surface::FocusTarget::Pane(pane) => Some(pane),
            _ => None,
        };
        let contributed = crate::contributed_a11y::project(
            &self.renderers.contributed,
            &pane_rects,
            focused_pane,
        );
        let contributed_focus = contributed.focus;
        let mut trees = contributed.trees;
        for (&pane, &rect) in &pane_rects {
            if matches!(self.pane_content(pane), Some(PaneContent::Registered(kind))
                if kind.as_str() == crate::panes::kind::ARRANGE)
                && let Some(tree) = crate::arrange_pane::physics_refusal_tree(&self.app, pane, rect)
            {
                trees.insert(pane, tree);
            }
            if matches!(self.pane_content(pane), Some(PaneContent::Gloss(config))
                if config.sections.iter().any(|id| id == "diagnostics"))
                && let Some(tree) = self
                    .renderers
                    .gloss
                    .get(&pane)
                    .and_then(|surface| surface.diagnostic_tree(pane, rect))
            {
                trees.insert(pane, tree);
            }
        }
        let (tree, routes) =
            crate::a11y::project_app_with_routes_and_contributions(&self.app, trees);
        let focus = contributed_focus.filter(|focus| tree.nodes.iter().any(|(id, _)| id == focus));
        let mut frame = document_a11y::Projected {
            tree,
            routes,
            focus,
            guests: contributed.guests,
            admissions: contributed
                .routes
                .into_iter()
                .map(|(id, route)| (id, a11y_bridge::Admission::Contributed(route)))
                .collect(),
        };
        let header_focus = if self
            .app
            .blueprint_space(crate::action::SpaceRef::Primary)
            .is_none()
        {
            let focused = frame_keys::header_focus_in_window(
                self.outer_focus.as_ref(),
                0,
                self.app.session_id.as_uuid(),
            );
            self.outer_frames
                .borrow()
                .get(&0)
                .and_then(|outer| outer.accessibility(focused))
                .and_then(|outer| {
                    let parent = uxtree::node_id_for_path(&format!(
                        "frisket/{}",
                        self.app.frisket.id.as_str()
                    ));
                    let bodies = crate::panes::pane_projection_ids(&self.app.frisket);
                    crate::frame_a11y::graft(&mut frame.tree, parent, &bodies, outer)
                })
                .and_then(|(routes, focus)| {
                    frame.routes.extend(
                        routes
                            .into_iter()
                            .map(|(id, route)| (id, crate::a11y::A11yRoute::Frame(route))),
                    );
                    focus
                })
        } else {
            None
        };
        let plan = self.surface_plan();
        self.project_document_accessibility(&mut frame, &plan);
        frame.focus = frame_keys::accessibility_focus(
            crate::a11y::host_input_focus(&self.app),
            header_focus,
            frame.focus,
        );
        frame.focus = frame
            .focus
            .filter(|id| frame.tree.nodes.iter().any(|(current, _)| current == id));
        frame
            .routes
            .retain(|id, _| frame.tree.nodes.iter().any(|(node, _)| node == id));
        let mounted: std::collections::HashSet<_> = frame
            .tree
            .nodes
            .iter()
            .filter_map(|(_, n)| n.tree_id())
            .collect();
        frame
            .guests
            .retain(|guest| mounted.contains(&guest.tree_id));
        frame
            .admissions
            .retain(|(tree, _), _| mounted.contains(tree));
        frame
    }

    /// Publish each retained change on its event frame, including the frame
    /// that settles into idle. Observation uses this same composed forest.
    pub(super) fn push_a11y_tree(&mut self) {
        self.sync_content_placements();
        self.drain_surface_accessibility();
        // All losing native parents detach before any destination attaches.
        let current = self.foreign_a11y_placements();
        if self
            .a11y_surface_layout
            .iter()
            .any(|(node, _)| !current.iter().any(|(n, _)| n == node))
        {
            let retained = current
                .into_iter()
                .filter(|(node, _)| self.a11y_surface_layout.iter().any(|(n, _)| n == node))
                .collect();
            self.push_primary_a11y_tree(Some(retained));
        }
        self.withdraw_lens_a11y_departures();
        self.push_primary_a11y_tree(None);
        self.push_lens_a11y_trees();
    }
}
