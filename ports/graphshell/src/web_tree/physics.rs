// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The "Graph tools" side region's first section: arrangement and physics.
//!
//! Cambium controls hold the form; Apply reads them into the typed inputs of
//! `mere::canvas::physics_controls`, which the old page's form also calls.
use super::*;
use cambium::{
    SelectState, TextInput, button, checkbox, disclosure, lens, select, text_field_typed,
};
use mere::canvas::channels::Channel;
use mere::canvas::dynamics_spec::Node;
use mere::canvas::physics_controls::{
    self, ArrangementTransition, CUSTOM_PROFILE, LawStart, StageDraft, TwoLawGrouping, TwoLawMix,
    TwoStageSchedule, arrangement_choices,
};
use mere::canvas::{
    CANVAS_PHYSICS_DEPTH_SOURCES, CANVAS_PHYSICS_KIND_SOURCES, CANVAS_PHYSICS_LAWS,
    CANVAS_PHYSICS_MASS_SOURCES, CANVAS_PHYSICS_OVERLAYS, CANVAS_PHYSICS_PROFILES, PhysicsChoice,
    PhysicsDepthSource, PhysicsKindSource, PhysicsLaw, PhysicsMassSource, PhysicsOverlay, Role,
    StageStop,
};

/// The profile picker's first entry, read when the live pair names no profile.
const CUSTOM_LABEL: &str = "Custom (no profile)";

/// The panel's control state and the arrangement it last applied.
pub(super) struct PhysicsPanel {
    pub(super) arrangement: SelectState,
    /// The recipe's role for the arrangement's positions (F48).
    pub(super) role: SelectState,
    pub(super) law: SelectState,
    /// The two explicit law choices for the bounded mix editor.
    pub(super) mix_first: SelectState,
    pub(super) mix_second: SelectState,
    /// Text fields preserve authored f64 shares on reopen. Apply still checks
    /// that each share can be represented by the runtime's f32 weight.
    pub(super) mix_first_share: TextInput,
    pub(super) mix_second_share: TextInput,
    pub(super) mix_note: String,
    /// The grouped root's partition channel, separate from Group pull and
    /// the role target's grouping channel.
    pub(super) group_partition: SelectState,
    pub(super) group_outer: SelectState,
    pub(super) group_inner: SelectState,
    pub(super) group_outer_share: TextInput,
    pub(super) group_note: String,
    /// The bounded authored schedule editor: exactly two plain law stages.
    pub(super) schedule_first_law: SelectState,
    pub(super) schedule_first_stop: SelectState,
    pub(super) schedule_first_frames: TextInput,
    pub(super) schedule_first_capture: SelectState,
    pub(super) schedule_second_law: SelectState,
    pub(super) schedule_second_stop: SelectState,
    pub(super) schedule_second_frames: TextInput,
    pub(super) schedule_second_capture: SelectState,
    /// Explicit common overlays, independent of either stage law.
    pub(super) schedule_overlays: Vec<bool>,
    pub(super) schedule_note: String,
    pub(super) kind: SelectState,
    pub(super) mass: SelectState,
    pub(super) depth: SelectState,
    /// Group pull's channel (any kind source; F37).
    pub(super) groups: SelectState,
    /// Role overrides' group channel, independent of Group pull (F49).
    pub(super) role_groups: SelectState,
    /// Index 0 is custom; profile `i` is at `i + 1`.
    pub(super) profile: SelectState,
    /// One per `CANVAS_PHYSICS_OVERLAYS` entry, in catalog order.
    pub(super) overlays: Vec<bool>,
    pub(super) layout_id: String,
    pub(super) status: String,
    pub(super) transition: Option<ArrangementTransition>,
    /// The layout where the last law was applied.
    pub(super) law_start: Option<LawStart>,
    /// The speed preset picked, the one the canvas runs, and the note on the
    /// speed reached while the budget binds.
    pub(super) speed: SelectState,
    pub(super) applied_speed: usize,
    pub(super) speed_note: Option<String>,
}

fn index_of<T: PartialEq>(items: impl IntoIterator<Item = T>, item: T) -> usize {
    items.into_iter().position(|i| i == item).unwrap_or(0)
}

fn labels(catalog: &[(&'static str, &'static str)]) -> Vec<&'static str> {
    catalog.iter().map(|(_, label)| *label).collect()
}

/// A plain law preset the bounded editor can faithfully expose. Per-part
/// overlays, terms and rungs need richer controls and must not be hidden.
fn mix_part(node: &Node) -> Option<(PhysicsLaw, f64)> {
    match node {
        Node::Preset {
            id,
            weight,
            terms,
            rung,
            overlays,
        } if terms.is_empty() && rung.is_none() && overlays.is_empty() => {
            Some((PhysicsLaw::parse(id)?, *weight))
        },
        _ => None,
    }
}

