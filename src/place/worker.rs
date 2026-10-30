// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Shell-owned retained-place worker.
//!
//! This is the product composition boundary. It opens Gemot, Commons graph,
//! Commons chat, and Stickleback group state for one Turnstone session, then
//! emits only app-owned summaries tagged with the session and open generation.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use armillary::{ActorHandle, Emitter, Wake, spawn_named};
use commons::chat::ChatReplica;
use commons::encrypted::EncryptedReplica;
use commons::{GemotAuthorityView, GroupKeys};
use gemot::moot::constitution::{CapabilityGrant, ConstitutionRules};
use gemot::moot::{
    AvailabilityPolicy, CollectionEvent, CollectionId, CollectionRef, CollectionVersion,
    ErasurePolicy, KeepBound, MOOT_ACT_ACTION, MOOT_DELEGATION_DOMAIN, MootAccessLevel,
    MootAuthority, MootFile, MootId, MootMember, MootMembershipAction, MootRetentionSettings,
    PolicyRevision,
};
use identity::delegation::Issue;
use identity::{IdentityProvider, SealedRecordStorage};
use insigne::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, DelegationRevocation,
    SignedDelegationCertificate, SignedDelegationRevocation, delegation_signing_salt,
};
use muniment::RedbBackend;
use proofs::Digest;
use servitor::{Cap, cap_path};
use stickleback::{
    DataKeyring, DropExportProfile, DropLimits, GroupControlFrame, GroupDirectFrame, GroupKeyExt,
    GroupKeyLane, GroupPrekeyBundle, GroupSession, GroupSessionDispatch, GroupSessionId,
};

use crate::action::Update;
use crate::identity::RootIdentity;
use crate::panes::SessionId;
use crate::place::invite::PlaceInviteV1;
use crate::place::{
    CapturedCollectionSelection, CapturedCollectionSelectionStatus, ChatCache, GraphCache,
    GroupCache, MootCache, OfflinePlaceSnapshot, PlaceBindingV1, PlaceCollectionChoice,
    PlaceCollectionId, PlaceCollectionVersion, PlaceId,
};

const GROUP_SESSION_RECORD: &str = "group.session";
/// This profile's OWN published pre-key bundle, which every invitation it
/// authors carries as `inviter_prekey`. Kept sealed on the side because it is
/// the publishable half of a secret the session never exports; the per-root
/// copies this once kept beside it are gone, the group session's own
/// `recipient_for_root` having replaced them.
const GROUP_PREKEY_RECORD: &str = "group.prekey";

/// Why a revoke stops before it touches Gemot: without a recipient there is
/// nothing to remove from the group, and revoking membership alone would
/// leave that member still reading.
pub(crate) const NO_GROUP_RECIPIENT: &str = "that member has no group recipient to remove";

/// The group-key lane's store.
pub(crate) const GROUP_KEY_STORE: &str = "group-keys.redb";
/// The encrypted shared graph's store. A name of its own, so a plaintext
/// store from before encryption is never read as an encrypted one.
pub(crate) const GRAPH_STORE: &str = "commons-graph-encrypted.redb";
/// The plaintext graph store places kept before encryption.
pub(crate) const LEGACY_GRAPH_STORE: &str = "commons-graph.redb";
pub(crate) const LEGACY_GRAPH_REFUSAL: &str = "this place predates encrypted shared graphs";

/// How long an authored invitation stays usable: seven days. Long enough to
/// hand over out of band, short enough that a forwarded envelope dies.
const INVITE_LIFETIME_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// The same ceiling `knot_authoring`'s directory mode uses. Restated rather
/// than shared because that module reads the whole `TURNSTONE_KNOT_*` set for
/// the authoring surface, and the place serves the vault, not the surface.
const DEFAULT_KNOT_MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;

pub use super::protocol::*;

pub(crate) struct OpenPlace {
    /// Session-owned view sidecars are written only by this worker.
    pub(crate) directory: PathBuf,
    /// Live lane handles, when this open dialed. First live field: the
    /// lane tasks must stop before the stores they drain into close.
    pub(crate) lanes: Option<crate::place::lanes::LiveLanes>,
    /// The binding this place was opened under, so a later command names the
    /// same scopes the projections fold from.
    pub(crate) binding: PlaceBindingV1,
    /// Stable local root used for the refresh-time capability projection.
    /// Writes still recheck the current identity and authority immediately
    /// before authoring.
    pub(crate) subject: [u8; 32],
    /// Session-scoped capture library expected for this open. This prevents a
    /// cross-actor session switch from briefly resolving against the departed
    /// session's in-memory records.
    pub(crate) capture_directory: PathBuf,
    /// Exact local view state restored before the first capture projection.
    pub(crate) collection_selection: Option<PlaceCollectionVersion>,
    pub(crate) moot: MootFile,
    pub(crate) graph: EncryptedReplica<RedbBackend>,
    pub(crate) chat: ChatReplica<RedbBackend>,
    pub(crate) group: PlaceGroup,
    /// Records re-admitted since this open, both encrypted lanes together.
    pub(crate) readmitted: u64,
}

