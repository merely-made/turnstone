// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! One graph scene source shared by presentation and portable export.
use super::*;
use cambium_rootstock::{
    PortableProducerError, ProducedScene, ProducerFrameInfo, ProducerFramePurpose,
};

impl CanvasProducer {
    pub(super) fn produce(
        &mut self,
        frame: &ProducerFrameInfo,
    ) -> Result<ProducedScene, PortableProducerError> {
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
        let profile = shared.timing.borrow().active();
        if shared.remote_shown.get() {
            // One leaf, the producer picks the scene: the board, mirroring
            // the canvas's law and speed, ticked once a frame and drawn from
            // its bodies.
            let stage = canvas.live_stage_spec().ok();
            let speed = canvas.physics_speed();
            drop(canvas);
            let scene = {
                let mut remote = shared.remote.borrow_mut();
                remote.sync_board(stage, speed);
                remote.board.set_step_budget(Some(budget));
                remote.board.tick();
                let remote = &mut *remote;
                remote
                    .text
                    .set_font_roles(&shared.appearance.borrow().font_roles());
                let empty = mere::canvas::BoardScene::default();
                let painted = if remote.mounted().is_some() {
                    remote.board.scene()
                } else {
                    &empty
                };
                painted.paint_titled(
                    remote.board.board(),
                    size.0,
                    size.1,
                    remote::BOARD_FIT,
                    &mut remote.text,
                )
            };
            shared.dirty.set(false);
            shared.physical_size.set(frame.physical_size);
            return self.remember_scene(scene);
        }
        let (scene, moving) = if profile {
            let (scene, moving, sample) = match frame.timestamp {
                Some(timestamp) => {
                    canvas.frame_profiled_at(size.0, size.1, timestamp, physics_config, now_ms)
                },
                None => canvas.frame_profiled(size.0, size.1, now_ms),
            };
            shared.timing.borrow_mut().stages(
                mere::canvas::CanvasFrameProfile::STAGES
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
                mere::canvas::PhysicsChoice::live(&canvas).law.id(),
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