impl PhysicsPanel {
    pub(super) fn new(canvas: &Canvas, layout_id: &str) -> Self {
        let mut panel = Self {
            arrangement: SelectState::new(index_of(
                arrangement_choices().map(|(id, _)| id),
                layout_id,
            ))
            .with_label("Arrangement"),
            role: SelectState::new(0).with_label("Role"),
            law: SelectState::new(0).with_label("Physics law"),
            mix_first: SelectState::new(0).with_label("First law"),
            mix_second: SelectState::new(1).with_label("Second law"),
            mix_first_share: TextInput::new("1"),
            mix_second_share: TextInput::new("1"),
            mix_note: String::new(),
            group_partition: SelectState::new(0).with_label("Grouping partition"),
            group_outer: SelectState::new(index_of(PhysicsLaw::ALL, PhysicsLaw::Charge))
                .with_label("Outer law"),
            group_inner: SelectState::new(index_of(PhysicsLaw::ALL, PhysicsLaw::Springs))
                .with_label("Inner law"),
            group_outer_share: TextInput::new(
                mere::canvas::composition::CHARGE_BETWEEN_WEIGHT.to_string(),
            ),
            group_note: String::new(),
            schedule_first_law: SelectState::new(0).with_label("First stage law"),
            schedule_first_stop: SelectState::new(0).with_label("First stage stop"),
            schedule_first_frames: TextInput::new("60"),
            schedule_first_capture: SelectState::new(0).with_label("First stage capture"),
            schedule_second_law: SelectState::new(1).with_label("Second stage law"),
            schedule_second_stop: SelectState::new(0).with_label("Second stage stop"),
            schedule_second_frames: TextInput::new("60"),
            schedule_second_capture: SelectState::new(0).with_label("Second stage capture"),
            schedule_overlays: vec![false; CANVAS_PHYSICS_OVERLAYS.len()],
            schedule_note: String::new(),
            kind: SelectState::new(0).with_label("Kinds"),
            mass: SelectState::new(0).with_label("Mass"),
            depth: SelectState::new(0).with_label("Depth"),
            groups: SelectState::new(0).with_label("Groups"),
            role_groups: SelectState::new(0).with_label("Role groups"),
            profile: SelectState::new(0).with_label("Profile"),
            overlays: vec![false; CANVAS_PHYSICS_OVERLAYS.len()],
            layout_id: layout_id.to_string(),
            status: String::new(),
            transition: None,
            law_start: None,
            speed: SelectState::new(crate::web_speed::preset_of(canvas)).with_label("Speed"),
            applied_speed: crate::web_speed::preset_of(canvas),
            speed_note: None,
        };
        panel.sync(canvas);
        panel
    }

    /// Set every physics control to the canvas's live choice. A mix root is
    /// custom because its first flat law does not describe the full profile.
    pub(super) fn sync(&mut self, canvas: &Canvas) {
        let live = PhysicsChoice::live(canvas);
        self.law.selected = index_of(PhysicsLaw::ALL, live.law);
        self.kind.selected = index_of(PhysicsKindSource::ALL, live.kind);
        self.mass.selected = index_of(PhysicsMassSource::ALL, live.mass);
        self.depth.selected = index_of(PhysicsDepthSource::ALL, live.depth);
        self.groups.selected = index_of(PhysicsKindSource::ALL, live.groups);
        self.sync_role_target(canvas);
        for (checked, overlay) in self.overlays.iter_mut().zip(PhysicsOverlay::ALL) {
            *checked = live.overlays.contains(&overlay);
        }
        self.profile.selected = live
            .profile_id()
            .and_then(|id| CANVAS_PHYSICS_PROFILES.iter().position(|p| p.id == id))
            .map_or(0, |index| index + 1);
        let spec = canvas.dynamics_spec().ok();
        if spec.as_ref().is_some_and(|spec| {
            matches!(
                &spec.root,
                Node::Mix { .. } | Node::Grouped { .. } | Node::Schedule { .. }
            )
        }) {
            // A composed or scheduled root cannot be named by a profile
            // inferred from the live law or first stage.
            self.profile.selected = 0;
        }
        self.sync_mix(spec.as_ref().map(|spec| &spec.root));
        self.sync_grouping(spec.as_ref().map(|spec| &spec.root));
        self.sync_schedule(canvas);
    }

    /// Sync role controls from the live, canonical target and resolved Canvas.
    fn sync_role_target(&mut self, canvas: &Canvas) {
        self.role.selected = index_of(Role::ALL, canvas.arrangement_roles().default);
        self.role_groups.selected = index_of(PhysicsKindSource::ALL, canvas.role_group_source());
    }

    /// Read the authored root when it is the exact two-law form this editor
    /// can represent. A flat live choice is not presented as the whole mix.
    fn sync_mix(&mut self, root: Option<&Node>) {
        let Some(root) = root else {
            self.mix_note = "The authored dynamics record could not be read".into();
            return;
        };
        let Node::Mix { parts, weight, .. } = root else {
            self.mix_note = "No authored two-law mix".into();
            return;
        };
        if *weight != 1.0 {
            self.mix_note =
                "Unsupported authored mix shape (root weight) · editor values are not readback"
                    .into();
            return;
        }
        if parts.len() != 2 {
            self.mix_note =
                "Unsupported authored mix shape (part count) · editor values are not readback"
                    .into();
            return;
        }
        let (Some((first, first_share)), Some((second, second_share))) =
            (mix_part(&parts[0]), mix_part(&parts[1]))
        else {
            self.mix_note =
                "Unsupported authored mix shape (part data) · editor values are not readback"
                    .into();
            return;
        };
        self.mix_first.selected = index_of(PhysicsLaw::ALL, first);
        self.mix_second.selected = index_of(PhysicsLaw::ALL, second);
        self.mix_first_share = TextInput::new(first_share.to_string());
        self.mix_second_share = TextInput::new(second_share.to_string());
        self.mix_note.clear();
    }