/// The place's one live group: the session, the key handle chat and the graph
/// read at every admission, and the lane carrying every member's frames.
///
/// Invites and revocations change `session` only through [`Self::commit`],
/// and received frames only through [`Self::drain`]; both refresh `keys`, so
/// a joined lane follows a rotation without rejoining.
pub(crate) struct PlaceGroup {
    pub(crate) session: GroupSession,
    pub(crate) keys: GroupKeys,
    pub(crate) lane: GroupKeyLane<RedbBackend>,
    /// Frames the last drain could not apply yet.
    pub(crate) pending: usize,
    /// Frames refused since this open; refused frames are settled, not retried.
    pub(crate) refused: usize,
}

/// The channel a founded place opens with. Renaming is a later product
/// decision; founding without one would leave nowhere to speak.
pub const DEFAULT_CHANNEL: &str = "general";

/// `SignedDelegationCertificate::issue` takes a sized provider and the worker
/// holds a trait object. Borrowing through this bridges the two without
/// copying key material or widening any public signature.
pub(crate) struct ProviderRef<'a>(pub(crate) &'a dyn IdentityProvider);

/// What one admission authored. Any may be absent: a root already admitted
/// authors no membership fact, and a reader gets neither delegation nor grant.
#[derive(Default)]
struct AdmittedOps {
    membership: Option<stickleback::Operation<gemot::moot::MootGroupExt>>,
    delegation: Option<stickleback::Operation<gemot::moot::delegation::MootDelegationExt>>,
    /// The projection-connect grant a writer's invitation carries. Not a
    /// Gemot operation and never published on a lane: it travels in the
    /// envelope, because the door it opens is this host's, not the Moot's.
    projection: Option<SignedDelegationCertificate>,
}

/// One admitted place. Deliberately carries no store handle, key, or frame:
/// the caller persists the binding and reopens through the ordinary path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmittedPlace {
    pub binding: PlaceBindingV1,
    /// Governance membership: Gemot's converged fold.
    pub moot: MootCache,
    /// Crypto membership: the DCGKA group this welcome joined.
    ///
    /// Deliberately not the same number as `moot.members`, and not asserted
    /// equal to it. They are different folds and can legitimately disagree the
    /// moment a join completes, because the joiner has processed exactly one
    /// welcome while Gemot's evidence may already carry later membership. A
    /// lasting divergence is worth surfacing; an immediate one is normal.
    pub group_members: usize,
}

/// How long a same-process reopen waits out the store lock a closed bind
/// still holds. p2panda runs each LogSync session's actor on its own
/// `std::thread` (ractor `ThreadLocalActorSpawner`), so the store clone in
/// that actor's state outlives both the awaited lane leave and the lane
/// runtime's shutdown by a few hundred milliseconds.
pub(crate) const RECONNECT_REOPEN_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

#[cfg(test)]
#[path = "worker/tests.rs"]
pub(crate) mod tests;

