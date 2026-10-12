// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Explicitly armed acceptance control for this application's owned device.
//! It retains copied authority and weak identities, never old GPU resources.
use super::Context;
use cambium_rootstock::ProducerImageIdentity;
use genet_render_host::{DeviceLossNotice, RecoveryStatus, RenderCore, SharedRenderCore};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{Arc, Weak},
};
use tabard_workshop::{READER_LEAF_KEY, ReaderSpecimen, STYLESHEET_LEAF_KEY, StylesheetSpecimen};

struct BeforeLoss {
    generation: u64,
    core: Weak<RenderCore>,
    owner: Weak<SharedRenderCore>,
    reader: std::rc::Weak<RefCell<ReaderSpecimen>>,
    stylesheet: std::rc::Weak<RefCell<StylesheetSpecimen>>,
    runner: usize,
    authority: BTreeMap<String, String>,
    images: [ProducerImageIdentity; 2],
    notice: DeviceLossNotice,
    presentation: cambium_rootstock::PresentedFrame,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel_control(delta: [u8; 4], every_pixel: bool) -> RecoveryControl {
        let mut control = RecoveryControl {
            run: Some("pixel-test".into()),
            before: None,
            pixels: None,
            pixel_errors: Vec::new(),
            pixels_verified: false,
            pixel_measurement: None,
        };
        let baseline = cambium_rootstock::Frame {
            width: 4,
            height: 4,
            rgba: [20, 30, 40, 250].repeat(16),
        };
        let mut after = cambium_rootstock::Frame {
            width: 4,
            height: 4,
            rgba: baseline.rgba.clone(),
        };
        for pixel in after
            .rgba
            .chunks_exact_mut(4)
            .take(if every_pixel { 16 } else { 1 })
        {
            for channel in 0..4 {
                pixel[channel] += delta[channel];
            }
        }
        control.inspect("before_loss", &baseline);
        control.inspect("after_loss", &after);
        control
    }
    #[test]
    fn recovery_pixels_accept_existing_interior_boundary() {
        assert!(
            pixel_control([3, 0, 0, 2], false)
                .pixel_failures()
                .is_empty()
        );
    }
    #[test]
    fn recovery_pixels_reject_rgb_over_existing_interior_limit() {
        assert!(
            !pixel_control([4, 0, 0, 0], false)
                .pixel_failures()
                .is_empty()
        );
    }
    #[test]
    fn recovery_pixels_reject_alpha_over_existing_limit() {
        assert!(
            !pixel_control([0, 0, 0, 3], false)
                .pixel_failures()
                .is_empty()
        );
    }
    #[test]
    fn recovery_pixels_reject_mean_over_existing_limit() {
        assert!(
            !pixel_control([2, 2, 2, 0], true)
                .pixel_failures()
                .is_empty()
        );
    }

    fn refusal(control: RecoveryControl) -> String {
        let control = Rc::new(RefCell::new(control));
        let result = Rc::new(RefCell::new(None));
        let output = result.clone();
        let mut hooks = cambium_genet_winit_host::inert_hooks();
        hooks.frame = Box::new(move |ctx| {
            *output.borrow_mut() = Some(control.borrow_mut().destroy(ctx));
            false
        });
        let mut host = cambium_genet_winit_host::Harness::with_hooks(
            crate::initialize(tabard_workshop::WorkshopState::in_memory()),
            hooks,
        );
        host.prepare_frame();
        result
            .borrow_mut()
            .take()
            .expect("actual frame hook ran")
            .unwrap_err()
    }

    #[test]
    fn ordinary_windowless_host_refuses_unarmed_device_destruction() {
        assert!(
            refusal(RecoveryControl {
                run: None,
                before: None,
                pixels: None,
                pixel_errors: Vec::new(),
                pixels_verified: false,
                pixel_measurement: None
            })
            .contains("dedicated acceptance opt-in")
        );
    }

    #[test]
    fn armed_windowless_host_cannot_claim_an_actual_device_loss() {
        assert!(
            refusal(RecoveryControl {
                run: Some("test-windowless".into()),
                before: None,
                pixels: None,
                pixel_errors: Vec::new(),
                pixels_verified: false,
                pixel_measurement: None
            })
            .contains("real native core")
        );
    }
}