    /// Read the authored root only when it is the exact plain grouped form
    /// this editor can represent. The outer share remains authored f64 text.
    fn sync_grouping(&mut self, root: Option<&Node>) {
        let Some(root) = root else {
            self.group_note = "The authored dynamics record could not be read".into();
            return;
        };
        let Node::Grouped {
            partition,
            outer,
            inner,
            weight,
            ..
        } = root
        else {
            self.group_note = "No authored grouping".into();
            return;
        };
        if *weight != 1.0 {
            self.group_note =
                "Unsupported authored grouping shape (root weight) · editor values are not readback"
                    .into();
            return;
        }
        let Some(source) = Channel::parse(partition).and_then(|channel| match channel {
            Channel::Groups(source) => Some(source),
            _ => None,
        }) else {
            self.group_note =
                "Unsupported authored grouping shape (partition) · editor values are not readback"
                    .into();
            return;
        };
        let (Some((outer_law, outer_share)), Some((inner_law, inner_share))) =
            (mix_part(outer), mix_part(inner))
        else {
            self.group_note =
                "Unsupported authored grouping shape (law data) · editor values are not readback"
                    .into();
            return;
        };
        if inner_share != 1.0 {
            self.group_note =
                "Unsupported authored grouping shape (inner weight) · editor values are not readback"
                    .into();
            return;
        }
        self.group_partition.selected = index_of(PhysicsKindSource::ALL, source);
        self.group_outer.selected = index_of(PhysicsLaw::ALL, outer_law);
        self.group_inner.selected = index_of(PhysicsLaw::ALL, inner_law);
        self.group_outer_share = TextInput::new(outer_share.to_string());
        self.group_note.clear();
    }

    /// Read only the exact shape represented by the bounded two-stage form.
    /// Unsupported records leave the draft untouched and state that its
    /// values are not readback.
    fn sync_schedule(&mut self, canvas: &Canvas) {
        match physics_controls::read_two_stage_schedule(canvas) {
            Ok(Some(schedule)) => {
                self.set_schedule_stage(true, schedule.first);
                self.set_schedule_stage(false, schedule.second);
                for (checked, overlay) in self.schedule_overlays.iter_mut().zip(PhysicsOverlay::ALL)
                {
                    *checked = schedule.overlays.contains(&overlay);
                }
                self.schedule_note.clear();
            },
            Ok(None) => self.schedule_note = "No authored two-stage schedule".into(),
            Err(reason) => {
                self.schedule_note = format!(
                    "Unsupported authored schedule shape · {reason} · editor values are not readback"
                );
            },
        }
    }

    fn set_schedule_stage(&mut self, first: bool, stage: StageDraft) {
        let (law, stop, frames, capture) = if first {
            (
                &mut self.schedule_first_law,
                &mut self.schedule_first_stop,
                &mut self.schedule_first_frames,
                &mut self.schedule_first_capture,
            )
        } else {
            (
                &mut self.schedule_second_law,
                &mut self.schedule_second_stop,
                &mut self.schedule_second_frames,
                &mut self.schedule_second_capture,
            )
        };
        law.selected = index_of(PhysicsLaw::ALL, stage.law);
        match stage.stop {
            StageStop::Rest => stop.selected = 0,
            StageStop::Frames(count) => {
                stop.selected = 1;
                *frames = TextInput::new(count.to_string());
            },
            StageStop::LawDone => stop.selected = 2,
        }
        capture.selected = stage
            .capture
            .and_then(|role| Role::ALL.iter().position(|candidate| *candidate == role))
            .map_or(0, |index| index + 1);
    }

    fn stage_draft(
        law: &SelectState,
        stop: &SelectState,
        frames: &TextInput,
        capture: &SelectState,
        label: &str,
    ) -> Result<StageDraft, String> {
        let stage_stop = match stop.selected.min(2) {
            0 => StageStop::Rest,
            1 => {
                let count = frames
                    .text()
                    .parse::<u32>()
                    .ok()
                    .filter(|count| *count > 0)
                    .ok_or_else(|| format!("{label} frames must be a positive integer"))?;
                StageStop::Frames(count)
            },
            _ => StageStop::LawDone,
        };
        Ok(StageDraft {
            law: PhysicsLaw::ALL[law.selected.min(PhysicsLaw::ALL.len() - 1)],
            stop: stage_stop,
            capture: capture
                .selected
                .checked_sub(1)
                .and_then(|index| Role::ALL.get(index).copied()),
        })
    }

