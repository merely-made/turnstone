// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl JoinedLanes {
    /// Leave every lane and wait for its drain and sync actor to let go.
    ///
    /// Concurrently and under a bound: each lane's LogSync shutdown takes
    /// seconds on its own, and the caller is a person who clicked reconnect.
    pub(super) async fn leave_and_wait(self) {
        let moot = self.moot;
        let left = [
            tokio::spawn(moot.constitution.leave_and_wait()),
            tokio::spawn(moot.delegation.leave_and_wait()),
            tokio::spawn(moot.membership.leave_and_wait()),
            tokio::spawn(moot.records.leave_and_wait()),
            tokio::spawn(moot.standing.leave_and_wait()),
            tokio::spawn(moot.tulpa.leave_and_wait()),
            tokio::spawn(moot.flora.leave_and_wait()),
        ];
        let graph = tokio::spawn(self.graph.leave_and_wait());
        let chat = tokio::spawn(self.chat.leave_and_wait());
        let group_keys = tokio::spawn(self.group_keys.leave_and_wait());
        let all = async move {
            for lane in left {
                if let Ok(Err(error)) = lane.await {
                    tracing::warn!(%error, "place lane did not leave cleanly");
                }
            }
            let _ = graph.await;
            let _ = chat.await;
            let _ = group_keys.await;
        };
        if tokio::time::timeout(LEAVE_BUDGET, all).await.is_err() {
            tracing::warn!("place lanes did not all leave within the budget");
        }
    }
}
