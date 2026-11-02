// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl LiveLanes {
    /// Leave every lane and wait for its sync actor to release the store
    /// clone it captured, before this value is dropped.
    ///
    /// Only a caller about to REOPEN these stores in the same process needs
    /// this: it costs seconds, and an ordinary close does not care who still
    /// holds a handle. Nothing but drop may follow it.
    pub(crate) fn leave_and_wait(&mut self) {
        if let (Some(joined), Some(runtime)) = (self.joined.take(), self._runtime.as_ref()) {
            runtime.block_on(joined.leave_and_wait());
        }
    }

    /// Stop like a killed process: every task dropped at once, no goodbye on
    /// the wire, and nothing released.
    ///
    /// The ordinary `Drop` aborts the accept loop, closes the endpoint and
    /// waits, which is exactly what a process that dies does NOT do — so a test
    /// about abrupt loss cannot use it. The leak is the point: the socket stays
    /// open and silent, which is the worst case a visitor can be handed.
    #[cfg(test)]
    pub(crate) fn abandon(mut self) {
        if let Some(runtime) = self._runtime.take() {
            runtime.shutdown_background();
        }
        std::mem::forget(self);
    }

    /// Stop answering while the socket stays open, for `hold`.
    ///
    /// The other half of "the holder went away": a process that is alive and
    /// reachable but no longer running — a frozen machine, a suspended laptop,
    /// a partitioned link. Nothing reaches the wire at all, so no transport
    /// error is coming; only a deadline of the visitor's own ends the wait.
    /// Every worker thread of the lane runtime is parked, which is what the
    /// endpoint's driver runs on.
    #[cfg(test)]
    pub(crate) fn freeze(&self, hold: std::time::Duration) {
        let handle = self
            ._runtime
            .as_ref()
            .expect("lane runtime is present")
            .handle();
        for _ in 0..4 {
            handle.spawn(async move {
                std::thread::sleep(hold);
            });
        }
    }

    pub(super) fn joined(&self) -> &JoinedLanes {
        self.joined.as_ref().expect("lanes are taken only in drop")
    }

    pub(super) fn counter_handles(&self) -> LaneCounters {
        let joined = self.joined();
        let mut handles = joined.moot.status_handles().to_vec();
        handles.push(joined.graph.status_handle());
        handles.push(joined.chat.status_handle());
        handles.push(joined.group_keys.status_handle());
        LaneCounters {
            handles,
            projection: self
                .projection
                .as_ref()
                .map(ProjectionServing::watch_counters),
        }
    }

    /// Per-lane accepted-operation counters: Gemot's seven, graph, chat, then
    /// group keys. These counters do not establish whether a lane is caught up.
    pub(crate) fn ops_received(&self) -> [u64; 10] {
        let joined = self.joined();
        let gemot = joined.moot.sync_status();
        [
            gemot[0].ops_received,
            gemot[1].ops_received,
            gemot[2].ops_received,
            gemot[3].ops_received,
            gemot[4].ops_received,
            gemot[5].ops_received,
            gemot[6].ops_received,
            joined.graph.sync_status().ops_received,
            joined.chat.sync_status().ops_received,
            joined.group_keys.sync_status().ops_received,
        ]
    }

    /// What this bind is serving by projection, or `None` when it serves
    /// nothing because this profile has no Knot vault.
    pub(crate) fn projection_snapshot(&self) -> Option<crate::place::PlaceProjectionSnapshot> {
        self.projection.as_ref().map(ProjectionServing::snapshot)
    }

    /// This bind's own tickets, and how many peers it dialed. Neither says a
    /// peer is reachable; the first says where this one can be reached.
    pub(crate) fn rendezvous(&self) -> (Vec<String>, usize) {
        (self.local_rendezvous.clone(), self.dialed_rendezvous)
    }

    pub(crate) fn sync_snapshot(&self) -> Vec<crate::place::PlaceLaneSnapshot> {
        const NAMES: [&str; 10] = [
            "gemot/constitution/v1",
            "gemot/delegation/v1",
            "gemot/membership/v1",
            "gemot/records/v1",
            "gemot/standing/v1",
            "gemot/tulpa/v1",
            "gemot/flora/v1",
            COMMONS_ENCRYPTED_GRAPH_LANE,
            "commons/chat/v1",
            stickleback::GROUP_KEY_LANE,
        ];
        let joined = self.joined();
        let statuses = joined.moot.sync_status().into_iter().chain([
            joined.graph.sync_status(),
            joined.chat.sync_status(),
            joined.group_keys.sync_status(),
        ]);
        NAMES
            .into_iter()
            .zip(statuses)
            .map(|(name, status)| crate::place::PlaceLaneSnapshot {
                name,
                syncing: status.syncing,
                sync_rounds: status.sync_rounds,
                ops_received: status.ops_received,
                last_activity_ms: status.last_activity_ms,
            })
            .collect()
    }

    /// Push one freshly authored graph operation onto the live lane.
    ///
    /// Initial sync covers retained history for a late joiner; an operation
    /// authored while live reaches connected peers only through this. Storing
    /// it is what makes it survive, publishing is what makes it arrive.
    pub(crate) fn publish_graph(
        &self,
        operation: stickleback::Operation<EncryptedCommonsExt>,
    ) -> Result<(), String> {
        self.joined()
            .graph
            .publish(operation)
            .map_err(|error| format!("publish graph operation: {error}"))
    }

    /// Push one freshly authored chat operation onto the live lane.
    pub(crate) fn publish_chat(
        &self,
        operation: stickleback::Operation<ChatExt>,
    ) -> Result<(), String> {
        self.joined()
            .chat
            .publish(operation)
            .map_err(|error| format!("publish chat operation: {error}"))
    }

    /// Push one freshly authored group-key frame onto the live lane, so members
    /// already connected apply an add or a rotation without a new round.
    pub(crate) fn publish_group_key(
        &self,
        operation: stickleback::Operation<GroupKeyExt>,
    ) -> Result<(), String> {
        self.joined()
            .group_keys
            .publish(operation)
            .map_err(|error| format!("publish group-key frame: {error}"))
    }

    /// Push one freshly authored membership operation onto the live lane, so
    /// an admission reaches peers already connected rather than waiting for
    /// whatever restarts reconciliation.
    pub(crate) fn publish_membership(
        &self,
        operation: stickleback::Operation<gemot::moot::MootGroupExt>,
    ) -> Result<(), String> {
        self.joined()
            .moot
            .publish_membership(operation)
            .map_err(|error| format!("publish membership operation: {error}"))
    }

    /// Refuse withdrawn projection grants at this bind's door. A bind that
    /// serves no vault has no door, so there is nothing to refuse.
    pub(crate) fn withdraw_projection_grants(
        &self,
        statements: &[insigne::delegation::SignedDelegationRevocation],
    ) -> Result<(), String> {
        let Some(serving) = &self.projection else {
            return Ok(());
        };
        statements
            .iter()
            .try_for_each(|statement| serving.withdraw(statement))
    }

    /// Push one freshly authored delegation operation onto the live lane.
    pub(crate) fn publish_delegation(
        &self,
        operation: stickleback::Operation<gemot::moot::delegation::MootDelegationExt>,
    ) -> Result<(), String> {
        self.joined()
            .moot
            .publish_delegation(operation)
            .map_err(|error| format!("publish delegation operation: {error}"))
    }
}
