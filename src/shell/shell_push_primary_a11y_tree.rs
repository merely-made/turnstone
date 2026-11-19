// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[path = "shell_uxtree_snapshot.rs"]
mod shell_uxtree_snapshot;

struct PrimaryA11yFrame {
    update: accesskit::TreeUpdate,
    visible: Vec<uuid::Uuid>,
    placements: Vec<(uuid::Uuid, accesskit::Rect)>,
    guests: Vec<accesskit::TreeUpdate>,
    admissions: std::collections::HashMap<a11y_bridge::AdmissionKey, a11y_bridge::Admission>,
    routes: crate::a11y::ActionEnvelopes,
}

impl Shell {
    fn primary_a11y_frame(
        &mut self,
        override_placements: Option<Vec<(uuid::Uuid, accesskit::Rect)>>,
        plan: &[crate::surface::Surface],
    ) -> PrimaryA11yFrame {
        let placements = override_placements
            .unwrap_or_else(|| self.window_foreign_a11y_placements_with(0, plan.to_vec()));
        let document_a11y::Projected {
            mut tree,
            mut routes,
            mut focus,
            guests,
            admissions,
        } = self.projected_a11y_tree_in(plan);
        let mut visible = Vec::new();
        for &(node, rect) in &placements {
            let Some(mut graft) = self.surface_a11y.graft(node, rect) else {
                continue;
            };
            let id = uxtree::node_id_for_path(&format!("turnstone/doc/{node}"));
            // Retire the former structural outline as well as its root's
            // child-list. The OS must not receive orphan outline nodes beside
            // the real native subtree.
            let mut pending = tree
                .nodes
                .iter()
                .find(|(candidate, _)| *candidate == id)
                .map(|(_, old)| old.children().to_vec())
                .unwrap_or_default();
            let mut retired = std::collections::HashSet::new();
            while let Some(child) = pending.pop() {
                if retired.insert(child) {
                    if let Some((_, old)) =
                        tree.nodes.iter().find(|(candidate, _)| *candidate == child)
                    {
                        pending.extend(old.children().iter().copied());
                    }
                }
            }
            tree.nodes
                .retain(|(candidate, _)| !retired.contains(candidate));
            routes.retain(|candidate, _| !retired.contains(candidate));
            if let Some((_, existing)) = tree
                .nodes
                .iter_mut()
                .find(|(candidate, _)| *candidate == id)
            {
                if let Some(label) = existing.label() {
                    graft.set_label(label.to_owned());
                }
                *existing = graft;
            } else {
                graft.set_label("browser page");
                if let Some((_, root)) = tree
                    .nodes
                    .iter_mut()
                    .find(|(candidate, _)| *candidate == tree.root)
                {
                    let mut children = root.children().to_vec();
                    children.push(id);
                    root.set_children(children);
                }
                tree.nodes.push((id, graft));
            }
            if !frame_keys::header_focus_in_window(
                self.outer_focus.as_ref(),
                0,
                self.app.session_id.as_uuid(),
            ) && matches!(self.app.focus, crate::surface::FocusTarget::Content { node: focused, .. } if focused == node)
            {
                focus = Some(id);
            }
            visible.push(node);
        }
        // An open host editor owns keyboard focus even while a document
        // remains the selected graph content behind it.
        let published: std::collections::HashSet<_> =
            tree.nodes.iter().map(|(id, _)| *id).collect();
        focus =
            frame_keys::accessibility_focus(crate::a11y::host_input_focus(&self.app), None, focus)
                .filter(|id| published.contains(id));
        // Some app routes outlive their optional projected nodes. A route
        // becomes available to the platform only while its node is published.
        routes.retain(|id, _| published.contains(id));
        let mut update = tree.to_tree_update(focus);
        // Narrator refuses to walk past a node without a bounding rectangle:
        // UIA treats boundless elements as off-screen, so the tree served
        // without these read as "three buttons, the omnibar, then nothing".
        // Contributed retained DOM nodes already carry exact pane-translated
        // fragment bounds. App-authored structural nodes do not yet, so only
        // those boundless nodes claim the window's extent: coarse geometry,
        // correct names and structure, without overwriting precise surfaces.
        let bounds = accesskit::Rect {
            x0: 0.0,
            y0: 0.0,
            x1: self.width.max(1) as f64,
            y1: self.height.max(1) as f64,
        };
        for (_, node) in &mut update.nodes {
            if node.bounds().is_none() {
                node.set_bounds(bounds);
            }
        }
        PrimaryA11yFrame {
            update,
            visible,
            placements,
            guests,
            admissions,
            routes,
        }
    }

