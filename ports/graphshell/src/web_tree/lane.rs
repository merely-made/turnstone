// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Product observations and actions for Mesquite on the tree page.
use super::*;

/// The tree page's half of the scenario lane.
pub(super) struct TreeLane {
    pub(super) shared: Rc<Shared>,
    pub(super) errors: Vec<String>,
    pub(super) pointer: Option<(f32, f32)>,
}

impl TreeLane {
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
        _: &mut mesquite::Ctx<'_, Self>,
        outcome: &taproot::Outcome,
    ) -> Result<(), String> {
        let mut lines = vec![format!("RESULT {}", if outcome.ok { "ok" } else { "fail" })];
        lines.extend(outcome.log.iter().cloned());
        lines.extend(self.receipt_lines());
        publish(outcome.ok, &lines.join("\n"), &self.shared);
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
        ProbeSnapshot::default()
            .with_field("focus", format!("{focus:?}"))
            .with_field("focus-kind", focus_kind)
            .with_field("physics-paused", canvas.physics_paused().to_string())
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
            )
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
            "release-at" => {
                let point = self.pointer.take().ok_or("release-at without a press")?;
                ctx.pointer.push(HostPointer::Release(point.0, point.1));
                Ok(())
            },
            _ => Err(format!("unknown verb: {line}")),
        }
    }

    /// Busy while the layout moves or a timing window's GPU times are out, so
    /// `wait` holds for a settled frame and a finished report.
    fn busy_mut(
        &mut self,
        _ctx: &mut AppCtx<'_, TreePage, Logic, Child>,
        capture_pending: bool,
    ) -> Option<bool> {
        let pending = self.shared.timing.borrow_mut().poll();
        Some(capture_pending || pending || self.shared.moving.get())
    }

    fn receipt_lines(&self) -> Vec<String> {
        let timing = self.shared.timing.borrow();
        let mut lines = timing.receipt_lines();
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