    fn schedule_draft(&self) -> Result<TwoStageSchedule, String> {
        let first = Self::stage_draft(
            &self.schedule_first_law,
            &self.schedule_first_stop,
            &self.schedule_first_frames,
            &self.schedule_first_capture,
            "First stage",
        )?;
        let second = Self::stage_draft(
            &self.schedule_second_law,
            &self.schedule_second_stop,
            &self.schedule_second_frames,
            &self.schedule_second_capture,
            "Second stage",
        )?;
        let overlays = PhysicsOverlay::ALL
            .into_iter()
            .zip(&self.schedule_overlays)
            .filter_map(|(overlay, checked)| checked.then_some(overlay))
            .collect();
        Ok(TwoStageSchedule {
            first,
            second,
            overlays,
        })
    }

    /// The recipe role the role control holds.
    pub(super) fn role(&self) -> Role {
        Role::ALL[self.role.selected.min(Role::ALL.len() - 1)]
    }

    /// The choice the controls hold. Overlays the picked law refuses are
    /// left out, whatever was ticked before it was picked.
    pub(super) fn choice(&self) -> PhysicsChoice {
        let law = self.picked_law();
        PhysicsChoice {
            law,
            overlays: physics_controls::ticked_overlays(|overlay| {
                law.refuses(overlay).is_none()
                    && PhysicsOverlay::ALL
                        .iter()
                        .position(|o| *o == overlay)
                        .is_some_and(|index| self.overlays[index])
            }),
            kind: PhysicsKindSource::ALL[self.kind.selected.min(PhysicsKindSource::ALL.len() - 1)],
            groups: PhysicsKindSource::ALL
                [self.groups.selected.min(PhysicsKindSource::ALL.len() - 1)],
            mass: PhysicsMassSource::ALL[self.mass.selected.min(PhysicsMassSource::ALL.len() - 1)],
            depth: PhysicsDepthSource::ALL
                [self.depth.selected.min(PhysicsDepthSource::ALL.len() - 1)],
        }
    }

    /// The profile id the picker shows, or custom.
    pub(super) fn profile_id(&self) -> &'static str {
        self.profile
            .selected
            .checked_sub(1)
            .and_then(|index| CANVAS_PHYSICS_PROFILES.get(index))
            .map_or(CUSTOM_PROFILE, |profile| profile.id)
    }

    /// The overlay ids ticked in the panel, comma-joined.
    pub(super) fn ticked(&self) -> String {
        self.choice()
            .overlays
            .iter()
            .map(|overlay| overlay.id())
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Whether the speed picker names a preset the canvas does not run yet.
    pub(super) fn speed_pending(&self) -> bool {
        self.speed.selected != self.applied_speed
    }

    /// The law the picker names (applied or not).
    pub(super) fn picked_law(&self) -> PhysicsLaw {
        PhysicsLaw::ALL[self.law.selected.min(PhysicsLaw::ALL.len() - 1)]
    }
}

impl TreePage {
    fn apply_arrangement(&mut self) {
        let Some((id, _)) = arrangement_choices().nth(self.physics.arrangement.selected) else {
            return;
        };
        let mut canvas = self.shared.canvas.borrow_mut();
        self.physics.transition = None;
        self.physics.status =
            match physics_controls::apply_arrangement(&mut canvas, id, self.shared.size.get()) {
                Ok(applied) => {
                    self.physics.layout_id = applied.layout_id;
                    self.physics.transition = applied.transition;
                    applied.status
                },
                Err(error) => format!("Failed · {error}"),
            };
        self.shared.dirty.set(true);
    }

    /// The recipe's role: every item takes it unless a group or the item
    /// says otherwise (F22, F48).
    fn apply_role(&mut self) {
        let role = self.physics.role();
        let mut canvas = self.shared.canvas.borrow_mut();
        match physics_controls::apply_role(&mut canvas, role) {
            Ok(status) => {
                self.physics.status = status;
                self.physics.sync_role_target(&canvas);
                self.shared.dirty.set(true);
            },
            Err(error) => self.physics.status = format!("Role change refused · {error}"),
        }
    }

    /// Set the channel used to read role overrides. Group pull remains an
    /// independent physics source.
    fn apply_role_groups(&mut self) {
        let source = PhysicsKindSource::ALL[self
            .physics
            .role_groups
            .selected
            .min(PhysicsKindSource::ALL.len() - 1)];
        let mut canvas = self.shared.canvas.borrow_mut();
        match physics_controls::apply_role_group_source(&mut canvas, source) {
            Ok(status) => {
                self.physics.status = status;
                self.physics.sync_role_target(&canvas);
                self.shared.dirty.set(true);
            },
            Err(error) => self.physics.status = format!("Role groups change refused · {error}"),
        }
    }

    fn apply_physics(&mut self) {
        let choice = self.physics.choice();
        let mut canvas = self.shared.canvas.borrow_mut();
        self.physics.law_start = Some(LawStart::of(&canvas));
        self.physics.status = physics_controls::apply_physics(&mut canvas, &choice);
        self.physics.sync(&canvas);
        self.shared.dirty.set(true);
    }