    pub(super) fn push_primary_a11y_tree(
        &mut self,
        override_placements: Option<Vec<(uuid::Uuid, accesskit::Rect)>>,
        plan: &[crate::surface::Surface],
    ) {
        let epoch = self
            .a11y_adapter
            .as_ref()
            .map(|bridge| bridge.activation_epoch());
        if epoch != self.a11y_epoch {
            self.surface_a11y.reset_publication();
            self.a11y_actions.clear_publication();
            self.a11y_epoch = epoch;
        }
        let PrimaryA11yFrame {
            update,
            visible,
            placements,
            guests,
            admissions,
            routes,
        } = self.primary_a11y_frame(override_placements, plan);
        let updates = match self
            .surface_a11y
            .frame_with_guests(update, &visible, &guests)
        {
            Ok(updates) => updates,
            Err(error) => {
                tracing::warn!(%error, "foreign accessibility publication refused");
                return;
            },
        };
        let served = self.a11y_adapter.as_mut().is_some_and(|publisher| {
            match publisher.publish_at(
                epoch.unwrap_or(0),
                updates,
                self.surface_a11y.tree_lineages(),
                a11y_bridge::window_admissions(admissions, &routes, &self.surface_a11y),
                a11y_bridge::admission_equivalence(),
            ) {
                Ok(served) => served,
                Err(error) => {
                    tracing::warn!(%error, "a11y publication refused");
                    false
                },
            }
        });
        if !served {
            self.surface_a11y.retry_publication();
        }
        self.a11y_surface_layout = placements;
    }

    pub(super) fn lint_primary_a11y(&mut self) -> uxtree::lint::Report {
        use crate::a11y::A11yRoute;
        use accesskit::{Action, TreeId};

        self.sync_content_placements();
        self.drain_surface_accessibility();
        let plan = self.surface_plan();
        let PrimaryA11yFrame {
            update,
            visible,
            guests,
            admissions,
            routes,
            ..
        } = self.primary_a11y_frame(None, &plan);
        let foreign = self.surface_a11y.snapshots_for_lint(&visible);
        let forward: std::collections::HashSet<_> =
            foreign.iter().map(|tree| tree.tree_id).collect();
        let mut trees = vec![update];
        trees.extend(guests);
        trees.extend(foreign);
        uxtree::lint::inspect(
            TreeId::ROOT,
            &trees,
            |(tree, local)| tree == TreeId::ROOT && routes.contains_key(&local),
            |address @ (tree, local), action| {
                if tree == TreeId::ROOT {
                    return match routes.get(&local).map(|envelope| &envelope.provenance) {
                        Some(A11yRoute::Frame(_)) => true,
                        Some(A11yRoute::OpenOmnibar) => {
                            matches!(action, Action::Click | Action::Focus)
                        },
                        Some(
                            A11yRoute::Action(_)
                            | A11yRoute::OmnibarRow { .. }
                            | A11yRoute::SelectMember { .. },
                        ) => action == Action::Click,
                        _ => false,
                    };
                }
                match admissions.get(&address) {
                    Some(a11y_bridge::Admission::PoolDocument(_)) => {
                        document_a11y::request_action(action).is_some()
                    },
                    Some(a11y_bridge::Admission::Contributed(_)) => {
                        matches!(action, Action::Click | Action::Focus)
                    },
                    _ => forward.contains(&tree),
                }
            },
        )
    }

