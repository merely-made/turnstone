// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::{PlaceBindingV1, PlaceCollectionVersion, invite::PlaceInviteV1};
use crate::{identity::RootIdentity, panes::SessionId};
use std::{path::PathBuf, sync::Arc};
/// Host-set evaluation time for converged authority.
///
/// Delegation grants and revocations carry absolute windows, so this value is
/// what decides whether an unauthorized operation reads as pending or revoked.
/// It is a host input on purpose: session, relay, and transport identity never
/// enter the decision. Tests pin it so a verdict is reproducible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityClock {
    SystemTime,
    Fixed(u64),
}

/// Runtime settings for the offline worker. The default is deliberately
/// conservative: opening cached state never proposes expiry or erasure.
#[derive(Clone, Debug)]
pub struct PlaceWorkerSettings {
    #[cfg(feature = "peers")]
    pub retention: gemot::moot::MootRetentionSettings,
    pub authority_clock: AuthorityClock,
    pub(crate) capture_library: crate::place::captured_collection::LocalCaptureLibrary,
    /// The Knot vault a live place serves by projection, or `None` when this
    /// profile has no vault and the place serves no document.
    ///
    /// Read from the environment here, beside the clock and the retention
    /// policy, because a vault root is a host input. The lanes module never
    /// reads it: it receives a [`crate::place::projection_host::ProjectionSetup`]
    /// or nothing at all.
    pub knot_root: Option<PathBuf>,
    /// The write grant's ceiling for that vault, matching directory mode.
    pub knot_max_source_bytes: u64,
}

/// Commands accepted by the one shell-owned place worker.
pub enum PlaceWorkerCommand {
    Open {
        session: SessionId,
        generation: u64,
        directory: PathBuf,
        binding: PlaceBindingV1,
    },
    Join {
        session: SessionId,
        generation: u64,
        directory: PathBuf,
        invite: Box<PlaceInviteV1>,
    },
    /// Reopen retained state and reconnect using the admitted rendezvous
    /// descriptor. This never replays invitation welcome material.
    Reconnect {
        session: SessionId,
        generation: u64,
        directory: PathBuf,
        binding: PlaceBindingV1,
        /// Optional explicit contact-only refresh for this same retained place.
        contact: Option<Box<crate::place::PlaceCardV1>>,
    },
    /// Found a new place in this session's directory and open it live,
    /// listen-only. Nothing is dialed: the founder has invited nobody yet.
    Found {
        session: SessionId,
        generation: u64,
        directory: PathBuf,
        name: String,
    },
    /// Publish this profile's group identity for one Moot named by a card.
    /// Authority-free: a pre-key is an offer, not an admission.
    OfferPrekey {
        session: SessionId,
        generation: u64,
        directory: PathBuf,
        moot: [u8; 32],
    },
    /// Admit one offered pre-key's root and author its invitation, carrying
    /// this open's own live rendezvous. `lifetime_ms` bounds a writer's
    /// grants; `None` issues them without expiry.
    Invite {
        session: SessionId,
        generation: u64,
        directory: PathBuf,
        prekey: Vec<u8>,
        access: crate::place::PlaceInviteAccess,
        lifetime_ms: Option<u64>,
    },
    /// Re-fold the open place's projections without touching its lanes.
    ///
    /// The live lanes drain received operations straight into the retained
    /// stores; this is how what arrived becomes authority-filtered,
    /// product-visible state. Answered with [`Update::PlaceOpened`] under the
    /// same generation, so the app folds it exactly like an open.
    Resync {
        session: SessionId,
        generation: u64,
    },
    /// Set or clear the exact local collection version used for captured search.
    SetCollection {
        session: SessionId,
        generation: u64,
        selection: Option<PlaceCollectionVersion>,
    },
    /// Author one fact into the shared place and push it to live peers.
    Author {
        session: SessionId,
        generation: u64,
        request: u64,
        command: PlaceCommand,
    },
    /// Prepare a dial to the mere holding a place-held Knot document.
    ///
    /// Answered with [`Update::PlaceDocumentVisit`]. The worker only prepares
    /// the dial: it spends the stored grant's delegation hop and signs the
    /// hello, because that is where the identity provider is, and hands back
    /// something Send. Opening the stream and reading it belong to one
    /// visiting thread, because the retained session over that carrier is not
    /// `Send`.
    VisitDocument {
        session: SessionId,
        generation: u64,
        directory: PathBuf,
        holder_root: [u8; 32],
        path: String,
        request: u64,
    },
    Release(std::sync::mpsc::SyncSender<()>),
    /// The vault unlocked: the profile root a pending worker was started
    /// without. The commands it held run now, in the order they came.
    AdoptIdentity(Arc<RootIdentity>),
    /// Permanently withdraw the current profile root from this worker and
    /// release its open place before a fresh binding may be adopted.
    RetireIdentity(std::sync::mpsc::SyncSender<()>),
    /// Stop serving the way a killed process stops: no close on the wire.
    /// Test-only, because nothing a person does to this app produces it.
    #[cfg(test)]
    Abandon(std::sync::mpsc::SyncSender<()>),
    /// Stop answering without closing anything, the way a frozen machine
    /// stops. Test-only.
    #[cfg(test)]
    Freeze(std::time::Duration, std::sync::mpsc::SyncSender<()>),
}

/// One authored change to the shared place.
///
/// Deliberately small and concrete. Every variant is something a person does
/// in a place; none is a generic "write this operation", because a host that
/// can post arbitrary operations has taken over authorship from the domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceCommand {
    SendMessage {
        channel: String,
        body: String,
    },
    ShareNode {
        address: String,
    },
    /// Remove one member and revoke every grant this profile issued to it.
    RevokeMember {
        member: [u8; 32],
    },
}

#[cfg(feature = "peers")]
use gemot::moot::MootRetentionSettings;
#[cfg(feature = "peers")]
use gemot::moot::{AvailabilityPolicy, ErasurePolicy, KeepBound, PolicyRevision};
#[cfg(feature = "peers")]
use proofs::Digest;
const DEFAULT_KNOT_MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
#[path = "worker/authority_clock_now_ms.rs"]
mod clock;
#[path = "worker/impl_default_for_place_worker_settings.rs"]
mod defaults;
