// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's data-only shared-place vocabulary.
//!
//! This module deliberately contains no Gemot, Commons, transport, store, or
//! key types. Those live behind the shell-owned [`worker`] while this module
//! carries the durable public binding and app-owned state used by surfaces.

use serde::{Deserialize, Serialize};

pub(crate) mod captured_collection;
pub(crate) mod contract;
pub mod invite;
#[cfg(feature = "peers")]
pub(crate) mod lanes;
#[cfg(not(feature = "peers"))]
#[path = "disabled/place_lanes.rs"]
pub(crate) mod lanes;
pub mod projection;
#[cfg(feature = "peers")]
pub(crate) mod projection_host;
pub(crate) mod protocol;
pub(crate) mod rendezvous;
#[cfg(feature = "peers")]
pub mod worker;
#[cfg(not(feature = "peers"))]
#[path = "disabled/place_worker.rs"]
pub mod worker;

macro_rules! place_id {
    ($name:ident, $kind:literal) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub [u8; 32]);

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                encode_hex(&value.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = PlaceIdError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                decode_hex($kind, &value).map(Self)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(&encode_hex(&self.0))
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::try_from(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

place_id!(PlaceId, "Moot id");
place_id!(SharedContainerId, "Commons root container id");
place_id!(ChatSpaceId, "Commons chat space id");
place_id!(PlaceCollectionId, "Moot collection id");

/// One member of the place's Gemot membership fold, app-owned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceMember {
    pub root: [u8; 32],
    pub access: PlaceMemberAccess,
}

/// Gemot's membership access levels, without the domain type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceMemberAccess {
    Pull,
    Read,
    Write,
    Manage,
}

/// This profile's own standing in the place at the last refresh.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaceStanding {
    /// Nothing withdrawn: a member, with an active grant or none issued.
    #[default]
    Member,
    /// The membership fold no longer names this profile.
    MembershipRevoked,
    /// A grant to this profile was revoked and none is active.
    GrantRevoked,
    /// Every grant to this profile has expired, the latest at `at_ms`.
    GrantExpired { at_ms: u64 },
}

/// How much of an elided value's head and tail survives. A rendezvous ticket
/// is ~200 characters and a chrome row is one clipped line, so the status
/// surface shows the ends and the palette's copy action carries the whole.
const ELIDE_HEAD: usize = 12;
const ELIDE_TAIL: usize = 8;

