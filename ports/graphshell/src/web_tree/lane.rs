// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Product observations and actions for Mesquite on the tree page.
use super::*;

/// The tree page's half of the scenario lane.
pub(super) struct TreeLane {
    pub(super) shared: Rc<Shared>,
    pub(super) errors: Vec<String>,
    pub(super) pointer: Option<(f32, f32)>,
    /// Page px where the last `release-at` let go, for `drag-return`.
    pub(super) drop: Option<(f32, f32)>,
    /// The same drop in world units, for `drag-return-world`.
    pub(super) drop_world: Option<(f32, f32)>,
}

/// `layout-*` fields are a pairwise pass plus all-pairs hop distances, so a
/// snapshot computes them only for graphs up to this size.
const LAYOUT_STATS_LIMIT: usize = 512;

/// The canvas leaf's painted rect in page px.
fn leaf_rect(ctx: &AppCtx<'_, TreePage, Logic, Child>) -> Option<(f32, f32, f32, f32)> {
    let leaf = {
        let dom = ctx.runner.dom();
        let dom = dom.borrow();
        taproot::matching(&dom, &Selector::class("tree-canvas"))
            .into_iter()
            .next()
    }?;
    ctx.painted_rect(leaf)
}

impl TreeLane {
    /// The focused node's page point, for `press-focused`.
    fn focused_point(
        &self,
        ctx: &AppCtx<'_, TreePage, Logic, Child>,
    ) -> Result<(f32, f32), String> {
        let (x, y) = self
            .shared
            .canvas
            .borrow()
            .focused_screen_position()
            .ok_or("wants exactly one focused node")?;
        let (left, top, _, _) = leaf_rect(ctx).ok_or("the canvas leaf is not painted")?;
        Ok((left + x, top + y))
    }