    /// Drain assistive actions queued by the platform thread and lower each
    /// through the route table. Runs on the main thread, woken by the same
    /// proxy the actors use.
    pub(super) fn drain_a11y_actions(&mut self) {
        self.drain_surface_accessibility();
        for lens in self.lens_windows.values_mut() {
            lens.a11y_forest.sync_retained_from(&self.surface_a11y);
        }
        let mut queues = vec![(None, self.a11y_actions.clone())];
        queues.extend(
            self.lens_windows
                .iter()
                .map(|(id, lens)| (Some(*id), lens.a11y_actions.clone())),
        );
        let mut requests = Vec::new();
        for (window, queue) in queues {
            requests.extend(
                queue
                    .take()
                    .into_iter()
                    .map(|request| (window, queue.clone(), request)),
            );
        }
        for (window, queue, queued) in requests {
            // Activation can change on the platform thread before another
            // event frame has reset the source forest and queued bindings.
            let same_activation = match window {
                None => self
                    .a11y_adapter
                    .as_ref()
                    .is_some_and(|publisher| self.a11y_epoch == Some(publisher.activation_epoch())),
                Some(id) => self.lens_windows.get(&id).is_some_and(|lens| {
                    lens.a11y_epoch == Some(lens.a11y.activation_epoch())
                        && lens.a11y_scope.session == self.app.session_id.as_uuid().as_u128()
                }),
            };
            let native_target_is_current = queue.native_target_is_current(&queued);
            let request = queued.request;
            let Some(captured) = queued.admission.filter(|captured| {
                same_activation && native_target_is_current && queue.is_current(&request, captured)
            }) else {
                self.app.note(crate::observe::AppEvent::InteractionMissed {
                    what: "a11y-action",
                    target: "retired or absent accessibility admission".into(),
                });
                continue;
            };
            use a11y_bridge::Admission;
            let landed = match a11y_bridge::with_captured_generation(captured) {
                Admission::Host(envelope) => {
                    self.dispatch_host_accessibility(window, envelope.provenance, request);
                    continue;
                },
                Admission::Guest(witness) => {
                    self.dispatch_guest_accessibility(window, &witness, request);
                    continue;
                },
                Admission::PoolDocument(route) => {
                    self.dispatch_pool_document_accessibility(&route, &request)
                },
                Admission::Contributed(published) => {
                    self.dispatch_published_contributed_accessibility(window, &published, &request)
                },
                #[cfg(test)]
                Admission::Document(_) => false,
            };
            if !landed {
                self.app.note(crate::observe::AppEvent::InteractionMissed {
                    what: "a11y-action",
                    target: "stale or unsupported live document action".into(),
                });
            }
            self.request_redraw();
        }
    }

    fn dispatch_guest_accessibility(
        &mut self,
        window: Option<winit::window::WindowId>,
        witness: &cambium_winit_a11y::guest::GuestAdmission<uuid::Uuid>,
        request: accesskit::ActionRequest,
    ) {
        let address = (request.target_tree, request.target_node);
        let (forest, slot) = match window {
            None => (Some(&self.surface_a11y), 0),
            Some(id) => self.lens_windows.get(&id).map_or((None, 0), |lens| {
                (Some(&lens.a11y_forest), lens.ordinal + 1)
            }),
        };
        let target = forest
            .filter(|forest| forest.matches_guest(address, witness))
            .ok_or_else(|| "foreign accessibility admission retired".to_owned())
            .and_then(|forest| forest.action_target(&request));
        let placements = match (target.is_ok(), slot) {
            (true, 0) => self.foreign_a11y_placements(),
            (true, _) => self.window_foreign_a11y_placements(slot),
            (false, _) => Vec::new(),
        };
        if let Err(reason) = foreign_a11y::deliver_foreign_action(
            &mut self.content_sessions,
            target,
            &placements,
            request,
        ) {
            self.app.note(crate::observe::AppEvent::InteractionMissed {
                what: "a11y-action",
                target: reason,
            });
        }
    }

    fn dispatch_published_contributed_accessibility(
        &mut self,
        window: Option<winit::window::WindowId>,
        published: &crate::contributed_a11y::PublishedRoute,
        request: &accesskit::ActionRequest,
    ) -> bool {
        let origin = match window {
            None => Some(crate::action::SpaceRef::Primary),
            Some(id) => self
                .lens_windows
                .get(&id)
                .map(|lens| crate::action::SpaceRef::Lens(lens.ordinal)),
        };
        let windows = ContributedActionWindows {
            primary: (self.width, self.height),
            lenses: self
                .lens_windows
                .values()
                .map(|lens| (lens.ordinal, lens.width, lens.height))
                .collect(),
        };
        let plan = contributed_action_plan(&self.app, &self.content_sessions, &windows, origin);
        let spec = self.contributed_pane_spec(
            published.route.pane,
            self.pane_content(published.route.pane).as_ref(),
        );
        dispatch_contributed_action(
            &mut self.app,
            &mut self.renderers.contributed,
            published,
            request,
            &plan,
            spec.as_ref(),
        )
    }