    /// Apply only the editor's explicit two-law mix form.
    fn apply_mix(&mut self) {
        let parsed = self
            .physics
            .mix_first_share
            .text()
            .parse::<f64>()
            .and_then(|first_share| {
                self.physics
                    .mix_second_share
                    .text()
                    .parse::<f64>()
                    .map(|second_share| (first_share, second_share))
            });
        let Ok((first_share, second_share)) = parsed else {
            self.physics.status = "Mix shares must be numbers".into();
            return;
        };
        let mix = TwoLawMix {
            first: PhysicsLaw::ALL[self
                .physics
                .mix_first
                .selected
                .min(PhysicsLaw::ALL.len() - 1)],
            second: PhysicsLaw::ALL[self
                .physics
                .mix_second
                .selected
                .min(PhysicsLaw::ALL.len() - 1)],
            first_share,
            second_share,
        };
        let mut canvas = self.shared.canvas.borrow_mut();
        let law_start = LawStart::of(&canvas);
        match physics_controls::apply_mix(&mut canvas, &mix) {
            Ok(status) => {
                self.physics.law_start = Some(law_start);
                self.physics.status = status;
                self.physics.sync(&canvas);
                self.shared.dirty.set(true);
            },
            Err(error) => self.physics.status = format!("Mix refused · {error}"),
        }
    }

    /// Apply one bounded grouped composition. Its partition source is
    /// independent of Group pull and role-target sources.
    fn apply_grouping(&mut self) {
        let Ok(outer_share) = self.physics.group_outer_share.text().parse::<f64>() else {
            self.physics.status = "Grouping outer share must be a number".into();
            return;
        };
        let grouping = TwoLawGrouping {
            partition: PhysicsKindSource::ALL[self
                .physics
                .group_partition
                .selected
                .min(PhysicsKindSource::ALL.len() - 1)],
            outer: PhysicsLaw::ALL[self
                .physics
                .group_outer
                .selected
                .min(PhysicsLaw::ALL.len() - 1)],
            inner: PhysicsLaw::ALL[self
                .physics
                .group_inner
                .selected
                .min(PhysicsLaw::ALL.len() - 1)],
            outer_share,
        };
        let mut canvas = self.shared.canvas.borrow_mut();
        let law_start = LawStart::of(&canvas);
        match physics_controls::apply_grouping(&mut canvas, &grouping) {
            Ok(status) => {
                self.physics.law_start = Some(law_start);
                self.physics.status = status;
                self.physics.sync(&canvas);
                self.shared.dirty.set(true);
            },
            Err(error) => self.physics.status = format!("Grouping refused · {error}"),
        }
    }

    /// Apply the bounded two-stage form. The typed action owns the canonical
    /// spec update and intentionally starts this schedule at stage zero.
    fn apply_schedule(&mut self) {
        let draft = match self.physics.schedule_draft() {
            Ok(draft) => draft,
            Err(error) => {
                self.physics.status = format!("Schedule refused · {error}");
                return;
            },
        };
        let mut canvas = self.shared.canvas.borrow_mut();
        let law_start = LawStart::of(&canvas);
        match physics_controls::apply_two_stage_schedule(&mut canvas, &draft) {
            Ok(status) => {
                self.physics.law_start = Some(law_start);
                self.physics.status = status;
                self.physics.sync(&canvas);
                self.shared.dirty.set(true);
            },
            Err(error) => self.physics.status = format!("Schedule refused · {error}"),
        }
    }

    /// Explicitly replace any authored composition with the picked single law.
    fn use_single_law(&mut self) {
        let choice = self.physics.choice();
        let mut canvas = self.shared.canvas.borrow_mut();
        let law_start = LawStart::of(&canvas);
        match physics_controls::apply_single_law(&mut canvas, &choice) {
            Ok(status) => {
                self.physics.law_start = Some(law_start);
                self.physics.status = status;
                self.physics.sync(&canvas);
                self.shared.dirty.set(true);
            },
            Err(error) => self.physics.status = format!("Single law refused · {error}"),
        }
    }

    /// The picked speed, applied when chosen (ruled 2026-10-04, "Speed select").
    pub(super) fn apply_speed(&mut self) {
        let index = self.physics.speed.selected;
        let mut canvas = self.shared.canvas.borrow_mut();
        self.physics.status = crate::web_speed::choose(&mut canvas, index);
        self.physics.applied_speed = index;
        self.shared.dirty.set(true);
    }

    fn apply_profile(&mut self) {
        let id = self.physics.profile_id();
        let mut canvas = self.shared.canvas.borrow_mut();
        self.physics.law_start = Some(LawStart::of(&canvas));
        self.physics.status = match physics_controls::apply_profile(&mut canvas, id) {
            Ok(status) => status,
            Err(error) => format!("Failed · {error}"),
        };
        self.physics.sync(&canvas);
        self.shared.dirty.set(true);
    }

    /// Step a running arrangement transition at host time `host_ms`. Under
    /// reduced motion it lands on this frame: a second step, far enough on
    /// to finish any schedule.
    pub(super) fn advance_arrangement(&mut self, host_ms: f64) {
        /// Past the end of any arrangement transition, in ms.
        const LAND_MS: f64 = 3_600_000.0;
        let reduced = self.shared.reduced_motion();
        let mut canvas = self.shared.canvas.borrow_mut();
        let steps: &[f64] = if reduced {
            &[host_ms, host_ms + LAND_MS]
        } else {
            &[host_ms]
        };
        for &at in steps {
            if let Some(status) = physics_controls::advance_arrangement(
                &mut canvas,
                &mut self.physics.transition,
                &self.physics.layout_id,
                at,
            ) {
                self.physics.status = status;
            }
        }
        self.shared.dirty.set(true);
    }