    /// The physics panel's and canvas's observations.
    fn physics_fields(
        &self,
        ctx: &AppCtx<'_, TreePage, Logic, Child>,
        snapshot: ProbeSnapshot,
    ) -> ProbeSnapshot {
        let page = ctx.runner.state();
        let canvas = self.shared.canvas.borrow();
        let choice = canvas.physics_choice();
        let checked = {
            let dom = ctx.runner.dom();
            let dom = dom.borrow();
            mere::canvas::CANVAS_PHYSICS_OVERLAYS
                .iter()
                .filter(|(_, label)| {
                    !taproot::matching(
                        &dom,
                        &Selector::role("checkbox")
                            .containing(*label)
                            .with_attr("aria-checked", "true"),
                    )
                    .is_empty()
                })
                .map(|(id, _)| *id)
                .collect::<Vec<_>>()
                .join(",")
        };
        // The overlay boxes greyed while the picked law refuses overlays.
        let disabled = {
            let dom = ctx.runner.dom();
            let dom = dom.borrow();
            mere::canvas::CANVAS_PHYSICS_OVERLAYS
                .iter()
                .filter(|(_, label)| {
                    !taproot::matching(
                        &dom,
                        &Selector::role("checkbox")
                            .containing(*label)
                            .with_attr("aria-disabled", "true"),
                    )
                    .is_empty()
                })
                .count()
                .to_string()
        };
        let drag_return = self.drop.and_then(|(dx, dy)| {
            let (x, y) = canvas.focused_screen_position()?;
            let (left, top, _, _) = leaf_rect(ctx)?;
            Some((left + x - dx).hypot(top + y - dy))
        });
        let mut snapshot = snapshot
            .with_field("ready", "true")
            .with_field(
                "tools-mode",
                if tools_docked(page) {
                    "docked"
                } else if page.tools_open {
                    "open"
                } else {
                    "collapsed"
                },
            )
            .with_field("layout", page.physics.layout_id.clone())
            .with_field("physics-law", choice.law.id())
            .with_field(
                "physics-overlays",
                choice
                    .overlays
                    .iter()
                    .map(|overlay| overlay.id())
                    .collect::<Vec<_>>()
                    .join(","),
            )
            .with_field(
                "physics-profile",
                graphshell::canvas_physics::profile_id(&canvas),
            )
            .with_field("physics-kind-source", choice.kind.id())
            .with_field("physics-mass-source", choice.mass.id())
            .with_field("physics-depth-source", choice.depth.id())
            .with_field("panel-law", page.physics.choice().law.id())
            .with_field("panel-overlays", page.physics.ticked())
            .with_field("panel-profile", page.physics.profile_id())
            .with_field("panel-status", page.physics.status.clone())
            .with_field("checked-overlays", checked)
            .with_field("disabled-overlays", disabled)
            .with_field("canvas-nodes", canvas.graph().node_count().to_string())
            .with_field("physics-settling", canvas.is_settling().to_string())
            .with_field(
                "physics-continuous",
                canvas.physics_tick_demand().0.to_string(),
            )
            .with_field("physics-budget", canvas.physics_tick_demand().1.to_string())
            .with_field(
                "dragging-node",
                canvas
                    .dragging_node()
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            )
            .with_field(
                "drag-return",
                drag_return
                    .map(|distance| format!("{distance:.0}"))
                    .unwrap_or_default(),
            )
            // The drop distance on the first frame after a release that
            // executed physics steps, and how many it executed: keyed to
            // steps, not render frames.
            .with_field(
                "drag-return-step",
                self.shared
                    .release_step
                    .get()
                    .map(|(distance, _)| format!("{distance:.1}"))
                    .unwrap_or_default(),
            )
            .with_field(
                "drag-return-step-world",
                self.shared
                    .release_step_world
                    .get()
                    .map(|distance| format!("{distance:.1}"))
                    .unwrap_or_default(),
            )
            .with_field(
                "drag-return-world",
                self.drop_world
                    .zip(canvas.focused_world_position())
                    .map(|((dx, dy), (x, y))| format!("{:.0}", (x - dx).hypot(y - dy)))
                    .unwrap_or_default(),
            )
            .with_field(
                "drag-return-steps",
                self.shared
                    .release_step
                    .get()
                    .map(|(_, steps)| steps.to_string())
                    .unwrap_or_default(),
            );
        if canvas.graph().node_count() <= LAYOUT_STATS_LIMIT {
            let stats = canvas.layout_stats();
            snapshot = snapshot
                .with_field("layout-spread", format!("{:.0}", stats.spread))
                .with_field("layout-overlaps", stats.overlaps.to_string())
                .with_field("layout-stretch", format!("{:.2}", stats.stretch))
                .with_field(
                    "layout-mass-area-rank",
                    format!("{:.2}", stats.mass_area_rank),
                )
                .with_field("layout-density-cv", format!("{:.3}", stats.density_cv));
            if let Some(start) = page.physics.law_start {
                for (name, value) in start.fields(&stats) {
                    snapshot = snapshot.with_field(name, value);
                }
            }
        } else {
            // Past the all-pairs limit, spread and overlaps (same
            // definition, a grid) still report; stretch does not.
            let stats = canvas.layout_stats_without_stretch();
            snapshot = snapshot
                .with_field("layout-spread", format!("{:.0}", stats.spread))
                .with_field("layout-overlaps", stats.overlaps.to_string());
        }
        for (name, value) in graphshell::canvas_physics::framing_fields(&canvas) {
            snapshot = snapshot.with_field(name, value);
        }
        // The GPU repulsion lane (P5c): whether the page has a device, and
        // the lane's counts, the receipt's proof the device ran.
        let device = self.shared.physics_device.borrow();
        let lane = canvas.repulsion_stats().unwrap_or_default();
        snapshot = snapshot
            .with_field(
                "physics-device",
                if device.is_some() { "on" } else { "off" },
            )
            .with_field(
                "gpu-threshold",
                device
                    .as_ref()
                    .map(|device| device.threshold().to_string())
                    .unwrap_or_default(),
            )
            .with_field("gpu-device-steps", lane.device_steps.to_string())
            .with_field("gpu-cpu-steps", lane.cpu_steps.to_string())
            .with_field("gpu-submissions", lane.submissions.to_string())
            .with_field("gpu-failures", lane.failures.to_string())
            .with_field(
                "gpu-answers",
                device
                    .as_ref()
                    .map(|device| device.answers().to_string())
                    .unwrap_or_default(),
            );
        snapshot
    }

