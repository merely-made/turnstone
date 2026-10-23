// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Graph controls use the same canvas operations as the existing product page.
use super::*;
use cambium::{PointerButton, PointerEvent, PointerPhase, button, focusable_if, on_click};

pub(super) fn toolbar(page: &TreePage) -> Child {
    let resident = page.resident_mere_shown();
    let paused = page.shared.canvas.borrow().physics_paused();
    let can_fit_selection = page.shared.canvas.borrow().can_fit_selection();
    let has_selection = !page.shared.canvas.borrow().selected_members().is_empty();
    let hosted = matches!(page.dataset, HostedDataset::Loaded(_));
    let commands = [
        CanvasCommand::Pan {
            dx: -PAN_STEP,
            dy: 0.0,
        },
        CanvasCommand::Pan {
            dx: PAN_STEP,
            dy: 0.0,
        },
        CanvasCommand::Pan {
            dx: 0.0,
            dy: -PAN_STEP,
        },
        CanvasCommand::Pan {
            dx: 0.0,
            dy: PAN_STEP,
        },
        CanvasCommand::Zoom { delta: ZOOM_STEP },
        CanvasCommand::Zoom { delta: -ZOOM_STEP },
        if hosted {
            CanvasCommand::FitVisible
        } else {
            CanvasCommand::Fit
        },
        CanvasCommand::FitSelection,
        CanvasCommand::RestoreArrangement,
        CanvasCommand::ResetToSeed,
        CanvasCommand::SetSeed,
        CanvasCommand::ToggleForegroundPin,
        CanvasCommand::DismissRetained,
        CanvasCommand::GrowTiles,
        CanvasCommand::ShrinkTiles,
        CanvasCommand::ToggleTerritory,
        CanvasCommand::ToggleSuggestions,
        CanvasCommand::TogglePhysics,
    ];
    let mut buttons: Vec<Child> = commands
        .into_iter()
        .filter(|_| !resident)
        .map(|command| {
            let label = if command == CanvasCommand::TogglePhysics {
                CanvasCommand::SetPhysicsPaused(!paused).label()
            } else {
                command.label()
            };
            let enabled = match command {
                CanvasCommand::FitSelection => can_fit_selection,
                CanvasCommand::ToggleForegroundPin
                | CanvasCommand::GrowTiles
                | CanvasCommand::ShrinkTiles => has_selection,
                _ => true,
            };
            let mut control = focusable_if(
                on_click(
                    el("button", label.to_owned()),
                    move |page: &mut TreePage, _| {
                        if page.resident_mere_shown() {
                            return;
                        }
                        if let CanvasCommand::Pan { dx, dy } = command {
                            page.shared.pan(dx, dy);
                            return;
                        }
                        let mut canvas = page.shared.canvas.borrow_mut();
                        if command == CanvasCommand::FitSelection && !canvas.can_fit_selection() {
                            return;
                        }
                        command.apply(&mut *canvas, page.shared.size.get());
                        crate::web_suggestions::after_command(command, &canvas);
                        page.shared.dirty.set(true);
                    },
                ),
                enabled,
            );
            if !enabled {
                control = control.attr("disabled", "").attr("aria-disabled", "true");
            }
            if matches!(command, CanvasCommand::TogglePhysics) {
                control = control.attr("data-command", "physics:toggle");
            }
            Box::new(control) as Child
        })
        .collect();
    if resident {
        buttons.push(Box::new(
            status_note("", "Resident graph · use the Mere panel to choose a layout or read a record"),
        ));
    } else {
        buttons.push(Box::new(
            button("Graph commands", |page: &mut TreePage, _| {
                commands::open_keyboard(page)
            })
            .attr("aria-haspopup", "dialog")
            .attr(
                "aria-expanded",
                if page.commands.open.is_some()
                    || page
                        .product
                        .as_ref()
                        .is_some_and(|product| product.menu_open())
                {
                    "true"
                } else {
                    "false"
                },
            )
            .attr("aria-keyshortcuts", "Control+k Meta+k Shift+F10"),
        ));
    }
    #[cfg(feature = "product")]
    if !resident && !hosted {
        let canvas = page.shared.canvas.borrow();
        let local_ready = page.product.as_ref().is_some_and(|p| !p.selection_locked());
        for (label, show, enabled) in [
            (
                "Hide selected link",
                false,
                local_ready && !canvas.selected_link_keys().is_empty(),
            ),
            (
                "Show hidden links",
                true,
                local_ready && !canvas.hidden_link_keys().is_empty(),
            ),
        ] {
            let mut control = focusable_if(
                on_click(el("button", label), move |page: &mut TreePage, _| {
                    if page.resident_mere_shown()
                        || matches!(page.dataset, HostedDataset::Loaded(_))
                    {
                        return;
                    }
                    if let Some(product) = &mut page.product {
                        let canvas = page.shared.canvas.borrow();
                        let result = if show {
                            product.show_hidden_links(&canvas)
                        } else {
                            product.hide_selected_links(&canvas)
                        };
                        if let Err(error) = result {
                            product.status = error;
                        }
                        page.shared.dirty.set(true);
                    }
                }),
                enabled,
            )
            .attr("data-link-visibility", if show { "show" } else { "hide" });
            if !enabled {
                control = control.attr("disabled", "").attr("aria-disabled", "true");
            }
            buttons.push(Box::new(control));
        }
    }
    #[cfg(feature = "product")]
    if let Some(product) = &page.product {
        let (state, message) = product.link_visibility_state();
        if state != "idle" {
            buttons.push(Box::new(
                el("span", message.to_owned())
                    .attr("role", "status")
                    .attr("data-link-visibility-status", state),
            ));
        }
    }
    #[cfg(feature = "product")]
    if page.product.is_some() {
        buttons.push(blocks::control(page));
    }
    // Collapsed, the Graph tools region opens over the canvas from here.
    {
        buttons.push(Box::new(
            disclosure_trigger("Graph tools", page.tools_open, |page: &mut TreePage| {
                page.tools_open = !page.tools_open;
            }),
        ));
    }
    Box::new(
        el("nav", buttons)
            .attr("class", "tree-controls workspace-controls")
            .attr(
                "aria-label",
                if resident {
                    "Resident graph controls"
                } else {
                    "Graph controls"
                },
            ),
    )
}

