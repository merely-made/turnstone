// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
        if let Some(fields) = line.strip_prefix("wait-coop ") {
            self.pending_coop_wait = Some(PendingCoopWait::new(fields)?);
            return Ok(());
        }
        if let Some(verb) = crate::scenario::pack_verbs::parse(line) {
            return self.run_pack_verb(verb?);
        }
        if let Some(verb) = crate::scenario::mod_verbs::parse(line) {
            return self.run_mod_verb(verb?);
        }
        if let Some(result) = a11y_lint::step(line, || self.lint_primary_a11y()) {
            return result;
        }
        if let Some(args) = line.strip_prefix("assert node ") {
            return self.assert_semantic_node(args);
        }
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
                .filter_map(|(id, envelope)| match &envelope.provenance {
                    crate::a11y::A11yRoute::Frame(route)
                        if matches!(&route.control, crate::frame_controls::Control::Body { node, .. }
                            if matches.contains(node))
                        && nodes
                            .iter()
                            .any(|(n, current)| n == id && current.supports_action(action)) =>
                    {
                        Some(route.clone())
                    },
                    _ => None,
                })
                .collect::<Vec<_>>();
            drop(dom);
            if targets.len() != 1 {
                return Err("inline target is absent, ambiguous or unsupported".into());
            }
            // Scenario steps observe and dispatch synchronously on the UI
            // thread. The frame still checks the current control and owner.
            return self
                .apply_frame_a11y(slot, &targets[0], action)
                .then_some(())
                .ok_or_else(|| "inline frame control refused the action".into());
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
                .filter_map(|(id, envelope)| match &envelope.provenance {
                    crate::a11y::A11yRoute::Frame(route)
                        if target.matches(&route.control)
                            && tree
                                .nodes
                                .iter()
                                .any(|(node, n)| node == id && n.supports_action(action)) =>
                    {
                        Some(route.clone())
                    },
                    _ => None,
                })
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err("lens frame target is absent, ambiguous or unsupported".into());
            }
            return self
                .apply_frame_a11y(slot, &matches[0], action)
                .then_some(())
                .ok_or_else(|| "lens frame control refused the action".into());
        }
        if let Some(args) = line.strip_prefix("frame-a11y ") {
            let (action, target) = crate::frame_a11y::fixture_request(args)?;
            let super::super::document_a11y::Projected { tree, routes, .. } =
                self.projected_a11y_tree();
            let matches = routes
                .iter()
                .filter_map(|(id, envelope)| match &envelope.provenance {
                    crate::a11y::A11yRoute::Frame(route)
                        if target.matches(&route.control)
                            && tree
                                .nodes
                                .iter()
                                .any(|(node, n)| node == id && n.supports_action(action)) =>
                    {
                        Some(route.clone())
                    },
                    _ => None,
                })
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err(
                    "frame-a11y target is absent, ambiguous or does not support that action".into(),
                );
            }
            return self
                .apply_frame_a11y(0, &matches[0], action)
                .then_some(())
                .ok_or_else(|| "primary frame control refused the action".into());
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
            let _ = window.request_surface_size(winit::dpi::LogicalSize::new(width, height).into());
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
    /// Run a mod project scenario verb: act, or copy a file over a component.
    fn run_mod_verb(&mut self, verb: crate::scenario::mod_verbs::ModVerb) -> Result<(), String> {
        use crate::scenario::mod_verbs::ModVerb;
        match verb {
            ModVerb::Step(step) => {
                self.act(crate::action::Action::ModProject(step));
            },
            ModVerb::GraphAdd(title) => {
                let graph = self.app.graph_runtimes.active_graph();
                let url = format!("https://viewer-receipt.example/{}", title.to_lowercase().replace(' ', "-"));
                let member = self
                    .app
                    .graph_runtimes
                    .canvas_mut(graph)
                    .ok_or("graph absent")?
                    .add_node_at((0.0, 0.0), &url);
                self.act(crate::action::Action::ContentTitleChanged { member, title });
            },
            ModVerb::GraphRetitle { from, to } => {
                let member = self
                    .app
                    .graph_runtimes
                    .graph()
                    .nodes()
                    .find(|(_, node)| node.title == from)
                    .map(|(_, node)| node.id)
                    .ok_or_else(|| format!("graph-retitle: no node titled '{from}'"))?;
                self.act(crate::action::Action::ContentTitleChanged { member, title: to });
            },
            ModVerb::CopyFile { from, to } => {
                std::fs::copy(&from, &to)
                    .map_err(|error| format!("copy-file '{from}' to '{to}': {error}"))?;
            },
        }
        Ok(())
    }

    /// Run a command pack scenario verb: act, or edit a file the pack reads.
    fn run_pack_verb(&mut self, verb: crate::scenario::pack_verbs::PackVerb) -> Result<(), String> {
        use crate::scenario::pack_verbs::PackVerb;
        match verb {
            PackVerb::Install { path, dev } => {
                self.act(crate::action::Action::InstallCommandPack { path, dev });
            },
            PackVerb::Reload(id) => {
                self.act(crate::action::Action::ReloadCommandPack { id });
            },
            PackVerb::WriteFile { path, text } => {
                std::fs::write(&path, text).map_err(|error| format!("write-file '{path}': {error}"))?;
            },
        }
        Ok(())
    }
}