    fn dispatch_host_accessibility(
        &mut self,
        window: Option<winit::window::WindowId>,
        route: crate::a11y::A11yRoute,
        request: accesskit::ActionRequest,
    ) {
        let slot = match window {
            None => 0,
            Some(id) => match self.lens_windows.get(&id) {
                Some(lens) => lens.ordinal + 1,
                None => return,
            },
        };
        if let crate::a11y::A11yRoute::Frame(route) = &route {
            if !self.apply_frame_a11y(slot, route, request.action) {
                self.app.note(crate::observe::AppEvent::InteractionMissed {
                    what: "a11y-action",
                    target: "Shared frame control is stale, hidden or unsupported".into(),
                });
            }
            return;
        }
        if !matches!(
            request.action,
            accesskit::Action::Click | accesskit::Action::Focus
        ) {
            return;
        }
        if request.action == accesskit::Action::Focus
            && slot == 0
            && matches!(route, crate::a11y::A11yRoute::OpenOmnibar)
            && crate::a11y::focus_host_input(&mut self.app, request.target_node)
        {
            self.request_redraw();
            return;
        }
        // App-only routes are activation routes. A reader moving focus
        // across them must not perform their default action.
        if request.action != accesskit::Action::Click {
            return;
        }
        if let crate::a11y::A11yRoute::SelectMember { pane, .. } = &route {
            if self.app.space_of(*pane) != Some(frame_keys::space(slot)) {
                self.app.note(crate::observe::AppEvent::InteractionMissed {
                    what: "a11y-action",
                    target: "Graph projection moved to another window".into(),
                });
                return;
            }
        }
        let effects = crate::a11y::apply_route(&mut self.app, Some(&route), request.target_node);
        self.run_effects(effects);
        self.request_redraw();
    }

    /// Run a scenario `script` step through the Piccolo control lane and lower
    /// its Actions through the same `act` spine a keypress takes — the
    /// automation runner of the "one description, two runners" pair. Without
    /// the `piccolo` feature the step is an honest, attributable failure
    /// rather than a silent skip.
    #[cfg(feature = "piccolo")]
    pub(super) fn run_scenario_script(&mut self, source: &str) {
        match crate::script::run_control(&self.app, source, 5000) {
            Ok(actions) => {
                for action in actions {
                    self.act(action);
                }
            },
            Err(err) => {
                tracing::warn!(%err, "scenario script failed");
                self.app.note(crate::observe::AppEvent::InteractionMissed {
                    what: "script",
                    target: err,
                });
            },
        }
    }

    #[cfg(not(feature = "piccolo"))]
    pub(super) fn run_scenario_script(&mut self, _source: &str) {
        tracing::warn!("scenario `script` step needs the `piccolo` feature; skipped");
        self.app.note(crate::observe::AppEvent::InteractionMissed {
            what: "script",
            target: "piccolo feature off".to_string(),
        });
    }

    /// Advance the self-drive scenario one step after each rendered frame.
    /// Steps lower to Actions through the same spine as a keypress; a Done
    /// tick writes the sentinel and exits without an implicit session save.
    /// Explicit product actions, including Settings Apply, retain their normal
    /// persistence behavior; acceptance runs use an isolated profile.
    /// Write the shared driver's outcome in turnstone's `scenario.done` format
    /// (first line `RESULT ok`/`RESULT fail`, then the log), so the same headed
    /// harness that waits on the turnstone driver reads a shared run identically.
    pub(super) fn write_shared_done(&mut self, outcome: &taproot::Outcome) {
        let mut outcome = outcome.clone();
        self.main_receipt.qualify(&mut outcome);
        if let Err(error) = self
            .diagnostic_observations
            .write_scenario_receipt(&self.shared_out_dir, &outcome)
        {
            eprintln!("turnstone: scenario receipt write failed: {error}");
        }
    }
}
