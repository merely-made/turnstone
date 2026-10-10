// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Graph controls use the same canvas operations as the existing product page.
use super::*;
use cambium::{PointerButton, PointerEvent, PointerPhase, button, focusable_if, on_click};

pub(super) fn toolbar(page: &TreePage) -> Child {
    let paused = page.shared.canvas.borrow().physics_paused();
    let can_fit_selection = page.shared.canvas.borrow().can_fit_selection();
    let hosted = matches!(page.dataset, HostedDataset::Loaded(_));
    let commands = [
        (
            "Pan left",
            CanvasCommand::Pan {
                dx: -PAN_STEP,
                dy: 0.0,
            },
        ),
        (
            "Pan right",
            CanvasCommand::Pan {
                dx: PAN_STEP,
                dy: 0.0,
            },
        ),
        (
            "Pan up",
            CanvasCommand::Pan {
                dx: 0.0,
                dy: -PAN_STEP,
            },
        ),
        (
            "Pan down",
            CanvasCommand::Pan {
                dx: 0.0,
                dy: PAN_STEP,
            },
        ),
        ("Zoom in", CanvasCommand::Zoom { delta: ZOOM_STEP }),
        ("Zoom out", CanvasCommand::Zoom { delta: -ZOOM_STEP }),
        if hosted {
            ("Fit visible graph", CanvasCommand::FitVisible)
        } else {
            ("Fit graph", CanvasCommand::Fit)
        },
        ("Fit selection", CanvasCommand::FitSelection),
        ("Restore arrangement", CanvasCommand::RestoreArrangement),
        (
            if paused {
                "Play physics"
            } else {
                "Pause physics"
            },
            CanvasCommand::TogglePhysics,
        ),
    ];
    let mut buttons: Vec<Child> = commands
        .into_iter()
        .map(|(label, command)| {
            let enabled = command != CanvasCommand::FitSelection || can_fit_selection;
            let mut control = focusable_if(
                on_click(
                    el("button", label.to_owned()),
                    move |page: &mut TreePage, _| {
                        if let CanvasCommand::Pan { dx, dy } = command {
                            page.shared.pan(dx, dy);
                            return;
                        }
                        let mut canvas = page.shared.canvas.borrow_mut();
                        if command == CanvasCommand::FitSelection && !canvas.can_fit_selection() {
                            return;
                        }
                        command.apply(&mut canvas, page.shared.size.get());
                        page.shared.dirty.set(true);
                    },
                ),
                enabled,
            );
            if !enabled {
                control = control.attr("disabled", "").attr("aria-disabled", "true");
            }
            Box::new(control) as Child
        })
        .collect();
    // Collapsed, the Graph tools region opens over the canvas from here.
    if !tools_docked(page) {
        buttons.push(Box::new(
            button("Graph tools", |page: &mut TreePage, _| {
                page.tools_open = !page.tools_open;
            })
            .attr(
                "aria-expanded",
                if page.tools_open { "true" } else { "false" },
            ),
        ));
    }
    Box::new(
        el("nav", buttons)
            .attr("class", "tree-controls")
            .attr("aria-label", "Graph controls"),
    )
}

impl TreePage {
    pub(super) fn pointer(&mut self, event: PointerEvent) {
        if self
            .product
            .as_ref()
            .is_some_and(|product| product.selection_locked())
        {
            return;
        }
        if event.button != PointerButton::Primary {
            return;
        }
        let (x, y) = event.local;
        let mut canvas = self.shared.canvas.borrow_mut();
        match event.phase {
            PointerPhase::Down => {
                canvas.pointer_down(mere::canvas::PointerButton::Left, x, y);
            },
            PointerPhase::Move => {
                canvas.cursor_moved(x, y);
            },
            PointerPhase::Up => {
                canvas.pointer_up(mere::canvas::PointerButton::Left, x, y);
                self.picked = canvas.focused_url().map(str::to_owned);
                if let Some(product) = &mut self.product {
                    product.select(canvas.selected_members().first().copied());
                }
            },
        }
        self.shared.dirty.set(true);
    }
}