    /// Apply the arrangement last applied again, onto a new graph: a host
    /// history's checkpoint keeps the reader's arrangement.
    pub(super) fn reapply_arrangement(&mut self) {
        let layout_id = self.physics.layout_id.clone();
        let mut canvas = self.shared.canvas.borrow_mut();
        self.physics.transition = None;
        match physics_controls::apply_arrangement(&mut canvas, &layout_id, self.shared.size.get()) {
            Ok(applied) => {
                self.physics.transition = applied.transition;
                self.physics.status = applied.status;
            },
            Err(error) => self.physics.status = format!("Failed · {error}"),
        }
    }
}

/// A labelled select bound to one of the panel's states.
fn picker(
    caption: &'static str,
    options: Vec<&'static str>,
    state: fn(&mut TreePage) -> &mut SelectState,
) -> Child {
    Box::new(el(
        "label",
        (
            el("span", caption).attr("class", "tools-caption"),
            lens(move |s: &mut SelectState| select(s, &options), state),
        ),
    ))
}

/// A tagged law selector for the mix editor's stable browser/test hooks.
fn mix_law_picker(
    caption: &'static str,
    role: &'static str,
    state: fn(&mut TreePage) -> &mut SelectState,
) -> Child {
    let options = labels(CANVAS_PHYSICS_LAWS);
    Box::new(
        el(
            "label",
            (
                el("span", caption).attr("class", "tools-caption"),
                lens(move |s: &mut SelectState| select(s, &options), state),
            ),
        )
        .attr("data-physics-law-role", role),
    )
}

/// An arbitrary-precision share editor. Parsing and validity checks happen at
/// Apply time, where a refusal leaves the live canvas untouched.
fn mix_share_field(
    caption: &'static str,
    role: &'static str,
    state: fn(&mut TreePage) -> &mut TextInput,
) -> Child {
    Box::new(
        el(
            "label",
            (
                el("span", caption).attr("class", "tools-caption"),
                lens(
                    move |input: &mut TextInput| {
                        text_field_typed(input).attr("aria-label", caption)
                    },
                    state,
                ),
            ),
        )
        .attr("data-physics-share-role", role),
    )
}

fn schedule_law_picker(
    caption: &'static str,
    stage: &'static str,
    state: fn(&mut TreePage) -> &mut SelectState,
) -> Child {
    let options = labels(CANVAS_PHYSICS_LAWS);
    Box::new(
        el(
            "label",
            (
                el("span", caption).attr("class", "tools-caption"),
                lens(move |s: &mut SelectState| select(s, &options), state),
            ),
        )
        .attr("data-physics-schedule-stage", stage)
        .attr("data-physics-schedule-field", "law"),
    )
}

fn schedule_stop_picker(
    caption: &'static str,
    stage: &'static str,
    state: fn(&mut TreePage) -> &mut SelectState,
) -> Child {
    let options = vec!["Rest", "Frames", "Law done"];
    Box::new(
        el(
            "label",
            (
                el("span", caption).attr("class", "tools-caption"),
                lens(move |s: &mut SelectState| select(s, &options), state),
            ),
        )
        .attr("data-physics-schedule-stage", stage)
        .attr("data-physics-schedule-field", "stop"),
    )
}

fn schedule_capture_picker(
    caption: &'static str,
    stage: &'static str,
    state: fn(&mut TreePage) -> &mut SelectState,
) -> Child {
    let options: Vec<_> = std::iter::once("No capture")
        .chain(Role::ALL.iter().map(|role| role.label()))
        .collect();
    Box::new(
        el(
            "label",
            (
                el("span", caption).attr("class", "tools-caption"),
                lens(move |s: &mut SelectState| select(s, &options), state),
            ),
        )
        .attr("data-physics-schedule-stage", stage)
        .attr("data-physics-schedule-field", "capture"),
    )
}

fn schedule_frames_field(
    caption: &'static str,
    stage: &'static str,
    state: fn(&mut TreePage) -> &mut TextInput,
) -> Child {
    Box::new(
        el(
            "label",
            (
                el("span", caption).attr("class", "tools-caption"),
                lens(
                    move |input: &mut TextInput| {
                        text_field_typed(input).attr("aria-label", caption)
                    },
                    state,
                ),
            ),
        )
        .attr("data-physics-schedule-stage", stage)
        .attr("data-physics-schedule-field", "frames"),
    )
}

/// Common schedule overlays are intentionally never filtered or disabled by
/// either stage law. The complete selected set is offered to the binder.
fn schedule_overlay_group() -> Child {
    let boxes: Vec<Child> = CANVAS_PHYSICS_OVERLAYS
        .iter()
        .zip(PhysicsOverlay::ALL)
        .enumerate()
        .map(|(index, ((id, label), overlay))| {
            Box::new(
                el(
                    "label",
                    (
                        lens(
                            move |checked: &mut bool| {
                                checkbox(*checked).attr("aria-label", format!("Schedule {label}"))
                            },
                            move |page: &mut TreePage| &mut page.physics.schedule_overlays[index],
                        ),
                        el("span", *label),
                    ),
                )
                .attr("data-physics-schedule-overlay", overlay.id())
                .attr("data-overlay-id", *id),
            ) as Child
        })
        .collect();
    Box::new(
        el(
            "fieldset",
            (
                el("legend", "Common overlays"),
                el("p", "Every selected overlay applies to both stages. If either law rejects one, the whole schedule is refused.")
                    .attr("class", "tools-note"),
                el("div", boxes),
            ),
        )
        .attr("data-physics-schedule-overlays", "common"),
    )
}

