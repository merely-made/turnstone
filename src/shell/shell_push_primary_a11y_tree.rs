// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn push_primary_a11y_tree(
        &mut self,
        override_placements: Option<Vec<(uuid::Uuid, accesskit::Rect)>>,
    ) {
        let epoch = self
            .a11y_adapter
            .as_ref()
            .map(|bridge| bridge.activation_epoch());
        if epoch != self.a11y_epoch {
            self.surface_a11y.reset_publication();
            self.a11y_actions
                .lock()
                .expect("a11y queue poisoned")
                .publish(Default::default());
            self.a11y_epoch = epoch;
        }
        let placements = override_placements.unwrap_or_else(|| self.foreign_a11y_placements());
        let document_a11y::Projected {
            mut tree,
            mut routes,
            mut focus,
            guests,
            admissions,
        } = self.projected_a11y_tree();
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
        focus =
            frame_keys::accessibility_focus(crate::a11y::host_input_focus(&self.app), None, focus)
                .filter(|id| tree.nodes.iter().any(|(current, _)| current == id));
        // Some app routes outlive their optional projected nodes. A route
        // becomes available to the platform only while its node is published.
        routes.retain(|id, _| tree.nodes.iter().any(|(current, _)| current == id));
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
        let initial = a11y_bridge::safe_initial(&update);
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
        self.a11y_routes = routes;
        let served = self.a11y_adapter.as_mut().is_some_and(|bridge| {
            match bridge.update_batch_at(epoch.unwrap_or(0), initial, updates) {
                Ok(served) => served,
                Err(error) => {
                    tracing::warn!(%error, "a11y publication refused");
                    false
                },
            }
        });
        self.a11y_actions
            .lock()
            .expect("a11y queue poisoned")
            .publish(if served {
                admissions
            } else {
                Default::default()
            });
        if !served {
            self.surface_a11y.retry_publication();
        }
        self.a11y_surface_layout = placements;
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
            let pending = std::mem::take(&mut queue.lock().expect("a11y queue poisoned").requests);
            requests.extend(
                pending
                    .into_iter()
                    .map(|request| (window, queue.clone(), request)),
            );
        }
        for (window, queue, a11y_bridge::QueuedAction { request, admission }) in requests {
            if let Some(captured) = admission {
                if !queue
                    .lock()
                    .expect("a11y queue poisoned")
                    .is_current(&request, &captured)
                {
                    self.app.note(crate::observe::AppEvent::InteractionMissed {
                        what: "a11y-action",
                        target: "retired accessibility admission".into(),
                    });
                    continue;
                }
                let landed = match captured.admission {
                    a11y_bridge::Admission::PoolDocument(route) => {
                        self.dispatch_pool_document_accessibility(&route, &request)
                    },
                    #[cfg(test)]
                    a11y_bridge::Admission::Document(_route) => {
                        false /* legacy routes exist only in queue tests */
                    },
                    a11y_bridge::Admission::Contributed(published) => {
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
                        let plan = contributed_action_plan(
                            &self.app,
                            &self.content_sessions,
                            &windows,
                            origin,
                        );
                        let spec = self.contributed_pane_spec(
                            published.route.pane,
                            self.pane_content(published.route.pane).as_ref(),
                        );
                        dispatch_contributed_action(
                            &mut self.app,
                            &mut self.renderers.contributed,
                            &published,
                            &request,
                            &plan,
                            spec.as_ref(),
                        )
                    },
                };
                if !landed {
                    self.app.note(crate::observe::AppEvent::InteractionMissed {
                        what: "a11y-action",
                        target: "stale or unsupported live document action".into(),
                    });
                }
                self.request_redraw();
                continue;
            }
            if request.target_tree != accesskit::TreeId::ROOT {
                let (target, placements) = match window {
                    None => (
                        self.surface_a11y.action_target(&request),
                        self.foreign_a11y_placements(),
                    ),
                    Some(id) => match self.lens_windows.get(&id) {
                        Some(lens) => (
                            lens.a11y_forest.action_target(&request),
                            self.window_foreign_a11y_placements(lens.ordinal + 1),
                        ),
                        None => (
                            Err("native accessibility window closed".to_owned()),
                            Vec::new(),
                        ),
                    },
                };
                let result = foreign_a11y::deliver_foreign_action(
                    &mut self.content_sessions,
                    target,
                    &placements,
                    request,
                );
                if let Err(reason) = result {
                    self.app.note(crate::observe::AppEvent::InteractionMissed {
                        what: "a11y-action",
                        target: reason,
                    });
                }
                continue;
            }
            let (slot, route) = match window {
                None => (0, self.a11y_routes.get(&request.target_node).cloned()),
                Some(id) => match self.lens_windows.get(&id) {
                    Some(lens) => (
                        lens.ordinal + 1,
                        lens.a11y_routes.get(&request.target_node).cloned(),
                    ),
                    None => continue,
                },
            };
            if let Some(crate::a11y::A11yRoute::Frame(route)) = &route {
                if !self.apply_frame_a11y(slot, route, request.action) {
                    self.app.note(crate::observe::AppEvent::InteractionMissed {
                        what: "a11y-action",
                        target: "Shared frame control is stale, hidden or unsupported".into(),
                    });
                }
                continue;
            }
            if !matches!(
                request.action,
                accesskit::Action::Click | accesskit::Action::Focus
            ) {
                continue;
            }
            if request.action == accesskit::Action::Focus
                && slot == 0
                && crate::a11y::focus_host_input(&mut self.app, request.target_node)
            {
                self.request_redraw();
                continue;
            }
            // App-only routes are activation routes. A reader moving focus
            // across them must not perform their default action.
            if request.action != accesskit::Action::Click {
                continue;
            }
            if let Some(crate::a11y::A11yRoute::SelectMember { pane, .. }) = &route {
                if self.app.space_of(*pane) != Some(frame_keys::space(slot)) {
                    self.app.note(crate::observe::AppEvent::InteractionMissed {
                        what: "a11y-action",
                        target: "Graph projection moved to another window".into(),
                    });
                    continue;
                }
            }
            let effects =
                crate::a11y::apply_route(&mut self.app, route.as_ref(), request.target_node);
            self.run_effects(effects);
            self.request_redraw();
        }
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