    /// The remote session's observations, named as the old page names them.
    fn remote_fields(
        &self,
        ctx: &AppCtx<'_, TreePage, Logic, Child>,
        snapshot: ProbeSnapshot,
    ) -> ProbeSnapshot {
        let page = ctx.runner.state();
        let remote = self.shared.remote.borrow();
        let board = &remote.board;
        let live = remote.live.as_ref();
        snapshot
            .with_field(
                "session",
                match page.session {
                    remote::Session::Local => "local",
                    remote::Session::Remote => "remote",
                },
            )
            .with_field("active-session", remote::active_line(page))
            .with_field(
                "tools-sections",
                format!(
                    "physics:{},remote:{}",
                    if page.sections.physics.expanded { "open" } else { "closed" },
                    if page.sections.remote.expanded { "open" } else { "closed" },
                ),
            )
            .with_field("remote-link", if live.is_some() { "webrtc" } else { "none" })
            .with_field("remote-state", remote.status())
            .with_field(
                "remote-revision",
                remote.revision().map(|r| r.to_string()).unwrap_or_default(),
            )
            .with_field(
                "remote-cards",
                remote
                    .mounted()
                    .map(|mounted| mounted.scene.tables.items.len().to_string())
                    .unwrap_or_default(),
            )
            .with_field(
                "remote-resume",
                live.map(|live| live.session.last_resume().to_string())
                    .unwrap_or_default(),
            )
            .with_field(
                "remote-rejoins",
                live.map(|live| live.session.rejoins().to_string())
                    .unwrap_or_default(),
            )
            .with_field(
                "remote-actions",
                remote
                    .actions()
                    .into_iter()
                    .map(|(label, _)| label)
                    .collect::<Vec<_>>()
                    .join("|"),
            )
            .with_field(
                "remote-physics-law",
                self.shared.canvas.borrow().physics_law().id(),
            )
            .with_field(
                "remote-physics-speed",
                crate::web_speed::field(remote.board.speed()),
            )
            .with_field("remote-energy", format!("{:.1}", board.energy()))
            .with_field(
                "remote-gap",
                board
                    .gap("0", "1")
                    .map(|gap| format!("{gap:.0}"))
                    .unwrap_or_default(),
            )
            .with_field("remote-overlaps", board.overlaps().to_string())
            .with_field(
                "remote-draft-open",
                remote.form().draft.is_some().to_string(),
            )
            .with_field("action-status", remote.form().status.clone())
            // What the board's slot tells a reader: each card's name and the
            // rectangle it is painted in, leaf-local px.
            .with_field(
                "board-nodes",
                if self.shared.remote_shown.get() {
                    let (width, height) = self.shared.size.get();
                    remote::board_semantics(&remote, width, height)
                        .children
                        .iter()
                        .map(|node| {
                            let [x, y, w, h] = node.rect;
                            format!("{}@{x:.0},{y:.0},{w:.0},{h:.0}", node.name)
                        })
                        .collect::<Vec<_>>()
                        .join("|")
                } else {
                    String::new()
                },
            )
    }

    /// `click-node <url>`: press and release over a node, through the host's
    /// pointer path, so the pick runs as a real click's would.
    fn node_point(
        &self,
        ctx: &mut AppCtx<'_, TreePage, Logic, Child>,
        url: &str,
    ) -> Result<(f32, f32), String> {
        let (x, y) = {
            let canvas = self.shared.canvas.borrow();
            let (key, _) = canvas
                .graph()
                .get_node_by_url(url)
                .ok_or_else(|| format!("click-node {url}: no such node"))?;
            canvas
                .screen_position_of(key)
                .ok_or_else(|| format!("click-node {url}: not on screen"))?
        };
        let leaf = {
            let dom = ctx.runner.dom();
            let dom = dom.borrow();
            taproot::matching(&dom, &Selector::class("tree-canvas"))
                .into_iter()
                .next()
        }
        .ok_or("the canvas leaf is not in the tree")?;
        let (left, top, width, height) = ctx
            .painted_rect(leaf)
            .ok_or("the canvas leaf is not painted")?;
        if x < 0.0 || y < 0.0 || x >= width || y >= height {
            return Err(format!(
                "click-node {url}: ({x}, {y}) is outside canvas ({left}, {top}, {width}, {height})"
            ));
        }
        Ok((left + x, top + y))
    }
}

impl Product for TreeLane {
    type State = TreePage;
    type Logic = Logic;
    type View = Child;
    const KIND: &'static str = "graphshell-tree";
    const SURFACE: &'static str = "tree";
    const LOG_PREFIX: &'static str = "graphshell-tree";

