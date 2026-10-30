// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl OpenPlace {
    pub(super) fn offers_projection(&self) -> bool {
        self.lanes
            .as_ref()
            .and_then(|lanes| lanes.projection_snapshot())
            .is_some()
    }

    /// Drain the group-key lane, then re-admit what the old key handle could
    /// not read. Every drain goes through here: a rotation that arrives with
    /// the content sealed to it is the race this closes.
    pub(crate) fn follow_group_keys(
        &mut self,
        identity: &dyn IdentityProvider,
    ) -> Result<(), String> {
        self.group.drain(&self.directory, identity)?;
        self.readmit_parked()
    }

    /// Offer both encrypted lanes their parked records again, chat first.
    /// Called after every key-handle refresh, drains and local rotations
    /// alike; a record whose epoch is still unheld simply stays parked.
    pub(crate) fn readmit_parked(&mut self) -> Result<(), String> {
        let chat = pollster::block_on(self.chat.readmit_parked())
            .map_err(|error| format!("re-admit parked messages: {error}"))?;
        let graph = pollster::block_on(self.graph.readmit_parked())
            .map_err(|error| format!("re-admit parked shared nodes: {error}"))?;
        self.readmitted += chat.admitted + graph.admitted;
        if chat.admitted + graph.admitted + chat.refused + graph.refused > 0 {
            tracing::info!(
                chat_admitted = chat.admitted,
                graph_admitted = graph.admitted,
                chat_refused = chat.refused,
                graph_refused = graph.refused,
                still_parked = chat.still_parked + graph.still_parked,
                "re-admitted parked records after a key-handle refresh"
            );
        }
        Ok(())
    }

    /// What both encrypted lanes hold parked right now, and how many records
    /// they have evicted for good. Read at snapshot time rather than carried
    /// from the last re-admission, because a live lane arrival parks between
    /// passes.
    pub(crate) fn parking(&self) -> Result<(u64, u64), String> {
        let chat = pollster::block_on(self.chat.parking_status())
            .map_err(|error| format!("read parked messages: {error}"))?;
        let graph = pollster::block_on(self.graph.parking_status())
            .map_err(|error| format!("read parked shared nodes: {error}"))?;
        Ok((
            chat.parked + graph.parked,
            chat.evicted_total + graph.evicted_total,
        ))
    }
}