#[path = "worker/keyring_state.rs"]
mod keyring_state;
#[path = "worker/open_place_offers_projection.rs"]
mod open_place_offers_projection;
#[path = "worker/place_group_open.rs"]
mod place_group_open;
#[path = "worker/place_worker_settings_projection_setup.rs"]
mod place_worker_settings_projection_setup;
use keyring_state::keyring_state;
#[path = "worker/place_store_dir.rs"]
mod place_store_dir;
pub use place_store_dir::place_store_dir;
#[path = "worker/place_secrets_dir.rs"]
mod place_secrets_dir;
pub use place_secrets_dir::place_secrets_dir;
#[path = "worker/place_secret_salt.rs"]
mod place_secret_salt;
use place_secret_salt::place_secret_salt;
#[path = "worker/sealed_group_store.rs"]
mod sealed_group_store;
use sealed_group_store::sealed_group_store;
#[path = "worker/invite_drop_limits.rs"]
mod invite_drop_limits;
use invite_drop_limits::invite_drop_limits;
#[path = "worker/admit_invitation.rs"]
mod admit_invitation;
pub use admit_invitation::admit_invitation;
#[path = "worker/prepare_group_identity.rs"]
mod prepare_group_identity;
pub use prepare_group_identity::prepare_group_identity;
#[path = "worker/author_invitation.rs"]
mod author_invitation;
pub use author_invitation::author_invitation;
#[path = "worker/author_invitation_with.rs"]
mod author_invitation_with;
pub(crate) use author_invitation_with::author_invitation_with;
#[path = "worker/sealed_prekey_bytes.rs"]
mod sealed_prekey_bytes;
use sealed_prekey_bytes::sealed_prekey_bytes;
#[path = "worker/inline_artifact.rs"]
mod inline_artifact;
use inline_artifact::inline_artifact;
#[path = "worker/found_place_group.rs"]
mod found_place_group;
pub use found_place_group::found_place_group;
#[path = "worker/place_tag.rs"]
mod place_tag;
pub(crate) use place_tag::place_tag;
#[path = "worker/founding_salt.rs"]
mod founding_salt;
use founding_salt::founding_salt;
#[path = "worker/root_grant_id.rs"]
mod root_grant_id;
pub(crate) use root_grant_id::root_grant_id;
#[path = "worker/place_capability_prefix.rs"]
mod place_capability_prefix;
pub use place_capability_prefix::place_capability_prefix;
#[path = "worker/place_scope.rs"]
mod place_scope;
pub use place_scope::place_scope;
#[path = "worker/place_rules.rs"]
mod place_rules;
pub use place_rules::place_rules;
#[path = "worker/founder_signing_key.rs"]
mod founder_signing_key;
pub use founder_signing_key::founder_signing_key;
#[path = "worker/place_delegation.rs"]
mod place_delegation;
pub use place_delegation::place_delegation;
#[path = "worker/impl_identity_provider_for_provider_ref.rs"]
mod impl_identity_provider_for_provider_ref;
#[path = "worker/random_id.rs"]
mod random_id;
use random_id::random_id;
#[path = "worker/found_place.rs"]
mod found_place;
pub use found_place::found_place;
#[path = "worker/admit_member.rs"]
mod admit_member;
use admit_member::admit_member;
#[path = "worker/load_group_session.rs"]
mod load_group_session;
pub(crate) use load_group_session::load_group_session;
#[path = "worker/admit_inner.rs"]
mod admit_inner;
use admit_inner::admit_inner;
#[path = "worker/save_group_session.rs"]
mod save_group_session;
pub use save_group_session::save_group_session;
#[path = "worker/reopen_cached_place.rs"]
mod reopen_cached_place;
use reopen_cached_place::reopen_cached_place;
#[path = "worker/open_cached_place.rs"]
mod open_cached_place;
pub(crate) use open_cached_place::open_cached_place;
#[path = "worker/author_into_place.rs"]
mod author_into_place;
use author_into_place::author_into_place;
#[path = "worker/local_standing.rs"]
mod local_standing;
use local_standing::local_standing;
#[path = "worker/revoke_member.rs"]
mod revoke_member;
use revoke_member::revoke_member;
#[path = "worker/rekey_without.rs"]
mod rekey_without;
use rekey_without::rekey_without;
#[path = "worker/withdrawn_projection_grants.rs"]
mod withdrawn_projection_grants;
pub(crate) use withdrawn_projection_grants::withdrawn_projection_grants;
#[path = "worker/visit_dial.rs"]
mod visit_dial;
use visit_dial::visit_dial;
#[path = "worker/canonical_chat.rs"]
mod canonical_chat;
use canonical_chat::canonical_chat;
#[path = "worker/projection_digest.rs"]
mod projection_digest;
use projection_digest::projection_digest;
#[path = "worker/place_snapshot.rs"]
mod place_snapshot;
use place_snapshot::place_snapshot;
#[path = "worker/spawn_place_worker.rs"]
mod spawn_place_worker;
pub use spawn_place_worker::spawn_place_worker;
#[path = "worker/spawn_pending_place_worker.rs"]
mod spawn_pending_place_worker;
pub use spawn_pending_place_worker::spawn_pending_place_worker;
#[path = "worker/spawn_place_worker_with.rs"]
mod spawn_place_worker_with;
use spawn_place_worker_with::spawn_place_worker_with;