/// Probe-page configuration; the Canvas API accepts the same settings directly.
/// The web host's GPU repulsion threshold: the node count at or above
/// which the canvas stages `NodeExclusion` on the page's device, this host's
/// measured crossover (physics stage 4.5 ms CPU against 1.5 on the device at
/// 512 nodes, 1.1 against 3.1 at 256; ruled 2026-10-02, "Web N = 9, web
/// threshold 400"). `gpu_threshold` overrides.
pub(super) const WEB_GPU_THRESHOLD: usize = 400;
/// How many steps old a device answer may be on this host. Chrome answers a
/// readback about two frames after the submit, six steps at three a frame,
/// so 9 covers it with a frame to spare (same ruling). `gpu_max_stale_steps`
/// overrides.
pub(super) const WEB_GPU_MAX_STALE_STEPS: u32 = 9;

/// The page's GPU repulsion options: `gpu=off` keeps the CPU law (the
/// baseline a receipt compares against), `gpu_threshold` and
/// `gpu_max_stale_steps` override the host defaults above.
#[derive(Clone, Copy, Debug)]
pub(super) struct GpuOptions {
    #[cfg_attr(not(feature = "canvas-gpu"), allow(dead_code))]
    pub(super) enabled: bool,
    pub(super) threshold: usize,
    pub(super) max_stale_steps: u32,
}

pub(super) fn gpu_options() -> Result<GpuOptions, String> {
    let search = web_sys::window()
        .ok_or("no window")?
        .location()
        .search()
        .map_err(|_| "cannot read page options")?;
    let params =
        web_sys::UrlSearchParams::new_with_str(&search).map_err(|_| "invalid page options")?;
    let mut options = GpuOptions {
        enabled: params.get("gpu").as_deref() != Some("off"),
        threshold: WEB_GPU_THRESHOLD,
        max_stale_steps: WEB_GPU_MAX_STALE_STEPS,
    };
    if let Some(value) = params.get("gpu_threshold") {
        options.threshold = value.parse().map_err(|_| "invalid gpu_threshold")?;
    }
    if let Some(value) = params.get("gpu_max_stale_steps") {
        options.max_stale_steps = value.parse().map_err(|_| "invalid gpu_max_stale_steps")?;
    }
    Ok(options)
}

/// The page's Meaning slice (dynamics grammar plan, G2, F35): `meaning_slice=0`
/// runs a Meaning run whole on the frame it starts, `meaning_slice=<n>` spends
/// at most `n` pair scores a frame; absent, the canvas's wasm default holds.
pub(super) fn meaning_slice() -> Result<Option<Option<usize>>, String> {
    let search = web_sys::window()
        .ok_or("no window")?
        .location()
        .search()
        .map_err(|_| "cannot read page options")?;
    let params =
        web_sys::UrlSearchParams::new_with_str(&search).map_err(|_| "invalid page options")?;
    let Some(value) = params.get("meaning_slice") else {
        return Ok(None);
    };
    let scores: usize = value.parse().map_err(|_| "invalid meaning_slice")?;
    Ok(Some((scores > 0).then_some(scores)))
}

/// A planted accessibility defect from `?plant_a11y=` (`missing_item`,
/// `missing_action`, `dead_action`): the receipts' positive control.
pub(super) fn reader_plant() -> Result<graphshell::canvas_reader::Plant, String> {
    let search = web_sys::window()
        .ok_or("no window")?
        .location()
        .search()
        .map_err(|_| "cannot read page options")?;
    let params =
        web_sys::UrlSearchParams::new_with_str(&search).map_err(|_| "invalid page options")?;
    match params.get("plant_a11y") {
        None => Ok(graphshell::canvas_reader::Plant::None),
        Some(value) => graphshell::canvas_reader::Plant::parse(&value)
            .ok_or_else(|| format!("invalid plant_a11y {value}")),
    }
}

pub(super) fn physics_config() -> Result<mere::canvas::ElapsedStepConfig, String> {
    let mut config = mere::canvas::ElapsedStepConfig::default();
    let search = web_sys::window()
        .ok_or("no window")?
        .location()
        .search()
        .map_err(|_| "cannot read page options")?;
    let params =
        web_sys::UrlSearchParams::new_with_str(&search).map_err(|_| "invalid page options")?;
    if let Some(value) = params.get("physics_max_steps") {
        config.max_steps = value.parse().map_err(|_| "invalid physics_max_steps")?;
    }
    if let Some(value) = params.get("physics_max_elapsed_ms") {
        config.max_elapsed = std::time::Duration::from_millis(
            value
                .parse()
                .map_err(|_| "invalid physics_max_elapsed_ms")?,
        );
    }
    Ok(config)
}