impl TreePage {
    pub(super) fn pointer(&mut self, event: PointerEvent) {
        // A cancellation always releases the gesture, including while a save
        // temporarily locks selection.
        if event.phase == PointerPhase::Cancel {
            #[cfg(feature = "product")]
            {
                forme::pointer(self, &event);
                field_inspection::pointer(self, &event);
            }
            self.shared.canvas.borrow_mut().pointer_cancel();
            self.shared.dirty.set(true);
            return;
        }
        if self.resident_mere_shown() {
            return;
        }
        #[cfg(feature = "product")]
        if field_inspection::pointer(self, &event) {
            event.prop.prevent_default();
            return;
        }
        if self
            .product
            .as_ref()
            .is_some_and(|product| product.selection_locked())
        {
            return;
        }
        if event.button == PointerButton::Primary
            && event.phase == PointerPhase::Down
            && (self.commands.open.is_some()
                || self
                    .product
                    .as_ref()
                    .is_some_and(|product| product.menu_open()))
        {
            commands::close(self);
        }
        #[cfg(feature = "product")]
        if event.button == PointerButton::Primary && forme::pointer(self, &event) {
            event.prop.prevent_default();
            return;
        }
        let button = match event.button {
            PointerButton::Primary => mere::canvas::PointerButton::Left,
            PointerButton::Secondary => mere::canvas::PointerButton::Right,
            PointerButton::Auxiliary => mere::canvas::PointerButton::Middle,
        };
        let (x, y) = event.local;
        let mut canvas = self.shared.canvas.borrow_mut();
        canvas.set_ctrl(event.modifiers.ctrl);
        canvas.set_shift(event.modifiers.shift);
        canvas.set_alt(event.modifiers.alt);
        let mut context = None;
        #[cfg(feature = "product")]
        let mut opened_pair = None;
        match event.phase {
            PointerPhase::Down => {
                // Canvas right-drag is a marquee; shared menu handlers retain
                // their default one-shot secondary press behavior.
                if event.button == PointerButton::Secondary {
                    event.capture_pointer();
                }
                canvas.pointer_down(button, x, y);
            },
            PointerPhase::Move => {
                canvas.cursor_moved(x, y);
            },
            PointerPhase::Up => {
                canvas.pointer_up(button, x, y);
                #[cfg(feature = "product")]
                if event.button == PointerButton::Primary {
                    let keys = canvas.selected_link_keys();
                    if keys.len() == 1 {
                        opened_pair = canvas.link_appearance_pair(&keys[0]);
                    }
                }
                context = canvas.take_context_request();
                self.picked = canvas.focused_url().map(str::to_owned);
                if let Some(product) = &mut self.product {
                    product.select(canvas.selected_members().first().copied());
                }
            },
            PointerPhase::Cancel => unreachable!("cancellation handled before selection lock"),
        }
        // Modifier state belongs to this event, not later toolbar commands.
        canvas.set_ctrl(false);
        canvas.set_shift(false);
        canvas.set_alt(false);
        drop(canvas);
        #[cfg(feature = "product")]
        if let Some((first, second)) = opened_pair {
            let target = graphshell::link_inspection::LinkTarget::new(first, second);
            if self.inspected_link != Some(target) {
                self.opened_link_layout = None;
                self.opened_link_error = None;
                self.inspected_statement = None;
                self.inspected_link_member = None;
            }
            self.inspected_link = Some(target);
            self.tools_open = true;
        }
        if let Some(request) = context {
            commands::open(
                self,
                commands::Request::Context {
                    at: request.at,
                    node: request.node,
                },
            );
        } else if event.phase == PointerPhase::Up && event.button == PointerButton::Secondary {
            commands::open(self, commands::Request::Selection);
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

pub(super) fn named_command(id: &str) -> Option<CanvasCommand> {
    graphshell::viewer_commands::scenario_canvas_command(id)
}