fn apply(label: &'static str, action: fn(&mut TreePage)) -> Child {
    Box::new(button(label, move |page: &mut TreePage, _| action(page)))
}

fn physics_action(label: &'static str, id: &'static str, action: fn(&mut TreePage)) -> Child {
    Box::new(
        el(
            "div",
            button(label, move |page: &mut TreePage, _| action(page)),
        )
        .attr("data-physics-action", id),
    )
}

/// The overlay checkboxes, each greyed and inert while the picked law
/// refuses it, with the law's reason beneath.
fn overlay_group(page: &TreePage) -> Child {
    let law = page.physics.picked_law();
    let mut reason = None;
    let mut boxes: Vec<Child> = CANVAS_PHYSICS_OVERLAYS
        .iter()
        .zip(PhysicsOverlay::ALL)
        .enumerate()
        .map(
            |(index, ((_, label), overlay))| match law.refuses(overlay) {
                None => Box::new(el(
                    "label",
                    (
                        lens(
                            move |checked: &mut bool| {
                                checkbox(*checked)
                                    .attr("aria-label", *label)
                                    .attr("data-overlay", overlay.id())
                            },
                            move |page: &mut TreePage| &mut page.physics.overlays[index],
                        ),
                        el("span", *label),
                    ),
                )) as Child,
                Some(why) => {
                    reason = Some(why);
                    Box::new(
                        el(
                            "label",
                            (
                                el("span", "[ ]")
                                    .attr("role", "checkbox")
                                    .attr("data-overlay", overlay.id())
                                    .attr("aria-label", *label)
                                    .attr("aria-checked", "false")
                                    .attr("aria-disabled", "true")
                                    .attr("aria-describedby", "tools-overlay-note")
                                    .attr("class", "checkbox disabled"),
                                el("span", *label),
                            ),
                        )
                        .attr("class", "disabled"),
                    ) as Child
                },
            },
        )
        .collect();
    if let Some(reason) = reason {
        boxes.push(Box::new(
            el("p", reason)
                .attr("id", "tools-overlay-note")
                .attr("role", "status")
                .attr("aria-label", "Overlay availability")
                .attr("class", "tools-note"),
        ));
    }
    let group = el("div", boxes)
        .attr("class", "tools-overlays")
        .attr("role", "group")
        .attr("aria-label", "Overlays");
    let all_refused = PhysicsOverlay::ALL
        .iter()
        .all(|o| law.refuses(*o).is_some());
    if all_refused {
        Box::new(group.attr("aria-disabled", "true"))
    } else {
        Box::new(group)
    }
}

/// The speed reached, beneath the speed picker, while the budget binds.
fn speed_note(page: &TreePage) -> Child {
    match &page.physics.speed_note {
        Some(note) => Box::new(
            el("p", note.clone())
                .attr("class", "tools-note")
                .attr("role", "status")
                .attr("aria-label", "Speed status"),
        ),
        None => Box::new(el("span", "").attr("hidden", "")),
    }
}