/// Cached, app-owned summary of the retained place domains.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OfflinePlaceSnapshot {
    /// This profile's Personae root, as the worker evaluated authority for.
    pub personae_root: [u8; 32],
    /// blake3 over the canonical effective shared-graph projection. Two
    /// converged peers produce the same string; a different one is a real
    /// difference, not a display artifact.
    pub graph_digest: String,
    /// blake3 over the canonical effective chat projection.
    pub chat_digest: String,
    /// Local lane observations at the last worker refresh, not peer liveness
    /// or message delivery. None means this open has no sync lanes.
    pub sync: Option<PlaceSyncSnapshot>,
    /// Local authority evaluation at refresh. Every write checks again.
    pub permissions: Option<PlacePermissionSnapshot>,
    /// What, if anything, has been withdrawn from this profile.
    pub standing: PlaceStanding,
    /// Gemot's membership fold, roots and access.
    pub members: Vec<PlaceMember>,
    pub moot: MootCache,
    pub graph: GraphCache,
    pub chat: ChatCache,
    pub group: GroupCache,
    /// Rebuildable search/read view over this Moot's effective captured-page
    /// contributions and the Fleece records this session currently holds.
    pub captured: CapturedCollectionCache,
    /// The exact collection version used to scope `captured`, or the explicit
    /// reason that version could not currently be projected.
    pub captured_selection: CapturedCollectionSelection,
    /// Current authorized choices, independently of the selected version.
    pub collection_choices: Vec<PlaceCollectionChoice>,
    /// What the shared graph actually holds, as opposed to how much of it.
    /// Already authority-filtered: see [`projection::SharedGraph`].
    pub shared: projection::SharedGraph,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlaceSyncSnapshot {
    pub lanes: Vec<PlaceLaneSnapshot>,
    /// What this bind is serving by projection, or `None` when this profile
    /// has no Knot vault and the place holds no document for anyone.
    ///
    /// An observation like every other field here. A live session count says
    /// somebody is being served right now; it does not say a member can reach
    /// this host, and `refused` names peers turned away, not attackers.
    pub projection: Option<PlaceProjectionSnapshot>,
    /// This bind's own dialable ticket(s). What a peer needs to reach here;
    /// holding one says nothing about whether anyone did.
    pub local_rendezvous: Vec<String>,
    /// How many peer tickets this bind dialed. Zero is listen-only.
    pub dialed_rendezvous: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceProjectionSnapshot {
    pub live_sessions: u32,
    pub refused: u64,
    /// The catalog route being served, `knot` today.
    pub route: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacePermissionSnapshot {
    pub message_write: bool,
    pub graph_write: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceLaneSnapshot {
    pub name: &'static str,
    pub syncing: bool,
    pub sync_rounds: u64,
    pub ops_received: u64,
    pub last_activity_ms: Option<u64>,
}

/// Local view choice for captured-page search. This is not a Gemot fact.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CapturedCollectionSelection {
    /// Search every currently effective captured-page contribution in the Moot.
    #[default]
    AllEffective,
    /// Search only one exact, historically identified collection version.
    Collection {
        requested: PlaceCollectionVersion,
        status: CapturedCollectionSelectionStatus,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapturedCollectionSelectionStatus {
    Ready {
        name: String,
        effective_contributions: usize,
        pending_facts: usize,
    },
    /// The named collection exists, but its current causal version has moved.
    Stale { current: PlaceCollectionVersion },
    /// The collection has no currently authorized projection.
    Unavailable,
    /// The requested version belongs to another Moot.
    ForeignMoot,
}

/// App-owned captured-page view. Gemot and Eidetic remain authoritative; this
/// cache can be discarded and rebuilt from both at any time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapturedCollectionCache {
    pub groups: Vec<CapturedContentGroup>,
    pub rejected: Vec<CapturedContributionStatus>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedContentGroup {
    pub extraction_schema: String,
    pub normalization: String,
    pub reader_profile: String,
    pub canonical_text_hash: String,
    pub canonical_text_iri: String,
    pub canonical_text: String,
    pub contributions: Vec<CapturedContribution>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedContribution {
    pub share_operation: [u8; 32],
    pub annotation_manifest: [u8; 32],
    pub contributor: [u8; 32],
    pub source: String,
    pub title: String,
    pub shared_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedContributionStatus {
    pub share_operation: [u8; 32],
    pub annotation_manifest: [u8; 32],
    pub title: String,
    pub reason: CapturedRejectionReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapturedRejectionReason {
    UnsupportedSchema(String),
    MissingRecord,
    InvalidRecord(String),
    LibraryUnavailable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedSearchHit {
    pub canonical_text_hash: String,
    pub title: String,
    pub source: String,
    pub contribution_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MootCache {
    pub membership_epoch: u64,
    pub members: usize,
    pub roster_members: usize,
    pub delegated_certificates: usize,
    pub standing_operations: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphCache {
    pub nodes: usize,
    pub edges: usize,
    pub pending_causality: usize,
    pub pending_authority: usize,
    pub revoked_authority: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatCache {
    pub channels: usize,
    pub messages: usize,
    pub deleted_messages: usize,
    pub pending_causality: usize,
    pub pending_authority: usize,
    pub revoked_authority: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupCache {
    pub members: usize,
    pub epochs: usize,
    pub has_current_epoch: bool,
    /// Group-key frames the last drain could not apply yet.
    pub pending_frames: usize,
    /// Group-key frames refused since this open.
    pub refused_frames: usize,
    /// Chat and shared-node records held for an epoch this profile does not
    /// hold yet, read at this refresh.
    pub parked_records: u64,
    /// Parked records admitted since this open, once their epoch arrived.
    pub readmitted_records: u64,
    /// Parked records dropped for good at the parking bound. Durable, so it
    /// counts every open, not just this one.
    pub evicted_records: u64,
}

/// Current product-visible state of the session's shared place.
///
/// Every worker-backed state carries the generation that opened it. A service
/// answer from a departed session or an earlier open therefore cannot mutate
/// the active app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceState {
    Personal,
    /// An invitation is being admitted.
    ///
    /// Deliberately carries no binding. The envelope names one, but until every
    /// admission check has answered there is no admitted place, and app state
    /// claiming a binding here would be the same conflation the persistence
    /// ordering exists to prevent.
    Joining {
        generation: u64,
    },
    Opening {
        binding: PlaceBindingV1,
        generation: u64,
    },
    Offline {
        binding: PlaceBindingV1,
        generation: u64,
        snapshot: OfflinePlaceSnapshot,
    },
    Degraded {
        binding: PlaceBindingV1,
        generation: u64,
        error: String,
    },
    Failed {
        error: String,
    },
    /// This session left a place: the worker released, the binding, contact
    /// hints and every retained store kept on disk under a left mark
    /// (`session::PLACE_LEFT_FILE`), so a restart reads `Left` again and
    /// `Rejoin` has what it needs. Otherwise behaves like `Personal`
    /// everywhere (founding, joining and opening a card are all valid from
    /// here) except its own status wording and the coop contract's
    /// `Verdict::Left`.
    Left {
        binding: PlaceBindingV1,
        retained: PlaceLeftSummary,
    },
}

/// What a departed session's stores held, read once from the `Offline`
/// snapshot in hand at the moment of leaving. Never re-read afterward:
/// `Left` does not reopen the stores itself, so this can go stale the way
/// every other "last refresh" fact here can.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlaceLeftSummary {
    pub graph_nodes: usize,
    pub chat_messages: usize,
    pub members: usize,
}

#[cfg(test)]
#[cfg(feature = "peers")]
#[path = "place/tests.rs"]
mod tests;

#[path = "place/encode_hex.rs"]
mod encode_hex;
use encode_hex::encode_hex;
#[path = "place/decode_hex.rs"]
mod decode_hex;
use decode_hex::decode_hex;
#[path = "place/hex32.rs"]
mod hex32;
#[path = "place/impl_std_error_error_for_place.rs"]
mod impl_std_error_error_for_place;
#[path = "place/impl_std_error_error_for_place_2.rs"]
mod impl_std_error_error_for_place_2;
#[path = "place/impl_std_fmt_display_for_place.rs"]
mod impl_std_fmt_display_for_place;
#[path = "place/impl_std_fmt_display_for_place_2.rs"]
mod impl_std_fmt_display_for_place_2;
#[path = "place/place_binding_v1_new.rs"]
mod place_binding_v1_new;
#[path = "place/place_card_v1_validate.rs"]
mod place_card_v1_validate;
#[path = "place/place_prekey_offer_v1_prekey_bytes.rs"]
mod place_prekey_offer_v1_prekey_bytes;
pub use hex32::hex32;
#[path = "place/utc_ms.rs"]
mod utc_ms;
pub use utc_ms::utc_ms;
#[path = "place/split_invite_lifetime.rs"]
mod split_invite_lifetime;
pub use split_invite_lifetime::split_invite_lifetime;
#[path = "place/elide_middle.rs"]
mod elide_middle;
#[path = "place/place_member_access_label.rs"]
mod place_member_access_label;
#[path = "place/place_standing_status_line.rs"]
mod place_standing_status_line;
pub use elide_middle::elide_middle;
#[path = "place/captured_collection_cache_search.rs"]
mod captured_collection_cache_search;
#[path = "place/impl_default_for_place_state.rs"]
mod impl_default_for_place_state;
#[path = "place/place_binding_version.rs"]
mod place_binding_version;
#[path = "place/place_lane_snapshot_label.rs"]
mod place_lane_snapshot_label;
#[path = "place/place_state_status_lines.rs"]
mod place_state_status_lines;
pub use place_binding_version::PLACE_BINDING_VERSION;
#[path = "place/place_id_error.rs"]
mod place_id_error;
pub use place_id_error::PlaceIdError;
#[path = "place/place_collection_version.rs"]
mod place_collection_version;
pub use place_collection_version::PlaceCollectionVersion;
#[path = "place/place_collection_choice.rs"]
mod place_collection_choice;
pub use place_collection_choice::PlaceCollectionChoice;
#[path = "place/place_binding_v1.rs"]
mod place_binding_v1;
pub use place_binding_v1::PlaceBindingV1;
#[path = "place/place_binding_error.rs"]
mod place_binding_error;
pub use place_binding_error::PlaceBindingError;
#[path = "place/place_card_version.rs"]
mod place_card_version;
pub use place_card_version::PLACE_CARD_VERSION;
#[path = "place/place_card_v1.rs"]
mod place_card_v1;
pub use place_card_v1::PlaceCardV1;
#[path = "place/place_prekey_offer_v1.rs"]
mod place_prekey_offer_v1;
pub use place_prekey_offer_v1::PlacePrekeyOfferV1;
#[path = "place/place_invite_access.rs"]
mod place_invite_access;
pub use place_invite_access::PlaceInviteAccess;