    fn close_on_completion(&self) -> bool {
        false
    }
    fn inspect(&mut self, name: &str, frame: &Frame) {
        if let Err(error) =
            web_scenario::publish_capture(name, frame.width, frame.height, &frame.rgba)
        {
            self.errors.push(error);
        }
    }
    fn complete(
        &mut self,
        ctx: &mut mesquite::Ctx<'_, Self>,
        outcome: &taproot::Outcome,
    ) -> Result<(), String> {
        let mut lines = vec![format!("RESULT {}", if outcome.ok { "ok" } else { "fail" })];
        lines.extend(outcome.log.iter().cloned());
        lines.extend(self.receipt_lines());
        let saved = ctx.runner.state().product.as_ref().map(|product| {
            json!({
                "session": product.session, "selected": product.selected,
                "title": product.title.text(), "tags": product.tags.text(),
                "save_state": product.save_state, "reopened": product.reopened,
            })
        });
        publish(outcome.ok, &lines.join("\n"), &self.shared, saved);
        Ok(())
    }

    fn sheet(&self) -> &str {
        SHEET
    }

    fn snapshot(
        &self,
        ctx: &AppCtx<'_, TreePage, Logic, Child>,
        _: usize,
        _: f32,
    ) -> ProbeSnapshot {
        let page = ctx.runner.state();
        let focus = ctx.runner.focus();
        let focus_kind = focus.map_or("none", |node| {
            let dom = ctx.runner.dom();
            let dom = dom.borrow();
            if taproot::matching(&dom, &Selector::class("tree-canvas")).contains(&node) {
                "graph"
            } else if taproot::matching(&dom, &Selector::role("button")).contains(&node) {
                "button"
            } else {
                "other"
            }
        });
        let canvas = self.shared.canvas.borrow();
        let geometry = canvas.cartography_geometry();
        let mut positions: Vec<_> = geometry.iter().collect();
        positions.sort_by_key(|(id, _)| *id);
        let finite = positions
            .iter()
            .all(|(_, (x, y))| x.is_finite() && y.is_finite());
        let hash = positions
            .iter()
            .flat_map(|(_, (x, y))| {
                x.to_bits()
                    .to_le_bytes()
                    .into_iter()
                    .chain(y.to_bits().to_le_bytes())
            })
            .fold(0xcbf29ce484222325_u64, |hash, b| {
                (hash ^ u64::from(b)).wrapping_mul(0x100000001b3)
            });
        let drag_distance = canvas
            .dragging_node()
            .and_then(|id| canvas.graph().get_node_by_id(id))
            .and_then(|(key, _)| canvas.screen_position_of(key))
            .and_then(|(x, y)| {
                let (px, py) = self.pointer?;
                let dom = ctx.runner.dom();
                let leaf = taproot::matching(&dom.borrow(), &Selector::class("tree-canvas"))
                    .into_iter()
                    .next()?;
                let (left, top, _, _) = ctx.painted_rect(leaf)?;
                Some((left + x - px).hypot(top + y - py))
            });
        let step = canvas.elapsed_step_report().unwrap_or_default();
        let snapshot = self.remote_fields(ctx, self.physics_fields(ctx, ProbeSnapshot::default()));
        let snapshot = super::speed::fields(snapshot, &canvas, &self.shared);
        let snapshot = snapshot
            .with_field("physics-steps", step.steps.to_string())
            .with_field(
                "physics-dropped-us",
                step.discarded_elapsed.as_micros().to_string(),
            )
            .with_field(
                "physics-step-cap",
                self.shared.physics_config.max_steps.to_string(),
            )
            .with_field("focus", format!("{focus:?}"))
            .with_field("focus-kind", focus_kind)
            .with_field("physics-paused", canvas.physics_paused().to_string())
            // Arrangement roles (dynamics grammar plan, G7): the recipe's role
            // and how many settles Settled has recorded.
            .with_field("arrangement-role", canvas.arrangement_roles().default.id())
            .with_field("panel-role", page.physics.role().id())
            .with_field("settles", canvas.settle_count().to_string())
            .with_field("anchored-home", canvas.anchored_home_count().to_string())
            .with_field("physics-energy", canvas.physics_energy().to_string())
            .with_field(
                "finite",
                (finite && canvas.physics_energy().is_finite()).to_string(),
            )
            .with_field("geometry", format!("{hash:016x}"))
            .with_field("dragging", canvas.dragging_node().is_some().to_string())
            .with_field(
                "drag-distance",
                drag_distance
                    .map(|distance| format!("{distance:.3}"))
                    .unwrap_or_else(|| "unavailable".into()),
            )
            .with_field("zoom", canvas.camera().zoom.to_string())
            .with_field("camera", format!("{:?}", canvas.camera().offset))
            .with_field("picked", page.picked.clone().unwrap_or_default())
            .with_field("nodes", page.nodes.to_string())
            .with_field("source", page.source.clone())
            .with_field("moving", self.shared.moving.get().to_string())
            .with_field(
                "gpu-timed",
                self.shared.timing.borrow().gpu_timed().to_string(),
            );
        let faces = self.shared.faces.get().unwrap_or_default().fields();
        let snapshot = faces.into_iter().fold(snapshot, |snapshot, (name, value)| {
            snapshot.with_field(name, value)
        });
        if let Some(product) = &page.product {
            snapshot
                .with_field("graph-session", product.session.clone())
                .with_field("storage-reopened", product.reopened.to_string())
                .with_field("save-state", product.save_state)
                .with_field("detail-open", product.detail_open.to_string())
                .with_field(
                    "detail-member",
                    product
                        .selected
                        .map(|id| id.to_string())
                        .unwrap_or_default(),
                )
                .with_field("detail-title", product.title.text())
                .with_field("detail-tags", product.tags.text())
                .with_field(
                    "item-role",
                    product
                        .selected
                        .and_then(|member| canvas.member_role(member))
                        .map_or("recipe", |role| role.id()),
                )
        } else {
            snapshot
        }
    }

