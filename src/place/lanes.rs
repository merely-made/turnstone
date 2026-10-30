// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's live-lane composition: dial a ticket, hold ten handles.
//!
//! The shell-owned counterpart to the domain join helpers. Everything here is
//! composition: the transport crate owns dialing and overlay tagging, Gemot
//! owns its seven lanes, Commons owns graph and chat, Stickleback owns the
//! group-key lane, and this module only decides what a *place* joins and in
//! what order it lets go. Session and transport identity never become content
//! authority; every accept closure runs the owning domain's admission, and
//! projections stay authority-filtered exactly as they are offline.

use std::sync::Arc;

use commons::chat::ChatExt;
use commons::encrypted::{COMMONS_ENCRYPTED_GRAPH_LANE, EncryptedCommonsExt};
use gemot::moot::MootLanes;
use graphshell_host::admission::open_session;
use graphshell_host::network_carrier::{
    CarrierRuntime, NetworkCarrier, dial_projection_session, projection_binding,
};
use identity::IdentityProvider;
use identity::delegation::Issue;
use insigne::delegation::{DelegationCertificate, DelegationParent, SignedDelegationCertificate};
use notochord::{HandshakeLimits, NetworkId, TrafficClass};
use stickleback::{GroupKeyExt, JoinedSpace};
use transport::p2panda_transport::MdnsDiscoveryMode;
use transport::{P2pandaStream, P2pandaTransport, PeerID, Transport, sync_overlay_topic};

use crate::place::PlaceBindingV1;
use crate::place::projection_host::{
    ProjectionServing, ProjectionSetup, ProjectionWatchCounters, projection_profile,
    projection_scope,
};
use crate::place::worker::{OpenPlace, ProviderRef};

/// How often the watcher samples lane counters, and how long they must hold
/// steady before it reports. A burst of operations from one sync round then
/// settles into a single re-fold instead of one per message.
const WATCH_TICK: std::time::Duration = std::time::Duration::from_millis(250);

/// How long a close waits for all ten lanes to let go of their store clones.
const LEAVE_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// One place's joined lanes, plus the transport and runtime that carry them.
///
/// The runtime is retained so the synchronous owner can await watcher
/// cancellation and endpoint close before this value's fields are dropped.
pub(crate) struct LiveLanes {
    watcher: Option<tokio::task::JoinHandle<()>>,
    /// This bind's own dialable ticket(s), read once the endpoint is up.
    local_rendezvous: Vec<String>,
    /// How many peer tickets this bind dialed. Zero is a listen-only bind.
    dialed_rendezvous: usize,
    /// Taken in `drop`: each lane is left and awaited by value, which is the
    /// only thing that releases the store clone its sync actor captured.
    joined: Option<JoinedLanes>,
    /// This place's Moot id, so a visitor's dial can name the same network the
    /// holder's policy does without carrying the whole binding.
    moot: [u8; 32],
    /// The vault host, when this profile has a vault to serve. Held here
    /// because its accept loop borrows the transport and must stop first.
    projection: Option<ProjectionServing>,
    /// Shared because the projection accept loop outlives any one call and
    /// `accept_one` borrows its transport; `drop` aborts that loop first.
    _transport: Arc<P2pandaTransport>,
    /// Taken in `drop` so the runtime can be shut down with a timeout: every
    /// task holding a store clone must be gone before the caller reopens.
    _runtime: Option<tokio::runtime::Runtime>,
}

/// The ten joined lanes, held together so `drop` can move them out.
struct JoinedLanes {
    moot: MootLanes,
    graph: JoinedSpace<EncryptedCommonsExt>,
    chat: JoinedSpace<ChatExt>,
    group_keys: JoinedSpace<GroupKeyExt>,
}

/// Shared counter handles across all ten lanes plus the projection host,
/// sampled by the watcher.
struct LaneCounters {
    handles: Vec<std::sync::Arc<std::sync::Mutex<stickleback::SyncStatus>>>,
    /// None when this profile has no vault, so there is no host to count.
    projection: Option<ProjectionWatchCounters>,
}

#[cfg(test)]
#[path = "lanes/tests.rs"]
mod tests;

/// Domain-separated salt for this place's transport identity. A derived key,
/// never the master: the transport peer id is session machinery, and the
/// worker's projections must stay exactly as valid if it changes.
/// The carrier a visitor mounts a holder's document over: mere's network
/// carrier on this place's own p2panda stream.
pub(crate) type PlaceProjectionCarrier = NetworkCarrier<P2pandaStream>;

/// A dial that has been prepared but not yet opened.
///
/// Everything a stream needs, with no borrow of [`LiveLanes`] left in it, so
/// the worker can prepare a dial on its own thread and a visiting thread can
/// open it on its. The carrier is produced by [`Self::open`] and stays on
/// whichever thread called it, which is the whole reason this split exists:
/// the retained session over that carrier is not `Send`, so the thread that
/// opens the stream must be the thread that reads it.
///
/// The runtime handle is a clone of the lane runtime's, and that runtime
/// lives in the worker's `LiveLanes`. It outlives every visit by ordering,
/// not by ownership: the shell drops its visiting hubs before the place is
/// released. If a runtime does go first, the carrier's next request fails and
/// the visited surface reports unavailable, which is the same thing a lost
/// holder looks like and the correct thing to show either way.
pub(crate) struct HolderDial {
    transport: Arc<P2pandaTransport>,
    handle: tokio::runtime::Handle,
    hello: notochord::SessionHello,
    ticket: String,
}

#[path = "lanes/dial_holder.rs"]
mod dial_holder;
#[path = "lanes/impl_drop_for_live_lanes.rs"]
mod impl_drop_for_live_lanes;
#[path = "lanes/joined_lanes_leave_and_wait.rs"]
mod joined_lanes_leave_and_wait;
#[path = "lanes/lane_counters_total.rs"]
mod lane_counters_total;
#[path = "lanes/live_lanes_leave_and_wait.rs"]
mod live_lanes_leave_and_wait;
pub(crate) use dial_holder::dial_holder;
#[path = "lanes/holder_dial.rs"]
mod holder_dial;
#[path = "lanes/holder_dial_open.rs"]
mod holder_dial_open;
pub(crate) use holder_dial::holder_dial;
#[path = "lanes/group_key_overlay.rs"]
mod group_key_overlay;
use group_key_overlay::group_key_overlay;
#[path = "lanes/transport_salt.rs"]
mod transport_salt;
use transport_salt::transport_salt;
#[path = "lanes/join_live.rs"]
mod join_live;
pub(crate) use join_live::join_live;
