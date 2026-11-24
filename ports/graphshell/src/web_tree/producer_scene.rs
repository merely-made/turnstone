// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! One graph scene source shared by presentation and portable export.
use super::*;
use cambium_rootstock::{
    PortableProducerError, PortableSceneUpdate, ProducedScene, ProducerFrameInfo,
    ProducerFramePurpose,
};

#[cfg(feature = "remote")]
pub(super) struct RemoteVisual {
    position: remote::BoardPosition,
    viewport: BoardViewport,
    pub(super) geometry: Option<cambium_rootstock::ProducerSemanticGeometry>,
}

#[cfg(feature = "remote")]
fn board_geometry(
    remote: &remote::TreeRemote,
    viewport: &BoardViewport,
) -> Option<cambium_rootstock::ProducerSemanticGeometry> {
    let (revision, transform) =
        remote
            .board
            .semantic_geometry(viewport.size.0, viewport.size.1, viewport.fit)?;
    Some(cambium_rootstock::ProducerSemanticGeometry {
        revision,
        scale: f64::from(transform.scale),
        origin: [f64::from(transform.origin.0), f64::from(transform.origin.1)],
    })
}

#[cfg(feature = "remote")]
#[derive(PartialEq)]
struct BoardViewport {
    size: (u32, u32),
    physical_size: [u32; 2],
    scale: f32,
    fit: cartography::canvas::BoardFit,
    font_roles: String,
}

impl CanvasProducer {
    pub(super) fn produce(
        &mut self,
        frame: &ProducerFrameInfo,
    ) -> Result<ProducedScene, PortableProducerError> {
        match self.produce_update(frame)? {
            PortableSceneUpdate::Produced(scene) => Ok(scene),
            PortableSceneUpdate::Retain { .. } => self
                .scene
                .clone()
                .ok_or(PortableProducerError::InvalidSource),
        }
    }

