// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The remote board: a mounted remote scene described as Pictograph's
//! [`BoardScene`], and the [`PhysicsBoard`] that moves its cards.
//!
//! Both browser pages draw the remote session through this, so the mapping
//! from Graphshell's scene to the board is written once. The score's
//! positions become anchor slots; the board mirrors the local canvas's
//! physics choice; nothing is written back. (Physics catalog — P3.)

use graphshell_client::MountedScene;
use graphshell_client::frozen::Satisfaction;
use mere::canvas::{
    BoardBackdrop, BoardCard, BoardFootprint, BoardRect, BoardScene, PhysicsBoard,
    PhysicsChoice,
};
use crate::view::ProjectionLayoutView;

/// Describe a mounted scene as a board: one card per active item in order,
/// the first in the lead tone, held cards marked, backdrops from the layout.
pub fn board_scene(mounted: &MountedScene) -> BoardScene {
    let tables = &mounted.scene.tables;
    let bounds = tables.bounds;
    let layout = ProjectionLayoutView::from_scene(&mounted.scene);
    BoardScene {
        bounds: BoardRect::new(bounds.origin.x, bounds.origin.y, bounds.size.w, bounds.size.h),
        backdrops: layout
            .backdrops
            .iter()
            .map(|backdrop| BoardBackdrop {
                rect: BoardRect::new(backdrop.x, backdrop.y, backdrop.width, backdrop.height),
                kind: backdrop.kind.clone(),
                collidable: backdrop.collidable,
            })
            .collect(),
        cards: mounted
            .scene
            .active_items_in_order()
            .into_iter()
            .map(|(instance, item)| BoardCard {
                id: instance.0.to_string(),
                // The name the endpoint's presentation gives the card.
                title: mounted
                    .presentation
                    .offers_for(instance)
                    .and_then(|offers| offers.first())
                    .map(|offer| offer.semantics.label.clone())
                    .unwrap_or_default(),
                slot: (item.transform.translate.x, item.transform.translate.y),
                site: tables
                    .sources
                    .get(item.source.0 as usize)
                    .and_then(|source| source.as_ref())
                    .map(|source| source.adapter.clone())
                    .unwrap_or_default(),
                footprint: BoardFootprint::from(&item.footprint),
                pinned: Satisfaction::is_pinned(tables, instance),
                lead: instance.0 == 0,
            })
            .collect(),
    }
}

/// The board a host keeps for its remote session.
#[derive(Default)]
pub struct RemoteBoard {
    board: PhysicsBoard,
    scene: BoardScene,
    revision: Option<u64>,
}

impl RemoteBoard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stage the board's repulsion on the host's device (P5c).
    #[cfg(feature = "canvas-gpu")]
    pub fn set_physics_device(&mut self, device: Option<mere::canvas::PhysicsDevice>) {
        self.board.set_physics_device(device);
    }

    /// Mirror `choice` (a no-op when unchanged) and, whenever the
    /// acknowledged revision moves, reconcile the bodies to the scene: a new
    /// card spawns at its slot with a settle burst, the others keep their
    /// simulated positions, every slot is re-anchored.
    pub fn sync(&mut self, mounted: Option<&MountedScene>, revision: Option<u64>, choice: PhysicsChoice) {
        self.board.set_choice(choice);
        if revision == self.revision && !self.board.is_empty() {
            return;
        }
        let Some(mounted) = mounted else {
            return;
        };
        self.scene = board_scene(mounted);
        self.board.sync(self.scene.items());
        self.revision = revision;
    }

    /// Advance the board one frame; whether it is still moving.
    pub fn tick(&mut self) -> bool {
        self.board.tick()
    }

    pub fn board(&self) -> &PhysicsBoard {
        &self.board
    }

    /// The board, for the viewer's own actions on its cards (F64).
    pub fn board_mut(&mut self) -> &mut PhysicsBoard {
        &mut self.board
    }

    /// The board as last described; hosts paint it with
    /// `scene().paint(board(), ..)`.
    pub fn scene(&self) -> &BoardScene {
        &self.scene
    }

    /// Overlapping footprints at the positions drawn.
    pub fn overlaps(&self) -> usize {
        self.scene.overlaps(&self.board)
    }

    pub fn energy(&self) -> f32 {
        self.board.energy()
    }

    pub fn gap(&self, a: &str, b: &str) -> Option<f32> {
        self.board.gap(a, b)
    }

    pub fn is_settling(&self) -> bool {
        self.board.is_settling()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_endpoint::LiveEndpoint;
    use graphshell_client::ClientState;
    use graphshell_endpoint::ProjectionSource;

    fn mount(endpoint: &mut LiveEndpoint) -> (ClientState, chirograph::ProjectionSession) {
        let snapshot = endpoint.snapshot(endpoint.request()).expect("a snapshot");
        let session = snapshot.session.clone();
        let mut client = ClientState::default();
        client.apply_snapshot(snapshot).expect("it applies");
        (client, session)
    }

    #[test]
    fn the_live_board_maps_to_cards_at_their_slots() {
        let mut endpoint = LiveEndpoint::new();
        endpoint.append();
        let (client, session) = mount(&mut endpoint);
        let scene = board_scene(client.mounted(&session).unwrap());
        assert_eq!(scene.cards.len(), 2);
        assert!(scene.cards[0].lead && !scene.cards[1].lead);
        assert_eq!(scene.cards[0].slot, (0.0, 0.0));
        assert_eq!(scene.cards[1].slot, (140.0, 0.0));
        assert_eq!(scene.cards[0].site, "live.graphshell");
        assert_eq!(scene.cards[0].title, "Card 0", "a snapshot lists cards in index order");
        assert_eq!(scene.cards[1].title, "Card 1");
        assert!(matches!(scene.cards[0].footprint, BoardFootprint::Rect { .. }));
    }

    #[test]
    fn the_board_resyncs_only_when_the_revision_moves() {
        let mut endpoint = LiveEndpoint::new();
        let (client, session) = mount(&mut endpoint);
        let mut remote = RemoteBoard::new();
        remote.sync(client.mounted(&session), Some(1), PhysicsChoice::default());
        assert_eq!(remote.board().len(), 1);

        endpoint.append();
        let (client, session) = mount(&mut endpoint);
        // Same revision claimed: nothing is reconciled.
        remote.sync(client.mounted(&session), Some(1), PhysicsChoice::default());
        assert_eq!(remote.board().len(), 1);
        remote.sync(client.mounted(&session), Some(2), PhysicsChoice::default());
        assert_eq!(remote.board().len(), 2);
        assert_eq!(remote.scene().cards.len(), 2);
    }
}
