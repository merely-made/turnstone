// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! What a screen reader reaches of the canvas and the board (dynamics
//! grammar plan, G9: F64 to F67), target-neutral so a native test reads it
//! through the same lowering a desktop host would.
//!
//! - [`canvas_semantics`] maps pictograph's [`CanvasDescription`] onto
//!   Cambium's producer semantics: a group named "N of M shown" whose items
//!   are groups, each item's advertised actions its neutral actions, which
//!   both lowerings draw as buttons (F65, F66).
//! - [`canvas_act`] carries out a reader's press of one: Pin pins the item
//!   where it is, Drag starts a keyboard move, which [`key_move`] steers.
//! - [`CanvasLocalActions`] and [`BoardLocalActions`] give graphshell-client's
//!   tree the viewer's own drag and pin on the local graph's items and the
//!   board's cards, carried out here and never sent to an endpoint (F64).
//! - [`Plant`] is the instruments' positive control: a missing item, a
//!   missing action, or an action that does not route.

use cambium_rootstock::{ProducerAction, ProducerNode, ProducerRole, ProducerSemantics};
use chirograph::AdvertisedAction;
use graphshell_client::{LocalActionTarget, LocalActions};
use mere::canvas::{
    ArrangementAction, Canvas, CanvasDescription, DESCRIBED_ITEMS, DRAG_INTENT, PIN_INTENT,
    PhysicsBoard, Role,
};

/// The local graph's items in a scene name this source adapter, with the
/// member's id (`MereHost`'s projection).
pub const GRAPH_SOURCE: &str = "mere.graph";

/// A planted defect, for an instrument's positive control. `None` in use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Plant {
    #[default]
    None,
    /// The first item is left out of the description.
    MissingItem,
    /// Every item's Pin is left out.
    MissingAction,
    /// Every action is described but none is carried out.
    DeadAction,
}

impl Plant {
    /// From a page parameter: `missing_item`, `missing_action`, `dead_action`.
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "missing_item" => Self::MissingItem,
            "missing_action" => Self::MissingAction,
            "dead_action" => Self::DeadAction,
            _ => return None,
        })
    }
}

/// An advertised action as a producer's neutral action: its intent the id,
/// its label, its explanation the description.
pub fn producer_action(action: &AdvertisedAction) -> ProducerAction {
    ProducerAction {
        id: action.intent.0.clone(),
        label: action.label.clone(),
        description: action.explanation.clone(),
    }
}

/// The canvas's description as its slot's semantics.
pub fn canvas_semantics(description: &CanvasDescription, plant: Plant) -> ProducerSemantics {
    let skip = usize::from(plant == Plant::MissingItem);
    let children = description
        .items
        .iter()
        .skip(skip)
        .map(|item| ProducerNode {
            key: item.key,
            role: ProducerRole::Group,
            name: item.name.clone(),
            rect: item.rect,
            actions: item
                .actions
                .iter()
                .filter(|action| plant != Plant::MissingAction || action.intent.0 != PIN_INTENT)
                .map(producer_action)
                .collect(),
        })
        .collect();
    ProducerSemantics {
        role: Some(ProducerRole::Group),
        name: Some(description.name()),
        children,
    }
}

/// The canvas's slot as a reader is told it now.
pub fn describe_canvas(canvas: &Canvas, plant: Plant) -> ProducerSemantics {
    canvas_semantics(&canvas.describe_items(DESCRIBED_ITEMS), plant)
}

/// Carry out a reader's press of action `id` on described item `key`: Pin
/// pins it where it is, Drag starts a keyboard move. Whether it was done.
pub fn canvas_act(canvas: &mut Canvas, key: u64, id: &str, plant: Plant) -> bool {
    if plant == Plant::DeadAction {
        return false;
    }
    let Some(member) = canvas.member_of_described(key) else {
        return false;
    };
    match id {
        PIN_INTENT => canvas.pin_member(member),
        DRAG_INTENT => canvas.begin_key_move(member),
        _ => false,
    }
}

/// A key during a keyboard move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveKey {
    Left,
    Right,
    Up,
    Down,
    /// Enter: drop it by its role.
    Drop,
    /// Escape: put it back where the move began.
    Back,
}

/// Steer the canvas's keyboard move by `key`, arrows nudging by `step`
/// screen px. Whether a move was under way to take it.
pub fn key_move(canvas: &mut Canvas, key: MoveKey, step: f32) -> bool {
    if canvas.key_moving().is_none() {
        return false;
    }
    match key {
        MoveKey::Left => canvas.key_move_by(-step, 0.0),
        MoveKey::Right => canvas.key_move_by(step, 0.0),
        MoveKey::Up => canvas.key_move_by(0.0, -step),
        MoveKey::Down => canvas.key_move_by(0.0, step),
        MoveKey::Drop => canvas.end_key_move(true),
        MoveKey::Back => canvas.end_key_move(false),
    }
}

/// The member a scene item names, when it is one of the local graph's.
fn member_of(item: &LocalActionTarget<'_>) -> Option<uuid::Uuid> {
    let source = item.source?;
    (source.adapter == GRAPH_SOURCE)
        .then(|| uuid::Uuid::parse_str(&source.id).ok())
        .flatten()
}

/// The viewer's drag and pin on the local graph's mounted items (F64, F67).
pub struct CanvasLocalActions<'a>(pub &'a mut Canvas);

impl LocalActions for CanvasLocalActions<'_> {
    fn actions(&self, item: &LocalActionTarget<'_>) -> Vec<AdvertisedAction> {
        member_of(item)
            .map(|member| self.0.advertised_actions(member))
            .unwrap_or_default()
    }

    fn invoke(&mut self, item: &LocalActionTarget<'_>, action: &AdvertisedAction) -> bool {
        let Some(member) = member_of(item) else {
            return false;
        };
        match action.intent.0.as_str() {
            PIN_INTENT => self.0.pin_member(member),
            DRAG_INTENT => self.0.begin_key_move(member),
            _ => false,
        }
    }
}

/// The viewer's drag and pin on the remote board's cards (F64, F67). A
/// card's id is its scene instance.
pub struct BoardLocalActions<'a>(pub &'a mut PhysicsBoard);

impl LocalActions for BoardLocalActions<'_> {
    fn actions(&self, item: &LocalActionTarget<'_>) -> Vec<AdvertisedAction> {
        self.0.advertised_actions(&item.instance.0.to_string())
    }

    fn invoke(&mut self, item: &LocalActionTarget<'_>, action: &AdvertisedAction) -> bool {
        let id = item.instance.0.to_string();
        match ArrangementAction::ALL
            .into_iter()
            .find(|candidate| candidate.intent() == action.intent.0)
        {
            Some(ArrangementAction::Pin) => self.0.set_item_role(&id, Some(Role::Pinned)),
            Some(ArrangementAction::Drag) => self.0.begin_key_move(&id),
            None => false,
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