pub(super) struct RecoveryControl {
    pub run: Option<String>,
    before: Option<BeforeLoss>,
    pixels: Option<(u32, u32, Vec<u8>)>,
    pixel_errors: Vec<String>,
    pixels_verified: bool,
    pixel_measurement: Option<String>,
}
impl RecoveryControl {
    pub fn from_env() -> Self {
        let armed = std::env::var("TABARD_CONTROLLED_DEVICE_LOSS").as_deref() == Ok("1");
        Self {
            run: armed
                .then(|| std::env::var("TABARD_RECOVERY_RUN").ok())
                .flatten()
                .filter(|run| !run.trim().is_empty()),
            before: None,
            pixels: None,
            pixel_errors: Vec::new(),
            pixels_verified: false,
            pixel_measurement: None,
        }
    }
    fn authority(ctx: &Context<'_>) -> BTreeMap<String, String> {
        let state = ctx.runner.state();
        BTreeMap::from([
            (
                "theme_json".into(),
                serde_json::to_string(state.draft_theme()).expect("theme serializes"),
            ),
            ("mode".into(), state.mode_key().into()),
            ("name".into(), state.draft_theme().name.clone()),
            ("dirty".into(), state.is_dirty().to_string()),
            ("status".into(), state.status().into()),
            (
                "graph_selected".into(),
                state
                    .selected_graph_node()
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            ),
            ("focus_node".into(), format!("{:?}", ctx.runner.focus())),
            (
                "name_selection".into(),
                format!(
                    "{:?}",
                    state
                        .text_field("name")
                        .map(|field| field.caret_selection())
                ),
            ),
            ("zoom".into(), ctx.ui_zoom.to_string()),
        ])
    }
    pub fn destroy(&mut self, ctx: &mut Context<'_>) -> Result<(), String> {
        if self.run.is_none() {
            return Err("owned device destruction requires the dedicated acceptance opt-in and run identity".into());
        }
        if self.before.is_some() {
            return Err("owned device destruction was already requested".into());
        }
        if ctx.capture.is_some() || ctx.capture_stamped.is_some() {
            return Err("finish the baseline capture before destroying its device".into());
        }
        let core = ctx
            .render_core
            .ok_or("owned device destruction requires a real native core")?;
        if ctx.presentation.is_none() {
            return Err(
                "owned device destruction requires a successful baseline presentation".into(),
            );
        }
        let images =
            [READER_LEAF_KEY, STYLESHEET_LEAF_KEY].map(|key| ctx.producers.image_identity(key));
        let [Some(reader_image), Some(stylesheet_image)] = images else {
            return Err("both production preview images must be staged before loss".into());
        };
        let generation = core.generation();
        let owner = core.shared_recovery();
        let authority = Self::authority(ctx);
        let reader = Rc::downgrade(&ctx.runner.state().reader_preview());
        let stylesheet = Rc::downgrade(&ctx.runner.state().stylesheet_preview());
        let weak_core = Arc::downgrade(core);
        let weak_owner = Arc::downgrade(&owner);
        let runner = ctx.runner as *const _ as usize;
        // The actual callback enters the production monitor. No synthesized
        // notice, extra host subscription, retained Device, or blocking wait.
        core.device().destroy();
        let _ = core.device().poll(wgpu::PollType::Poll);
        let notice = core
            .device_failure()
            .ok_or("installed loss callback was not observed after owned device destruction")?;
        if notice.generation != generation || notice.reason != wgpu::DeviceLostReason::Destroyed {
            return Err(format!("unexpected owned-device loss notice: {notice:?}"));
        }
        self.before = Some(BeforeLoss {
            generation,
            core: weak_core,
            owner: weak_owner,
            reader,
            stylesheet,
            runner,
            authority,
            images: [reader_image, stylesheet_image],
            notice,
            presentation: ctx.presentation.expect("baseline presentation checked"),
        });
        ctx.wake.wake();
        Ok(())
    }
    pub fn fields(&self, ctx: &Context<'_>) -> BTreeMap<String, String> {
        let mut fields = Self::authority(ctx);
        let generation = ctx.render_core.map_or(0, |core| core.generation());
        fields.insert("core_generation".into(), generation.to_string());
        fields.insert("loss_requested".into(), self.before.is_some().to_string());
        for (name, key) in [
            ("reader", READER_LEAF_KEY),
            ("stylesheet", STYLESHEET_LEAF_KEY),
        ] {
            if let Some(image) = ctx.producers.image_identity(key) {
                fields.insert(format!("{name}_generation"), image.generation.to_string());
                fields.insert(format!("{name}_stamp"), image.stamp.to_string());
                fields.insert(format!("{name}_size"), format!("{:?}", image.size));
                if let Some(rect) = image.owner.and_then(|node| ctx.visible_rect(node)) {
                    fields.insert(format!("{name}_visible_rect"), format!("{rect:?}"));
                }
            }
        }
        if let Some(before) = &self.before {
            let same_owner = ctx.render_core.is_some_and(|core| {
                before
                    .owner
                    .upgrade()
                    .is_some_and(|owner| Arc::ptr_eq(&owner, &core.shared_recovery()))
            });
            let ready = ctx.render_core.is_some_and(|core| matches!(core.shared_recovery().status(), RecoveryStatus::Ready { generation: current } if current == generation));
            fields.insert("lost_generation".into(), before.generation.to_string());
            fields.insert("loss_reason".into(), format!("{:?}", before.notice.reason));
            fields.insert("owner_same".into(), same_owner.to_string());
            fields.insert(
                "old_core_released".into(),
                before.core.upgrade().is_none().to_string(),
            );
            fields.insert(
                "recovery_ready".into(),
                (ready && generation != before.generation).to_string(),
            );
            fields.insert(
                "runner_same".into(),
                (before.runner == ctx.runner as *const _ as usize).to_string(),
            );
            fields.insert(
                "reader_source_same".into(),
                before
                    .reader
                    .upgrade()
                    .is_some_and(|old| Rc::ptr_eq(&old, &ctx.runner.state().reader_preview()))
                    .to_string(),
            );
            fields.insert(
                "stylesheet_source_same".into(),
                before
                    .stylesheet
                    .upgrade()
                    .is_some_and(|old| Rc::ptr_eq(&old, &ctx.runner.state().stylesheet_preview()))
                    .to_string(),
            );
        }
        fields
    }
    pub fn verify(&self, ctx: &Context<'_>) -> Result<(), String> {
        let before = self
            .before
            .as_ref()
            .ok_or("no owned device destruction was requested")?;
        let fields = self.fields(ctx);
        let presentation = ctx.presentation.ok_or("replacement has not presented")?;
        if presentation.host != before.presentation.host
            || presentation.sequence <= before.presentation.sequence
        {
            return Err(
                "recovery did not present a new sequence in the original native host".into(),
            );
        }
        for name in [
            "recovery_ready",
            "owner_same",
            "old_core_released",
            "runner_same",
            "reader_source_same",
            "stylesheet_source_same",
        ] {
            if fields.get(name).map(String::as_str) != Some("true") {
                return Err(format!("recovery invariant failed: {name}"));
            }
        }
        if Self::authority(ctx) != before.authority {
            return Err(
                "authored state, focus, selection or zoom changed across device recovery".into(),
            );
        }
        for (key, old) in [READER_LEAF_KEY, STYLESHEET_LEAF_KEY]
            .into_iter()
            .zip(before.images)
        {
            let image = ctx
                .producers
                .image_identity(key)
                .ok_or("production preview was not restaged")?;
            if image.generation == before.generation
                || image.stamp <= old.stamp
                || image.size != old.size
            {
                return Err(format!(
                    "production preview {key} lacks a fresh same-size image identity"
                ));
            }
            if image
                .owner
                .and_then(|node| ctx.visible_rect(node))
                .is_none()
            {
                return Err(format!(
                    "production preview {key} has no visible intersection"
                ));
            }
        }
        Ok(())
    }
    pub fn lines(&self) -> Vec<String> {
        self.before.as_ref().map(|before| vec![format!("controlled owned-device loss generation={} reason={:?}; injection after baseline capture, one requested destruction", before.generation, before.notice.reason), format!("displayed RGBA within existing limits verified={}", self.pixels_verified), self.pixel_measurement.clone().unwrap_or_default()]).unwrap_or_default()
    }