    /// `timing start <label>` and `timing stop`, and `click-node <url>`.
    fn app_step(
        &mut self,
        ctx: &mut AppCtx<'_, TreePage, Logic, Child>,
        _: mesquite::Checkpoints<'_>,
        line: &str,
    ) -> Result<(), String> {
        let (verb, rest) = line.split_once(' ').unwrap_or((line, ""));
        match verb {
            "timing" => {
                let (what, label) = rest.split_once(' ').unwrap_or((rest, ""));
                self.shared.with_gpu(|gpu| {
                    let mut timing = self.shared.timing.borrow_mut();
                    match what {
                        "start" => timing.start(label.trim(), gpu),
                        "stop" => timing.stop(gpu),
                        _ => Err(format!("timing wants start <label> or stop, got '{rest}'")),
                    }
                })
            },
            // `log-layout <label>`: room by mass now and where the law
            // started, into the receipt.
            "log-layout" => {
                let canvas = self.shared.canvas.borrow();
                let start = ctx.runner.state().physics.law_start;
                self.shared
                    .physics_log
                    .borrow_mut()
                    .push(graphshell::canvas_physics::layout_line(
                        rest.trim(),
                        &canvas,
                        start.as_ref(),
                    ));
                Ok(())
            },
            // `log-physics <label>`: the layout's signature and the GPU
            // lane's counts into the receipt, so a run can be read without
            // a failing assert to show the values.
            "log-physics" => {
                let canvas = self.shared.canvas.borrow();
                let stats = canvas.layout_stats_without_stretch();
                let lane = canvas.repulsion_stats().unwrap_or_default();
                let framing = graphshell::canvas_physics::framing_fields(&canvas)
                    .map(|(name, value)| format!("{name} {value}"))
                    .join(" ");
                let answers = self
                    .shared
                    .physics_device
                    .borrow()
                    .as_ref()
                    .map_or(0, |device| device.answers());
                self.shared.physics_log.borrow_mut().push(format!(
                    "physics {}: law {} nodes {} energy {:.1} spread {:.0} overlaps {} \
                     device {} device-steps {} cpu-steps {} submissions {} answers {} failures {} \
                     waiting {} stale {} mismatched {} last-age {} {framing}",
                    rest.trim(),
                    canvas.physics_law().id(),
                    canvas.graph().node_count(),
                    stats.energy,
                    stats.spread,
                    stats.overlaps,
                    if self.shared.physics_device.borrow().is_some() {
                        "on"
                    } else {
                        "off"
                    },
                    lane.device_steps,
                    lane.cpu_steps,
                    lane.submissions,
                    answers,
                    lane.failures,
                    lane.waiting,
                    lane.stale,
                    lane.mismatched,
                    lane.last_age,
                ));
                Ok(())
            },
            // `set-zoom <z>`: the zoom exactly, about the canvas centre.
            "set-zoom" => {
                let zoom = rest.trim().parse().map_err(|_| "set-zoom wants a number")?;
                let size = self.shared.size.get();
                CanvasCommand::SetZoom { zoom }.apply(&mut self.shared.canvas.borrow_mut(), size);
                self.shared.dirty.set(true);
                Ok(())
            },
            // `measure-faces <label>`: each face in view against its drawn
            // body, into the snapshot and the receipt.
            "measure-faces" => {
                let faces = graphshell::canvas_faces::FaceAlignment::measure(
                    &self.shared.canvas.borrow(),
                    self.shared.size.get(),
                );
                self.shared.faces.set(Some(faces));
                self.shared
                    .physics_log
                    .borrow_mut()
                    .push(faces.line(rest.trim()));
                Ok(())
            },
            "click-node" => {
                let (x, y) = self.node_point(ctx, rest.trim())?;
                ctx.pointer.push(HostPointer::Press(x, y));
                ctx.pointer.push(HostPointer::Release(x, y));
                Ok(())
            },
            "press-node" => {
                let point = self.node_point(ctx, rest.trim())?;
                self.pointer = Some(point);
                ctx.pointer.push(HostPointer::Press(point.0, point.1));
                Ok(())
            },
            "move-by" => {
                let args: Vec<f32> = rest
                    .split_whitespace()
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|_| "move-by wants dx dy")?;
                if args.len() != 2 {
                    return Err("move-by wants dx dy".into());
                }
                let point = self.pointer.as_mut().ok_or("move-by without a press")?;
                point.0 += args[0];
                point.1 += args[1];
                ctx.pointer.push(HostPointer::Moved(point.0, point.1));
                Ok(())
            },
            // The same move in world units, so a receipt's gesture means the
            // same distance to the law at any zoom; at zoom 1 it is `move-by`
            // (ruled 2026-10-04, "Measure in world units").
            "move-by-world" => {
                let args: Vec<f32> = rest
                    .split_whitespace()
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|_| "move-by-world wants dx dy")?;
                if args.len() != 2 {
                    return Err("move-by-world wants dx dy".into());
                }
                let (left, top, _, _) = leaf_rect(ctx).ok_or("the canvas leaf is not painted")?;
                let point = self.pointer.as_mut().ok_or("move-by-world without a press")?;
                let canvas = self.shared.canvas.borrow();
                let (wx, wy) = canvas.world_point_at((point.0 - left, point.1 - top));
                let (sx, sy) = canvas.screen_point_of((wx + args[0], wy + args[1]));
                *point = (sx + left, sy + top);
                ctx.pointer.push(HostPointer::Moved(point.0, point.1));
                Ok(())
            },
            "release-at" => {
                let point = self.pointer.take().ok_or("release-at without a press")?;
                self.drop = Some(point);
                // Watch for the first stepped frame after the release.
                let (left, top, _, _) = leaf_rect(ctx).ok_or("the canvas leaf is not painted")?;
                self.shared
                    .release_watch
                    .set(Some((point.0 - left, point.1 - top)));
                self.shared.release_step.set(None);
                let world = self
                    .shared
                    .canvas
                    .borrow()
                    .world_point_at((point.0 - left, point.1 - top));
                self.drop_world = Some(world);
                self.shared.release_watch_world.set(Some(world));
                self.shared.release_step_world.set(None);
                ctx.pointer.push(HostPointer::Release(point.0, point.1));
                Ok(())
            },
            // `reveal <role:name|.class> [text]`: scroll the first match into
            // view in its scrolling ancestors, as a click would, without
            // pressing it, so a capture can show a scrolled state.
            "reveal" => {
                let (head, text) = rest.trim().split_once(' ').unwrap_or((rest.trim(), ""));
                let mut selector = if let Some(role) = head.strip_prefix("role:") {
                    Selector::role(role)
                } else if let Some(class) = head.strip_prefix('.') {
                    Selector::class(class)
                } else {
                    return Err(format!("reveal wants role:name or .class, got '{head}'"));
                };
                if !text.trim().is_empty() {
                    selector = selector.containing(text.trim());
                }
                let node = {
                    let dom = ctx.runner.dom();
                    let dom = dom.borrow();
                    taproot::matching(&dom, &selector).into_iter().next()
                }
                .ok_or_else(|| format!("reveal {rest}: nothing matches"))?;
                ctx.scroll_into_view(node, cambium_rootstock::ScrollAlign::Nearest);
                Ok(())
            },
            // `center-node <url>`: bring a node into view by panning the camera
            // so it sits at the canvas centre. Zoom is kept, so screen-px
            // distances keep their meaning; the layout is untouched.
            "center-node" => {
                let url = rest.trim();
                let mut canvas = self.shared.canvas.borrow_mut();
                let (key, _) = canvas
                    .graph()
                    .get_node_by_url(url)
                    .ok_or_else(|| format!("center-node {url}: no such node"))?;
                let (x, y) = canvas
                    .screen_position_of(key)
                    .ok_or_else(|| format!("center-node {url}: no position"))?;
                let (width, height) = self.shared.size.get();
                let mut camera = canvas.camera();
                camera.offset.0 += width as f32 / 2.0 - x;
                camera.offset.1 += height as f32 / 2.0 - y;
                canvas.set_camera(camera);
                self.shared.dirty.set(true);
                Ok(())
            },
            // The drag gesture from the selected node: press where it is drawn.
            "press-focused" => {
                let point = self.focused_point(ctx)?;
                self.pointer = Some(point);
                if let Some((left, top, _, _)) = leaf_rect(ctx) {
                    self.shared
                        .press_point
                        .set(Some((point.0 - left, point.1 - top)));
                }
                ctx.pointer.push(HostPointer::Press(point.0, point.1));
                Ok(())
            },
            // `add-node <x> <y> <url>`: the empty-space add gesture at a
            // canvas-local point the receipt picks.
            "add-node" => {
                let mut words = rest.split_whitespace();
                let (Some(x), Some(y), Some(url), None) =
                    (words.next(), words.next(), words.next(), words.next())
                else {
                    return Err("add-node wants '<x> <y> <url>'".into());
                };
                let point = (
                    x.parse().map_err(|_| "add-node wants numeric x")?,
                    y.parse().map_err(|_| "add-node wants numeric y")?,
                );
                self.shared.canvas.borrow_mut().add_node_at(point, url);
                self.shared.dirty.set(true);
                Ok(())
            },
            _ => Err(format!("unknown verb: {line}")),
        }
    }

    /// The remote session's receipt events.
    fn drain_events(&mut self, _ctx: &mut mesquite::Ctx<'_, Self>) -> Vec<String> {
        std::mem::take(&mut self.shared.remote.borrow_mut().events)
    }

    /// Busy while a capture is pending, a remote answer is still to come, the
    /// local canvas moves while it is the one shown, or a timing window's GPU
    /// times are out ("Moving counts only when local is shown").
    fn busy_mut(
        &mut self,
        ctx: &mut AppCtx<'_, TreePage, Logic, Child>,
        capture_pending: bool,
    ) -> Option<bool> {
        let pending = self.shared.timing.borrow_mut().poll();
        let local_shown = ctx.runner.state().session == remote::Session::Local;
        Some(
            capture_pending
                || pending
                || self.shared.remote.borrow().in_flight()
                || (local_shown && self.shared.moving.get())
                || ctx
                    .runner
                    .state()
                    .product
                    .as_ref()
                    .is_some_and(|product| product.saving),
        )
    }

    fn receipt_lines(&self) -> Vec<String> {
        let timing = self.shared.timing.borrow();
        let mut lines = timing.receipt_lines();
        lines.extend(self.shared.release_log.borrow().iter().cloned());
        lines.extend(self.shared.physics_log.borrow().iter().cloned());
        lines.push(format!(
            "gpu timestamps: {}",
            if timing.gpu_timed() { "yes" } else { "no" }
        ));
        lines
    }

    fn receipt_checks(&self, _captures: &[CaptureRecord]) -> Vec<String> {
        let mut errors = self.errors.clone();
        if self.shared.timing.borrow().any_hidden() {
            errors.push("the page was hidden while a timing window was open".to_string());
        }
        errors
    }
}
