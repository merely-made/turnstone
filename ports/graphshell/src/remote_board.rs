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
//! live stage (dynamics grammar plan, F150) and its speed dial (ruled 2026-10-04, "The viewer's own
//! dial"); nothing is written back. (Physics catalog — P3.)

use crate::view::ProjectionLayoutView;
use graphshell_client::MountedScene;
use graphshell_client::frozen::Satisfaction;
use mere::canvas::dynamics_spec::DynamicsSpec;
use mere::canvas::{
    BoardBackdrop, BoardCard, BoardFootprint, BoardRect, BoardScene, PhysicsBoard, Speed,
};

/// Describe a mounted scene as a board: one card per active item in order,
/// the first in the lead tone, held cards marked, backdrops from the layout.
pub fn board_scene(mounted: &MountedScene) -> BoardScene {
    let tables = &mounted.scene.tables;
    let bounds = tables.bounds;
    let layout = ProjectionLayoutView::from_scene(&mounted.scene);
    BoardScene {
        bounds: BoardRect::new(
            bounds.origin.x,
            bounds.origin.y,
            bounds.size.w,
            bounds.size.h,
        ),
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
    position: Option<(scenotime::SceneEpoch, Option<u64>, Option<String>)>,
    /// The viewer's speed, as last mirrored.
    speed: Speed,
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

    /// Mirror the viewer's live `stage` and `speed` (no-ops when unchanged)
    /// and, whenever the acknowledged revision moves, reconcile the bodies to
    /// the scene: a new card spawns at its slot with a settle burst, the
    /// others keep their simulated positions, every slot is re-anchored. The
    /// stage is the canvas's [`live_stage_spec`](mere::canvas::Canvas::live_stage_spec)
    /// (dynamics grammar plan, F150): its law or composition, the stage under
    /// way when a schedule runs. `None`, or a stage the board refuses, leaves
    /// the board's stage as it was.
    pub fn sync(
        &mut self,
        mounted: Option<&MountedScene>,
        revision: Option<u64>,
        stage: Option<&DynamicsSpec>,
        speed: Speed,
    ) {
        self.sync_scoped(mounted, revision, None, stage, speed)
    }

    /// Read the acknowledged score's layout exactly. A resident mere has its
    /// own saved layout; simulated bodies from another attachment or layout
    /// cannot determine this reading, nor can the hidden local canvas's law.
    pub fn sync_reading(
        &mut self,
        mounted: Option<&MountedScene>,
        revision: Option<u64>,
        identity: Option<&str>,
    ) {
        let position =
            mounted.map(|mounted| (mounted.scene.epoch, revision, identity.map(str::to_owned)));
        if position != self.position {
            self.board.sync(Vec::new());
        }
        self.sync_scoped(mounted, revision, identity, None, Speed::REAL_TIME);
        self.board.halt();
    }

    /// An attached session is a separate identity even when an endpoint reuses
    /// its projection name, epoch or revision. It owns no durable graph here.
    pub fn sync_scoped(
        &mut self,
        mounted: Option<&MountedScene>,
        revision: Option<u64>,
        identity: Option<&str>,
        stage: Option<&DynamicsSpec>,
        speed: Speed,
    ) {
        if let Some(stage) = stage {
            let _ = self.board.set_stage(stage);
        }
        if speed != self.speed {
            self.speed = speed;
            self.board.set_speed(speed);
        }
        let Some(mounted) = mounted else {
            self.scene = BoardScene::default();
            self.board.sync(self.scene.items());
            self.position = None;
            return;
        };
        let position = (mounted.scene.epoch, revision, identity.map(str::to_owned));
        if Some(&position) == self.position.as_ref() && !self.board.is_empty() {
            return;
        }
        self.scene = board_scene(mounted);
        self.board.sync(self.scene.items());
        self.position = Some(position);
    }

    /// The step budget the board's frames run under: the page's, as its
    /// canvas's.
    pub fn set_step_budget(&mut self, budget: Option<mere::canvas::StepBudget>) {
        self.board.set_step_budget(budget);
    }

    /// The speed the board steps at: the viewer's, as last mirrored.
    pub fn speed(&self) -> Speed {
        self.speed
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

    /// Presentation-only emphasis of the reader's selected instance.
    pub fn set_focus(&mut self, instance: Option<&str>) {
        for card in &mut self.scene.cards {
            card.lead = instance == Some(card.id.as_str());
        }
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
    use graphshell_client::ClientState;
    use graphshell_endpoint::ProjectionSource;
    use graphshell_host::live_endpoint::LiveEndpoint;

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
        assert_eq!(
            scene.cards[0].title, "Card 0",
            "a snapshot lists cards in index order"
        );
        assert_eq!(scene.cards[1].title, "Card 1");
        assert!(matches!(
            scene.cards[0].footprint,
            BoardFootprint::Rect { .. }
        ));
    }

    #[test]
    fn the_board_resyncs_only_when_the_revision_moves() {
        let mut endpoint = LiveEndpoint::new();
        let (client, session) = mount(&mut endpoint);
        let mut remote = RemoteBoard::new();
        remote.sync(
            client.mounted(&session),
            Some(1),
            Some(&mere::canvas::PhysicsChoice::default().into_spec()),
            Speed::REAL_TIME,
        );
        assert_eq!(remote.board().len(), 1);

        endpoint.append();
        let (client, session) = mount(&mut endpoint);
        // Same revision claimed: nothing is reconciled.
        remote.sync(
            client.mounted(&session),
            Some(1),
            Some(&mere::canvas::PhysicsChoice::default().into_spec()),
            Speed::REAL_TIME,
        );
        assert_eq!(remote.board().len(), 1);
        remote.sync(
            client.mounted(&session),
            Some(2),
            Some(&mere::canvas::PhysicsChoice::default().into_spec()),
            Speed::REAL_TIME,
        );
        assert_eq!(remote.board().len(), 2);
        assert_eq!(remote.scene().cards.len(), 2);
    }

    /// The board steps at the speed the viewer's dial chose, and a change of
    /// dial reaches it on the next sync. (Ruled 2026-10-04, "The viewer's own
    /// dial".)
    #[test]
    fn the_board_steps_at_the_viewers_chosen_speed() {
        let mut endpoint = LiveEndpoint::new();
        endpoint.append();
        let (client, session) = mount(&mut endpoint);
        let mounted = client.mounted(&session);
        let mut remote = RemoteBoard::new();
        let steps = |remote: &mut RemoteBoard, frames: u64| {
            let before = remote.board().pace().ticks;
            for _ in 0..frames {
                remote.tick();
            }
            remote.board().pace().ticks - before
        };
        remote.sync(
            mounted,
            Some(1),
            Some(&mere::canvas::PhysicsChoice::default().into_spec()),
            Speed::REAL_TIME,
        );
        assert_eq!(steps(&mut remote, 4), 4, "1x: a tick a frame");
        remote.sync(
            mounted,
            Some(1),
            Some(&mere::canvas::PhysicsChoice::default().into_spec()),
            Speed::from_factor(5.0),
        );
        assert_eq!(remote.speed(), Speed::from_factor(5.0));
        assert_eq!(steps(&mut remote, 4), 20, "the viewer's 5x");
        // Control: the same board back at the viewer's 1x runs a tick a frame.
        remote.sync(
            mounted,
            Some(1),
            Some(&mere::canvas::PhysicsChoice::default().into_spec()),
            Speed::REAL_TIME,
        );
        assert_eq!(steps(&mut remote, 4), 4);
    }
}

#[cfg(test)]
mod scoped_session_tests {
    use super::*;
    use graphshell_client::ClientState;
    use graphshell_endpoint::{ProjectionCatalog, ProjectionSource};
    use graphshell_host::live_endpoint::LiveEndpoint;
    #[test]
    fn resident_reused_cards_take_new_owner_slots_and_stay_fitted_without_local_dynamics() {
        let mut endpoint = LiveEndpoint::new();
        for _ in 0..7 {
            endpoint.append();
        }
        let snapshot = endpoint
            .snapshot(endpoint.describe().projections[0].request.clone())
            .unwrap();
        let mut client = ClientState::default();
        client.apply_snapshot(snapshot.clone()).unwrap();
        let mut thaw = client.mounted(&snapshot.session).unwrap().clone();
        for item in thaw.scene.tables.items.iter_mut().flatten() {
            item.transform.translate.x += 6000.0;
            item.transform.translate.y += 3000.0;
        }
        let mut remote = RemoteBoard::new();
        remote.sync_scoped(
            Some(&thaw),
            Some(1),
            Some("thaw:spectral.default"),
            Some(&mere::canvas::PhysicsChoice::default().into_spec()),
            Speed::REAL_TIME,
        );
        let mut original = thaw.clone();
        for (index, item) in original.scene.tables.items.iter_mut().flatten().enumerate() {
            item.transform.translate.x = (index % 4) as f32 * 180.0;
            item.transform.translate.y = (index / 4) as f32 * 120.0;
        }
        remote.sync_reading(Some(&original), Some(1), Some("original:grid.default"));
        for _ in 0..120 {
            remote.tick();
        }
        for card in &remote.scene().cards {
            assert_eq!(remote.board().position(&card.id), Some(card.slot));
        }
        assert!(!remote.is_settling());
        for (width, height) in [(1100, 762), (1400, 900), (420, 600)] {
            let fit = mere::canvas::BoardFit::new(24.0, 24.0, 24.0, 24.0);
            let rects = remote
                .scene()
                .card_rects(remote.board(), width, height, fit);
            assert_eq!(rects.len(), 8);
            for (_, _, rect) in rects {
                assert!(rect.x >= 23.99 && rect.y >= 23.99);
                assert!(rect.x + rect.width <= width as f32 - 23.99);
                assert!(rect.y + rect.height <= height as f32 - 23.99);
            }
        }
        // An external same-layout edit can change slots at a new revision.
        original.scene.tables.items[0]
            .as_mut()
            .unwrap()
            .transform
            .translate
            .y = 240.0;
        remote.sync_reading(Some(&original), Some(2), Some("original:grid.default"));
        assert_eq!(remote.board().position("0"), Some((0.0, 240.0)));
    }
    #[test]
    fn epoch_and_attached_session_are_independent_of_revision_and_close_clears_the_board() {
        let mut endpoint = LiveEndpoint::new();
        let request = endpoint.describe().projections[0].request.clone();
        let snapshot = endpoint.snapshot(request).unwrap();
        let mut client = ClientState::default();
        client.apply_snapshot(snapshot.clone()).unwrap();
        let mut mounted = client.mounted(&snapshot.session).unwrap().clone();
        let mut board = RemoteBoard::new();
        board.sync_scoped(
            Some(&mounted),
            Some(1),
            Some("first"),
            None,
            Speed::REAL_TIME,
        );
        assert_eq!(board.board().len(), 1);
        endpoint.append();
        let snapshot = endpoint
            .snapshot(endpoint.describe().projections[0].request.clone())
            .unwrap();
        client.apply_snapshot(snapshot.clone()).unwrap();
        mounted = client.mounted(&snapshot.session).unwrap().clone();
        board.sync_scoped(
            Some(&mounted),
            Some(1),
            Some("second"),
            None,
            Speed::REAL_TIME,
        );
        assert_eq!(
            board.board().len(),
            2,
            "different attachment at the same epoch and revision replaces the canvas"
        );
        mounted.scene.epoch.0 += 1;
        mounted.scene.tables.items[1] = None;
        mounted.scene.tables.item_order[1] = None;
        board.sync_scoped(
            Some(&mounted),
            Some(1),
            Some("second"),
            None,
            Speed::REAL_TIME,
        );
        assert_eq!(
            board.board().len(),
            1,
            "epoch changes cannot reuse another canvas"
        );
        board.sync_scoped(None, None, None, None, Speed::REAL_TIME);
        assert_eq!(
            board.board().len(),
            0,
            "closed authority retains no drawn records"
        );
    }
}