    pub fn inspect(&mut self, name: &str, frame: &cambium_rootstock::Frame) {
        if self.run.is_none() || !matches!(name, "before_loss" | "after_loss") {
            return;
        }
        if frame.rgba.len() != frame.width as usize * frame.height as usize * 4 {
            self.pixel_errors
                .push("malformed recovery RGBA dimensions".into());
            return;
        }
        if frame.rgba.len() > 32 * 1024 * 1024 {
            self.pixel_errors
                .push("recovery capture exceeds the 32 MiB comparison bound".into());
            return;
        }
        if name == "before_loss" {
            self.pixels = Some((frame.width, frame.height, frame.rgba.clone()));
        } else if let Some((width, height, pixels)) = &self.pixels {
            self.pixels_verified = *width == frame.width
                && *height == frame.height
                && pixels.len() == frame.rgba.len();
            if self.pixels_verified {
                // Apply the existing stricter interior limits uniformly. The
                // renderer's looser edge/neighbor allowances are not needed.
                let mut max_rgba = 0_u8;
                let mut max_alpha = 0_u8;
                let mut sum = 0_u64;
                for (index, (before, after)) in pixels.iter().zip(&frame.rgba).enumerate() {
                    let delta = before.abs_diff(*after);
                    max_rgba = max_rgba.max(delta);
                    if index % 4 == 3 {
                        max_alpha = max_alpha.max(delta);
                    }
                    sum += u64::from(delta);
                }
                let mean = sum as f64 / pixels.len().max(1) as f64;
                self.pixels_verified = max_rgba <= 3 && max_alpha <= 2 && mean <= 1.0;
                self.pixel_measurement = Some(format!(
                    "displayed RGBA max={max_rgba} alpha={max_alpha} mean={mean:.12} exact={}; uniform existing interior limits RGB3/alpha2/mean1",
                    pixels == &frame.rgba
                ));
            }
            if !self.pixels_verified {
                self.pixel_errors.push(
                    "displayed recovery pixels exceed existing interior/mean limits or dimensions changed"
                        .into(),
                );
            }
        } else {
            self.pixel_errors
                .push("recovery lacks baseline RGBA bytes".into());
        }
    }
    pub fn pixel_failures(&self) -> Vec<String> {
        let mut errors = self.pixel_errors.clone();
        if self.run.is_some() && !self.pixels_verified {
            errors.push(
                "displayed recovery RGBA comparison under existing limits did not pass".into(),
            );
        }
        errors
    }
}