/// The "Graph tools" region's arrangement and physics section.
pub(super) fn section(page: &TreePage) -> Child {
    let profiles: Vec<&'static str> = std::iter::once(CUSTOM_LABEL)
        .chain(CANVAS_PHYSICS_PROFILES.iter().map(|profile| profile.label))
        .collect();
    let section: Vec<Child> = vec![
        picker(
            "Arrangement",
            arrangement_choices().map(|(_, label)| label).collect(),
            |page| &mut page.physics.arrangement,
        ),
        apply("Apply arrangement", TreePage::apply_arrangement),
        picker(
            "Role",
            Role::ALL.iter().map(|role| role.label()).collect(),
            |page| &mut page.physics.role,
        ),
        apply("Apply role", TreePage::apply_role),
        picker("Physics law", labels(CANVAS_PHYSICS_LAWS), |page| {
            &mut page.physics.law
        }),
        overlay_group(page),
        picker("Kinds", labels(CANVAS_PHYSICS_KIND_SOURCES), |page| {
            &mut page.physics.kind
        }),
        picker("Groups", labels(CANVAS_PHYSICS_KIND_SOURCES), |page| {
            &mut page.physics.groups
        }),
        picker("Role groups", labels(CANVAS_PHYSICS_KIND_SOURCES), |page| {
            &mut page.physics.role_groups
        }),
        Box::new(
            el(
                "p",
                "Role groups chooses which groups role overrides use. Groups supplies Group pull.",
            )
            .attr("class", "tools-note"),
        ),
        physics_action(
            "Apply role groups",
            "apply-role-groups",
            TreePage::apply_role_groups,
        ),
        picker("Mass", labels(CANVAS_PHYSICS_MASS_SOURCES), |page| {
            &mut page.physics.mass
        }),
        picker("Depth", labels(CANVAS_PHYSICS_DEPTH_SOURCES), |page| {
            &mut page.physics.depth
        }),
        apply("Apply physics", TreePage::apply_physics),
        Box::new(
            el(
                "div",
                (
                    el("h3", "Two-law mix"),
                    mix_law_picker("First law", "first", |page| &mut page.physics.mix_first),
                    mix_share_field("First share", "first", |page| {
                        &mut page.physics.mix_first_share
                    }),
                    mix_law_picker("Second law", "second", |page| &mut page.physics.mix_second),
                    mix_share_field("Second share", "second", |page| {
                        &mut page.physics.mix_second_share
                    }),
                    el(
                        "p",
                        "Shares set each law’s contribution; they do not need to add to one.",
                    )
                    .attr("class", "tools-note"),
                    physics_action("Apply mix", "apply-mix", TreePage::apply_mix),
                    physics_action("Use single law", "use-single-law", TreePage::use_single_law),
                    el("p", page.physics.mix_note.clone())
                        .attr("class", "tools-note")
                        .attr("role", "status")
                        .attr("aria-label", "Authored mix status"),
                ),
            )
            .attr("data-physics-composition-editor", "two-law-mix"),
        ),
        Box::new(
            el(
                "div",
                (
                    el("h3", "Grouped composition"),
                    picker("Grouping partition", labels(CANVAS_PHYSICS_KIND_SOURCES), |page| {
                        &mut page.physics.group_partition
                    }),
                    mix_law_picker("Outer law", "group-outer", |page| {
                        &mut page.physics.group_outer
                    }),
                    mix_share_field("Outer share", "group-outer", |page| {
                        &mut page.physics.group_outer_share
                    }),
                    mix_law_picker("Inner law", "group-inner", |page| {
                        &mut page.physics.group_inner
                    }),
                    el(
                        "p",
                        "The outer law's repulsion acts between groups; the inner law acts within each group. The outer share scales that repulsion and is not normalized.",
                    )
                    .attr("class", "tools-note"),
                    physics_action("Apply grouping", "apply-grouping", TreePage::apply_grouping),
                    el("p", page.physics.group_note.clone())
                        .attr("class", "tools-note")
                        .attr("role", "status")
                        .attr("aria-label", "Authored grouping status"),
                ),
            )
            .attr("data-physics-composition-editor", "grouped"),
        ),
        Box::new(
            el(
                "div",
                (
                    el("h3", "Two-stage schedule"),
                    el(
                        "p",
                        "Frame counts apply only when a stage stops after Frames. A capture applies its role when that stage ends.",
                    )
                    .attr("class", "tools-note"),
                    el(
                        "p",
                        "Applying starts stage 1 from the current layout. Source and role edits keep the running stage.",
                    )
                    .attr("class", "tools-note"),
                    el("h4", "Stage 1"),
                    schedule_law_picker("First stage law", "first", |page| {
                        &mut page.physics.schedule_first_law
                    }),
                    schedule_stop_picker("First stage stop", "first", |page| {
                        &mut page.physics.schedule_first_stop
                    }),
                    schedule_frames_field("First stage frames", "first", |page| {
                        &mut page.physics.schedule_first_frames
                    }),
                    schedule_capture_picker("First stage capture", "first", |page| {
                        &mut page.physics.schedule_first_capture
                    }),
                    el("h4", "Stage 2"),
                    schedule_law_picker("Second stage law", "second", |page| {
                        &mut page.physics.schedule_second_law
                    }),
                    schedule_stop_picker("Second stage stop", "second", |page| {
                        &mut page.physics.schedule_second_stop
                    }),
                    schedule_frames_field("Second stage frames", "second", |page| {
                        &mut page.physics.schedule_second_frames
                    }),
                    schedule_capture_picker("Second stage capture", "second", |page| {
                        &mut page.physics.schedule_second_capture
                    }),
                    schedule_overlay_group(),
                    physics_action(
                        "Apply and run schedule",
                        "apply-schedule",
                        TreePage::apply_schedule,
                    ),
                    el("p", page.physics.schedule_note.clone())
                        .attr("class", "tools-note")
                        .attr("role", "status")
                        .attr("aria-label", "Authored schedule status"),
                ),
            )
            .attr("data-physics-composition-editor", "two-stage-schedule"),
        ),
        picker(
            "Speed",
            crate::web_speed::PRESETS
                .iter()
                .map(|(_, label)| *label)
                .collect(),
            |page| &mut page.physics.speed,
        ),
        speed_note(page),
        picker("Profile", profiles, |page| &mut page.physics.profile),
        apply("Apply profile", TreePage::apply_profile),
        Box::new(
            el("p", page.physics.status.clone())
                .attr("aria-label", "Physics status")
                .attr("class", "tools-status")
                .attr("role", "status"),
        ),
    ];
    Box::new(
        el(
            "section",
            disclosure(&page.sections.physics, section, |page: &mut TreePage| {
                page.sections.physics.toggle()
            }),
        )
        .attr("class", "tools-section")
        .attr("aria-label", "Arrangement and physics"),
    )
}