    pub(super) fn produce_update(
        &mut self,
        frame: &ProducerFrameInfo,
    ) -> Result<PortableSceneUpdate, PortableProducerError> {
        if frame.purpose == ProducerFramePurpose::Export {
            let size = (
                frame.logical_size.0.round().max(1.0) as u32,
                frame.logical_size.1.round().max(1.0) as u32,
            );
            if size != self.shared.size.get() || self.shared.dirty.get() {
                return Err(PortableProducerError::InvalidSource);
            }
            return self
                .scene
                .clone()
                .map(PortableSceneUpdate::Produced)
                .ok_or(PortableProducerError::InvalidSource);
        }
        let owner = self.shared.clone();
        let shared = &owner;
        let visibility_before = visibility::before(shared);
        let (width, height) = frame.logical_size;
        let size = (
            width.round().max(1.0) as u32,
            height.round().max(1.0) as u32,
        );
        let mut canvas = shared.canvas.borrow_mut();
        if size != shared.size.get() {
            canvas.resize(size.0, size.1);
            shared.size.set(size);
        }
        if shared.fit_pending.replace(false) {
            canvas.fit_to_content();
        }
        // The step budget is a share of the display's period, read from the
        // frames' intervals (ruled 2026-10-04, "Infer the period").
        let frame_ms = frame
            .timestamp
            .map_or_else(now_ms, |timestamp| timestamp.as_secs_f64() * 1000.0);
        shared
            .period_worker
            .feed(&mut shared.frame_budget.borrow_mut(), frame_ms);
        let budget = shared.frame_budget.borrow_mut().frame(frame_ms);
        canvas.set_physics_step_budget(Some(budget));
        crate::web_speed::plant_max_frame(&canvas, shared.speed.plant_max_frame);
        let physics_config = crate::web_speed::planted_owed(
            shared.physics_config,
            canvas.physics_speed(),
            std::time::Duration::from_secs_f64(
                shared.frame_budget.borrow().last_interval_ms() / 1000.0,
            ),
            shared.speed.plant_owed,
        );
        #[cfg(feature = "content-backdrop")]
        canvas.set_content_backdrop_device_scale(frame.layout_scale);
        let profile = shared.timing.borrow().active();
        #[cfg(feature = "remote")]
        if shared.remote_is_shown() {
            // One leaf, the producer picks the scene: the board, mirroring
            // the canvas's law and speed, ticked once a frame and drawn from
            // its bodies.
            let stage = canvas.live_stage_spec().ok();
            let speed = canvas.physics_speed();
            drop(canvas);
            let (scene, semantic_labels) = {
                let mut remote = shared.remote.borrow_mut();
                let previous = self.remote_visual.take();
                #[cfg(feature = "remote-cards-receipt")]
                let start = now_ms();
                let settling = remote.board.is_settling();
                let (position, unchanged, labels) = remote.sync_board(
                    stage,
                    speed,
                    size,
                    previous.as_ref().map(|visual| &visual.position),
                );
                #[cfg(feature = "remote-cards-receipt")]
                remote::cards_receipt::record(|sample| sample.board_sync_ms += now_ms() - start);
                #[cfg(feature = "remote-cards-receipt")]
                let start = now_ms();
                remote.board.set_step_budget(Some(budget));
                let moving = remote.board.tick();
                #[cfg(feature = "remote-cards-receipt")]
                remote::cards_receipt::record(|sample| sample.board_tick_ms += now_ms() - start);
                let viewport = BoardViewport {
                    size,
                    physical_size: frame.physical_size,
                    scale: frame.layout_scale,
                    fit: remote.board_fit(size),
                    font_roles: shared.appearance.borrow().font_roles(),
                };
                let stationary = !settling && !moving;
                let stable = stationary
                    && previous
                        .as_ref()
                        .is_some_and(|previous| previous.viewport == viewport)
                    && !shared.dirty.get()
                    && !frame.needs_frame;
                let reuse = unchanged && stable;
                if reuse
                    && self.scene.is_some()
                    && let Some(position) = position
                {
                    let geometry = board_geometry(&remote, &viewport);
                    self.remote_visual = Some(RemoteVisual {
                        position,
                        viewport,
                        geometry,
                    });
                    if let Some(labels) = labels {
                        self.advance_board_semantics(Some(labels));
                    }
                    let output = self.scene.as_ref().expect("checked above");
                    #[cfg(feature = "remote-cards-receipt")]
                    {
                        remote::cards_receipt::record_retained_paint();
                        remote::cards_receipt::record(|sample| {
                            sample.canvas_revision = output.source_revision;
                            sample.canvas_reused = true;
                        });
                    }
                    return Ok(PortableSceneUpdate::Retain {
                        source_identity: output.source_identity,
                        source_revision: output.source_revision,
                    });
                }
                let remote = &mut *remote;
                remote.text.set_font_roles(&viewport.font_roles);
                let empty = cartography::canvas::BoardScene::default();
                let painted = if remote.mounted().is_some() {
                    remote.board.scene()
                } else {
                    &empty
                };
                #[cfg(feature = "remote-cards-receipt")]
                let start = now_ms();
                let result = painted.paint_titled(
                    remote.board.board(),
                    size.0,
                    size.1,
                    viewport.fit,
                    &mut remote.text,
                );
                #[cfg(feature = "remote-cards-receipt")]
                {
                    remote::cards_receipt::record(|sample| {
                        sample.board_paint_ms += now_ms() - start
                    });
                    if let Ok(scene) = &result {
                        remote::cards_receipt::record_paint(scene);
                    }
                }
                if result.is_ok()
                    && stationary
                    && let Some(position) = position
                {
                    let geometry = board_geometry(remote, &viewport);
                    self.remote_visual = Some(RemoteVisual {
                        position,
                        viewport,
                        geometry,
                    });
                }
                (result, labels.filter(|_| stable))
            };
            let scene = match scene {
                Ok(scene) => scene,
                Err(_) => {
                    self.scene = None;
                    shared.dirty.set(true);
                    return Err(PortableProducerError::InvalidSource);
                },
            };
            shared.dirty.set(false);
            shared.physical_size.set(frame.physical_size);
            self.advance_board_semantics(semantic_labels);
            let output = self.remember_scene(scene)?;
            #[cfg(feature = "remote-cards-receipt")]
            remote::cards_receipt::record(|sample| sample.canvas_revision = output.source_revision);
            return Ok(PortableSceneUpdate::Produced(output));
        }
        #[cfg(feature = "remote")]
        {
            self.remote_visual = None;
        }
        let (scene, moving) = if profile {
            let (scene, moving, sample) = match frame.timestamp {
                Some(timestamp) => {
                    canvas.frame_profiled_at(size.0, size.1, timestamp, physics_config, now_ms)
                },
                None => canvas.frame_profiled(size.0, size.1, now_ms),
            };
            shared.timing.borrow_mut().stages(
                cartography::canvas::CanvasFrameProfile::STAGES
                    .into_iter()
                    .zip(sample.stage_ms),
            );
            shared.timing.borrow_mut().counts([
                ("total_nodes", sample.total_nodes),
                ("visible_nodes", sample.visible_nodes),
                ("paint_before_cull", sample.paint_commands_before_cull),
                ("paint_after_cull", sample.paint_commands_after_cull),
            ]);
            (scene, moving)
        } else {
            match frame.timestamp {
                Some(timestamp) => canvas.frame_at(size.0, size.1, timestamp, physics_config),
                None => canvas.frame(size.0, size.1),
            }
        };
        if canvas.last_translation_error().is_some() {
            self.scene = None;
            shared.dirty.set(true);
            return Err(PortableProducerError::InvalidSource);
        }
        if profile && let Some(report) = canvas.elapsed_step_report() {
            shared.timing.borrow_mut().counts([
                ("physics_steps", report.steps as usize),
                (
                    "physics_discarded_us",
                    report.discarded_elapsed.as_micros().min(usize::MAX as u128) as usize,
                ),
                (
                    "physics_carried_us",
                    report.carried_elapsed.as_micros() as usize,
                ),
            ]);
        }
        shared.moving.set(moving);
        speed::record(shared, &canvas, moving, budget.per_frame);
        if let Some((x, y)) = shared.release_watch.get()
            && frame.timestamp.is_some()
            && canvas.dragging_node().is_none()
            && let Some(report) = canvas.elapsed_step_report()
            && report.steps > 0
        {
            let focused = canvas.focused_screen_position();
            let distance = focused.map_or(f32::NAN, |(fx, fy)| (fx - x).hypot(fy - y));
            let from_press = focused
                .zip(shared.press_point.get())
                .map_or(f32::NAN, |((fx, fy), (px, py))| (fx - px).hypot(fy - py));
            shared.release_step.set(Some((distance, report.steps)));
            shared.release_step_world.set(
                canvas
                    .focused_world_position()
                    .zip(shared.release_watch_world.get())
                    .map(|((fx, fy), (wx, wy))| (fx - wx).hypot(fy - wy)),
            );
            shared.release_log.borrow_mut().push(format!(
                "release-step {} {distance:.1} px after {} steps ({from_press:.1} px from the press point, zoom {:.2})",
                cartography::canvas::PhysicsChoice::live(&canvas).law.id(),
                report.steps,
                canvas.camera().zoom
            ));
            shared.release_watch.set(None);
        }
        visibility::after(shared, &canvas, visibility_before, frame.timestamp);
        shared.dirty.set(false);
        shared.physical_size.set(frame.physical_size);
        drop(canvas);
        self.remember_scene(scene)
            .map(PortableSceneUpdate::Produced)
    }

    #[cfg(feature = "remote")]
    fn advance_board_semantics(&mut self, labels: Option<Vec<(u64, String)>>) {
        let base = self.remote_semantic_revision;
        self.remote_semantic_revision = base.saturating_add(1);
        self.remote_semantic_labels = labels.map(|labels| (base, labels));
    }

    fn remember_scene(
        &mut self,
        scene: netrender::Scene,
    ) -> Result<ProducedScene, PortableProducerError> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(PortableProducerError::InvalidSource)?;
        let output = ProducedScene {
            scene,
            source_identity: CANVAS_RASTER,
            source_revision: self.generation,
            alpha: SourceAlpha::Straight,
            encoding: SourceEncoding::Srgb,
        };
        self.scene = Some(output.clone());
        Ok(output)
    }
}
