// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    /// Read a card, so a pre-key can be offered against the Moot it names.
    pub fn offer_place_prekey(&mut self, path: String) -> Vec<Effect> {
        if self.place.binding().is_some() {
            return self.refuse_place("leave this place before offering a pre-key elsewhere");
        }
        vec![Effect::ReadPlaceArtifact {
            path,
            kind: crate::action::PlaceArtifactKind::Card,
        }]
    }

    /// Publish this profile's group identity for the card's Moot.
    pub(crate) fn offer_place_prekey_for_card(
        &mut self,
        card: Box<crate::place::PlaceCardV1>,
        out: String,
    ) -> Vec<Effect> {
        if let Err(error) = card.validate() {
            return self.refuse_place(error);
        }
        self.next_place_generation = self.next_place_generation.wrapping_add(1);
        let generation = self.next_place_generation;
        self.pending_place_artifact = Some(super::super::PendingPlaceArtifact {
            generation,
            path: out,
            moot: card.binding.moot,
        });
        vec![Effect::OfferPlacePrekey {
            session: self.session_id,
            generation,
            moot: card.binding.moot.0,
        }]
    }

    /// Read a pre-key offer, so an invitation can be authored for it. The
    /// access travels with the read: the prompt that asked for the path is
    /// what decided whether this admits a writer or a reader.
    pub fn invite_to_place(
        &mut self,
        path: String,
        access: crate::place::PlaceInviteAccess,
        lifetime_ms: Option<u64>,
    ) -> Vec<Effect> {
        if self.place.binding().is_none() {
            return self.refuse_place("open a place before inviting anyone");
        }
        // Refused, not ignored: a lifetime the grant cannot carry is a mistake.
        if lifetime_ms.is_some() && access == crate::place::PlaceInviteAccess::Reader {
            return self.refuse_place("a reader invitation carries no grant to bound");
        }
        vec![Effect::ReadPlaceArtifact {
            path,
            kind: crate::action::PlaceArtifactKind::Prekey {
                access,
                lifetime_ms,
            },
        }]
    }

    pub(crate) fn invite_to_place_with_prekey(
        &mut self,
        prekey: Vec<u8>,
        out: String,
        access: crate::place::PlaceInviteAccess,
        lifetime_ms: Option<u64>,
    ) -> Vec<Effect> {
        let Some(binding) = self.place.binding().cloned() else {
            return self.refuse_place("open a place before inviting anyone");
        };
        let Some(generation) = self.place.generation() else {
            return self.refuse_place("open a place before inviting anyone");
        };
        self.pending_place_artifact = Some(super::super::PendingPlaceArtifact {
            generation,
            path: out,
            moot: binding.moot,
        });
        vec![Effect::InvitePlace {
            session: self.session_id,
            generation,
            prekey,
            access,
            lifetime_ms,
        }]
    }

    /// Revoke one member of the open place. The worker decides whether this
    /// profile may; the app only refuses when there is no place to ask.
    pub fn revoke_place_member(&mut self, member: [u8; 32]) -> Vec<Effect> {
        if !matches!(self.place, crate::place::PlaceState::Offline { .. }) {
            return self.refuse_place("open a place before revoking a member");
        }
        self.run_place_command(crate::place::worker::PlaceCommand::RevokeMember { member })
    }

    /// Read an invitation file, so the existing join path can admit it.
    pub fn join_place_file(&mut self, path: String) -> Vec<Effect> {
        if self.place.binding().is_some() {
            return self.refuse_place("leave this place before joining another");
        }
        vec![Effect::ReadPlaceArtifact {
            path,
            kind: crate::action::PlaceArtifactKind::Invite,
        }]
    }

    /// Ask the worker to admit `invite` into the current session.
    ///
    /// The app claims nothing while it waits. `Joining` carries a generation
    /// and no binding, because the envelope naming a place is not the same as
    /// the place having admitted this profile, and a refusal must leave the
    /// session exactly as personal as it was.
    pub fn join_place(&mut self, invite: Box<crate::place::invite::PlaceInviteV1>) -> Vec<Effect> {
        self.next_place_generation = self.next_place_generation.wrapping_add(1);
        let generation = self.next_place_generation;
        self.place = crate::place::PlaceState::Joining { generation };
        vec![Effect::JoinPlace {
            session: self.session_id,
            generation,
            invite,
        }]
    }

    /// Reopen admitted state and dial its saved contact hints on an explicit gesture.
    pub fn reconnect_place(&mut self) -> Vec<Effect> {
        self.reconnect_place_with_contact(None)
    }

    pub(crate) fn reconnect_place_with_contact(
        &mut self,
        contact: Option<Box<crate::place::PlaceCardV1>>,
    ) -> Vec<Effect> {
        // `binding()` reads `Left` as not joined, so a left session cannot
        // reconnect around its own mark; `rejoin_place` is its route back.
        let Some(binding) = self.place.binding().cloned() else {
            self.events.push(AppEvent::PlaceRefused(
                "join a place before reconnecting".into(),
            ));
            return vec![Effect::Redraw];
        };
        if let Some(card) = &contact {
            if let Err(error) = crate::place::rendezvous::card_tickets(&binding, card) {
                return self.refuse_place(error);
            }
        }
        self.next_place_generation = self.next_place_generation.wrapping_add(1);
        let generation = self.next_place_generation;
        self.place = crate::place::PlaceState::Opening {
            binding: binding.clone(),
            generation,
        };
        vec![Effect::ReconnectPlace {
            session: self.session_id,
            generation,
            binding,
            contact,
        }]
    }

    /// Reconnect a locally left place: clear its left mark, then dial saved
    /// contact hints the same way `reconnect_place` does for a still-live
    /// one. Valid only from `PlaceState::Left`, and the only route out of
    /// it: `reconnect_place` reads `Left` as not joined.
    pub fn rejoin_place(&mut self) -> Vec<Effect> {
        let Some(binding) = self.place.left_binding().cloned() else {
            self.events.push(AppEvent::PlaceRefused(
                "this session did not leave a place to rejoin".into(),
            ));
            return vec![Effect::Redraw];
        };
        self.next_place_generation = self.next_place_generation.wrapping_add(1);
        let generation = self.next_place_generation;
        self.place = crate::place::PlaceState::Opening {
            binding: binding.clone(),
            generation,
        };
        vec![
            Effect::ClearPlaceLeftMark {
                session: self.session_id,
            },
            Effect::ReconnectPlace {
                session: self.session_id,
                generation,
                binding,
                contact: None,
            },
        ]
    }

    /// Request local departure. The shell releases the worker and marks this
    /// place locally left -- binding, rendezvous hints and every retained
    /// store stay in place, so `Rejoin` has what it needs -- before
    /// completing the transition.
    pub fn leave_place(&mut self) -> Vec<Effect> {
        let Some(generation) = self.place.generation() else {
            self.events.push(AppEvent::PlaceRefused(
                "this session is not in a place".into(),
            ));
            return vec![Effect::Redraw];
        };
        // Retained counts come from the Offline snapshot in hand right now;
        // any other prior state (Degraded, a retried leave) has no snapshot
        // to read, so the summary reads zero rather than guessing at what
        // the stores hold. Computed here, once, rather than recomputed in
        // `finish_leave_place`: the shell needs the same counts to write the
        // left mark, and `self.place` does not change while the effect is
        // in flight.
        let retained = match &self.place {
            crate::place::PlaceState::Offline { snapshot, .. } => crate::place::PlaceLeftSummary {
                graph_nodes: snapshot.graph.nodes,
                chat_messages: snapshot.chat.messages,
                members: snapshot.moot.members,
            },
            _ => crate::place::PlaceLeftSummary::default(),
        };
        vec![Effect::LeavePlace {
            session: self.session_id,
            generation,
            retained,
        }]
    }

    pub(crate) fn finish_leave_place(
        &mut self,
        session: SessionId,
        generation: u64,
        retained: crate::place::PlaceLeftSummary,
        result: Result<bool, String>,
    ) -> Vec<Effect> {
        if self.session_id != session || self.place.generation() != Some(generation) {
            return Vec::new();
        }
        match result {
            Ok(_) => {
                // A binding is what makes Left rejoinable; a leave from
                // Joining (admission never finished, so there was never one)
                // has nothing to remember and reads as Personal, as it
                // always has.
                self.place = match self.place.binding().cloned() {
                    Some(binding) => crate::place::PlaceState::Left { binding, retained },
                    None => crate::place::PlaceState::Personal,
                };
                vec![Effect::Redraw]
            },
            Err(error) => {
                let binding = self.place.binding().cloned();
                self.next_place_generation = self.next_place_generation.wrapping_add(1);
                let failed_generation = self.next_place_generation;
                if let Some(binding) = binding {
                    self.place = crate::place::PlaceState::Degraded {
                        binding,
                        generation: failed_generation,
                        error: error.clone(),
                    };
                } else {
                    self.place = crate::place::PlaceState::Failed {
                        error: error.clone(),
                    };
                }
                self.events
                    .push(AppEvent::PlaceRefused(format!("leaving place: {error}")));
                vec![Effect::Redraw]
            },
        }
    }

    /// Bring the shared graph's addresses into this session's Canvas.
    ///
    /// Additive by construction. The place decides what the PLACE holds, never
    /// what this person's canvas holds, so a node the person put there is
    /// never moved, relabelled, or removed by shared state converging; a
    /// shared node that leaves the place simply stops arriving here. The only
    /// mutation is minting what is missing, which is why this can run on every
    /// resync without accumulating damage.
    ///
    /// Returns how many nodes it minted, so a caller can tell "nothing new"
    /// from "nothing happened".
    pub fn reconcile_shared_graph(
        &mut self,
        shared: &crate::place::projection::SharedGraph,
    ) -> usize {
        let missing: Vec<String> = shared
            .addresses()
            .filter(|address| {
                self.graph_runtimes
                    .graph()
                    .get_node_by_url(address)
                    .is_none()
            })
            .map(str::to_string)
            .collect();
        // Selection is the person's, so restore it: `visit` selects what it
        // mints, and a background resync must not move the cursor.
        let selected = self.graph_runtimes.selected_members();
        for address in &missing {
            self.graph_runtimes.visit(address);
        }
        if !missing.is_empty() {
            for member in selected {
                self.graph_runtimes.select_member(member);
            }
        }
        missing.len()
    }

    /// The focused node's address, for sharing it into the place.
    pub fn focused_address(&self) -> Option<String> {
        let member = self.graph_runtimes.focused_member()?;
        let (_, node) = self.graph_runtimes.graph().get_node_by_id(member)?;
        let url = node.url().trim();
        (!url.is_empty()).then(|| url.to_string())
    }

    /// Lower one place command, if this session actually has an open place.
    ///
    /// Refused locally when there is no place rather than sent to a worker
    /// that would refuse it anyway: "you are not in a place" is a different
    /// answer from "the place refused you", and only one of them is about
    /// authority.
    pub fn run_place_command(
        &mut self,
        command: crate::place::worker::PlaceCommand,
    ) -> Vec<Effect> {
        let Some(generation) = self.place.generation() else {
            self.events.push(AppEvent::PlaceRefused(
                "this session is not in a place".into(),
            ));
            return vec![Effect::Redraw];
        };
        self.next_place_request = self.next_place_request.wrapping_add(1);
        vec![Effect::RunPlaceCommand {
            session: self.session_id,
            generation,
            request: self.next_place_request,
            command,
        }]
    }

    /// Ask the worker to re-fold what the lanes have drained in.
    pub fn resync_place(&mut self) -> Vec<Effect> {
        let Some(generation) = self.place.generation() else {
            return Vec::new();
        };
        vec![Effect::ResyncPlace {
            session: self.session_id,
            generation,
        }]
    }

    /// Ask the open place worker to remint captured search through one exact
    /// collection version. This is a local view choice and authors no Moot fact.
    pub fn set_place_collection(
        &mut self,
        selection: Option<crate::place::PlaceCollectionVersion>,
    ) -> Vec<Effect> {
        let Some(generation) = self.place.generation() else {
            self.events.push(AppEvent::PlaceRefused(
                "this session is not in a place".into(),
            ));
            return vec![Effect::Redraw];
        };
        vec![Effect::SetPlaceCollection {
            session: self.session_id,
            generation,
            selection,
        }]
    }
}
