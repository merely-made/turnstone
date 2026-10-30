// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The shell's fetch ingress: Mere's bounded actor, or the offline receiver.

use fetch::FetchCommand;
use std::time::Duration;

pub(crate) enum FetchPort {
    /// Capacity is reserved before enqueue; refusals answer with the
    /// command's correlated failure update.
    Network(fetch::FetchActor),
    Offline(armillary::ActorHandle<FetchCommand>),
    Closed,
}

impl FetchPort {
    /// False means the command was not admitted.
    pub(crate) fn command(&self, command: FetchCommand) -> bool {
        match self {
            Self::Network(actor) => actor.command(command),
            Self::Offline(handle) => handle.command(command),
            Self::Closed => false,
        }
    }

    /// Fence ingress at once, then wait up to `within` for every fetch task and
    /// the worker to join. The join runs on its own thread, so a transport stuck
    /// in blocking work delays exit by at most `within`.
    pub(crate) fn close_and_join(&mut self, within: Duration) {
        let port = std::mem::replace(self, Self::Closed);
        if matches!(port, Self::Closed) {
            return;
        }
        let (done, joined) = std::sync::mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("fetch-join".into())
            .spawn(move || {
                let _ = done.send(match port {
                    Self::Network(actor) => actor.close_and_join().map(Some),
                    Self::Offline(handle) => handle.join().map(|()| None),
                    Self::Closed => Ok(None),
                });
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "fetch join thread could not start");
            return;
        }
        match joined.recv_timeout(within) {
            Ok(Ok(Some(report))) => tracing::debug!(?report, "fetch actor joined"),
            Ok(Ok(None)) => {},
            Ok(Err(error)) => tracing::warn!(%error, "fetch actor join failed"),
            Err(_) => tracing::warn!("fetch actor join timed out at exit"),
        }
    }
}
