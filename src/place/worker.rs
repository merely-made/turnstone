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
use commons::{GemotAuthorityView, Replica};
use gemot::moot::constitution::{CapabilityGrant, ConstitutionRules};
use gemot::moot::{
    AvailabilityPolicy, CollectionEvent, CollectionId, CollectionRef, CollectionVersion,
    ErasurePolicy, KeepBound, MOOT_ACT_ACTION, MOOT_DELEGATION_DOMAIN, MootAccessLevel,
    MootAuthority, MootFile, MootId, MootMember, MootMembershipAction, MootRetentionSettings,
    PolicyRevision,
};
use identity::delegation::{
    CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationCertificate,
    delegation_signing_salt,
};
use identity::{IdentityProvider, SealedRecordStorage};
use servitor::{Cap, cap_path};
use muniment::RedbBackend;
use proofs::Digest;
use stickleback::{
    DataKeyring, DropExportProfile, DropLimits, GroupControlFrame, GroupDirectFrame,
    GroupPrekeyBundle, GroupSession, GroupSessionId,
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
const GROUP_PREKEY_RECORD: &str = "group.prekey";

/// How long an authored invitation stays usable: seven days. Long enough to
/// hand over out of band, short enough that a forwarded envelope dies.
const INVITE_LIFETIME_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// The same ceiling `knot_authoring`'s directory mode uses. Restated rather
/// than shared because that module reads the whole `TURNSTONE_KNOT_*` set for
/// the authoring surface, and the place serves the vault, not the surface.
const DEFAULT_KNOT_MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;

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

impl AuthorityClock {
    pub(crate) fn now_ms(self) -> u64 {
        match self {
            // A clock behind the epoch yields 0, under which no grant has
            // opened yet, so every retained fact reads as pending rather than
            // effective. Unreadable time withholds content, it does not admit it.
            Self::SystemTime => std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_millis() as u64),
            Self::Fixed(at_ms) => at_ms,
        }
    }
}

/// Runtime settings for the offline worker. The default is deliberately
/// conservative: opening cached state never proposes expiry or erasure.
#[derive(Clone, Debug)]
pub struct PlaceWorkerSettings {
    pub retention: MootRetentionSettings,
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

impl PlaceWorkerSettings {
    /// What a live place needs to serve its vault, or `None` when it has none.
    pub(crate) fn projection_setup(
        &self,
        binding: &PlaceBindingV1,
        identity: &dyn IdentityProvider,
    ) -> Option<crate::place::projection_host::ProjectionSetup> {
        let root = self.knot_root.clone()?;
        Some(crate::place::projection_host::ProjectionSetup {
            root,
            max_source_bytes: self.knot_max_source_bytes,
            moot: binding.moot.0,
            authority: root_grant_id(binding.moot.0),
            issuer: identity.master_public_key().to_bytes(),
            clock: self.authority_clock,
        })
    }
}

impl Default for PlaceWorkerSettings {
    fn default() -> Self {
        Self {
            authority_clock: AuthorityClock::SystemTime,
            knot_root: std::env::var_os("TURNSTONE_KNOT_ROOT").map(PathBuf::from),
            knot_max_source_bytes: std::env::var("TURNSTONE_KNOT_MAX_BYTES")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(DEFAULT_KNOT_MAX_SOURCE_BYTES),
            capture_library: crate::place::captured_collection::LocalCaptureLibrary::default(),
            retention: MootRetentionSettings {
                revision: PolicyRevision(Digest::blake3(b"turnstone.place.offline-retention.v1")),
                availability: AvailabilityPolicy {
                    promised_floor: KeepBound::Forever,
                },
                erasure: ErasurePolicy {
                    history_ceiling: KeepBound::Forever,
                },
            },
        }
    }
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
    /// this open's own live rendezvous.
    Invite {
        session: SessionId,
        generation: u64,
        directory: PathBuf,
        prekey: Vec<u8>,
        access: crate::place::PlaceInviteAccess,
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
}

/// One authored change to the shared place.
///
/// Deliberately small and concrete. Both variants are things a person does in
/// a place; neither is a generic "write this operation", because a host that
/// can post arbitrary operations has taken over authorship from the domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceCommand {
    SendMessage { channel: String, body: String },
    ShareNode { address: String },
}

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
    pub(crate) graph: Replica<RedbBackend>,
    pub(crate) chat: ChatReplica<RedbBackend>,
    pub(crate) group: GroupSession,
}

pub fn place_store_dir(session_dir: &Path) -> PathBuf {
    session_dir.join("place")
}

pub fn place_secrets_dir(session_dir: &Path) -> PathBuf {
    session_dir.join("place-secrets")
}

fn place_secret_salt(moot: [u8; 32]) -> Vec<u8> {
    let mut salt = Vec::with_capacity(59);
    salt.extend_from_slice(b"turnstone.place.secrets.v1/");
    salt.extend_from_slice(&moot);
    salt
}

fn sealed_group_store(
    session_dir: &Path,
    identity: &dyn IdentityProvider,
    moot: [u8; 32],
) -> Result<SealedRecordStorage, String> {
    let key = identity
        .derive_keypair(&place_secret_salt(moot))
        .map_err(|error| format!("derive place secret key: {error}"))?
        .to_seed();
    Ok(SealedRecordStorage::open_with_key(
        place_secrets_dir(session_dir),
        key,
    ))
}

/// Bounds for a peer-supplied Gemot drop. An invitation arrives from someone
/// who is not yet trusted, so the reader is bounded before it allocates.
fn invite_drop_limits() -> DropLimits {
    DropLimits::default()
}

/// Admit one invitation, or leave nothing behind.
///
/// This is the gate the place-port plan's five checks describe. The ordering is
/// the point: every artifact is verified by its own domain first, and the
/// sealed group session is written only once all of them have answered. A
/// refused invitation must not leave a Gemot store, a sealed secret, or a
/// binding a later open could mistake for an admitted place.
///
/// Structural envelope validation is not admission and does not appear here
/// beyond its first line. Holding or forwarding an envelope grants nothing.
pub fn admit_invitation(
    directory: &Path,
    invite: &PlaceInviteV1,
    identity: &dyn IdentityProvider,
    settings: &PlaceWorkerSettings,
) -> Result<AdmittedPlace, String> {
    let store_existed = place_store_dir(directory).exists();
    match admit_inner(directory, invite, identity, settings) {
        Ok(admitted) => Ok(admitted),
        Err(error) => {
            // Refusal removes only what this attempt created. A half-imported
            // Gemot store would let a later open present an unadmitted place as
            // a retained one.
            //
            // The sealed secrets are deliberately not touched. They hold this
            // profile's own group identity, whose pre-key was published before
            // any invitation arrived; deleting it on a refused invitation would
            // let any stranger destroy the key material a pending welcome is
            // already addressed to.
            if !store_existed {
                let _ = std::fs::remove_dir_all(place_store_dir(directory));
            }
            Err(error)
        },
    }
}

/// Establish this profile's durable group identity for one Moot and return the
/// publishable pre-key bundle.
///
/// This must happen before an invitation can be issued, not after one arrives.
/// `GroupSession::new` draws its long-term key from the RNG, so the recipient
/// id is not derivable from the Personae root: the session that generated the
/// published pre-key is the only one a welcome can be addressed to. Creating a
/// fresh session at admission time would produce a different recipient and
/// refuse every genuine welcome.
///
/// Idempotent. A second call returns the same bundle rather than rotating the
/// identity out from under a welcome already in flight.
pub fn prepare_group_identity(
    directory: &Path,
    identity: &dyn IdentityProvider,
    moot: [u8; 32],
) -> Result<Vec<u8>, String> {
    let storage = sealed_group_store(directory, identity, moot)?;
    if let Some(bundle) = storage
        .load_record(GROUP_PREKEY_RECORD)
        .map_err(|error| format!("load sealed group pre-key: {error}"))?
    {
        return Ok(bundle);
    }
    let (session, prekey) = GroupSession::new(GroupSessionId(moot), identity)
        .map_err(|error| format!("create group session: {error}"))?;
    let bundle = prekey
        .to_bytes()
        .map_err(|error| format!("encode group pre-key: {error}"))?;
    save_group_session(directory, identity, &session)?;
    storage
        .save_record(GROUP_PREKEY_RECORD, &bundle)
        .map_err(|error| format!("seal group pre-key: {error}"))?;
    Ok(bundle)
}

/// Author an invitation to this profile's own place for one published pre-key.
///
/// The counterpart to [`admit_invitation`], and deliberately the same shape in
/// reverse: it reads only retained domain state, mints no authority, and every
/// field it fills is one the recipient re-derives and checks independently.
///
/// The caller supplies `joiner_prekey` out of band. Which channel carried it is
/// not this function's concern and must never become evidence: the bundle
/// carries its own Personae attestation, which is what binds it to a person.
#[allow(clippy::too_many_arguments)]
pub fn author_invitation(
    directory: &Path,
    binding: &PlaceBindingV1,
    identity: &dyn IdentityProvider,
    joiner_prekey: &[u8],
    not_after_ms: u64,
    rendezvous: Vec<crate::place::invite::RendezvousV1>,
    projection_grant: Option<SignedDelegationCertificate>,
    settings: &PlaceWorkerSettings,
) -> Result<PlaceInviteV1, String> {
    let moot = pollster::block_on(MootFile::open_existing(
        place_store_dir(directory).join("gemot"),
        MootId(binding.moot.0),
        settings.retention.clone(),
    ))
    .map_err(|error| format!("open Gemot store: {error}"))?;
    author_invitation_with(
        &moot,
        directory,
        binding,
        identity,
        joiner_prekey,
        not_after_ms,
        rendezvous,
        projection_grant,
    )
}

/// [`author_invitation`] against an already-open Moot, for the worker that
/// holds this place's only store handle.
#[allow(clippy::too_many_arguments)]
pub(crate) fn author_invitation_with(
    moot: &MootFile,
    directory: &Path,
    binding: &PlaceBindingV1,
    identity: &dyn IdentityProvider,
    joiner_prekey: &[u8],
    not_after_ms: u64,
    rendezvous: Vec<crate::place::invite::RendezvousV1>,
    projection_grant: Option<SignedDelegationCertificate>,
) -> Result<PlaceInviteV1, String> {
    binding
        .validate()
        .map_err(|error| format!("place binding: {error}"))?;
    let prekey = GroupPrekeyBundle::from_bytes(joiner_prekey)
        .map_err(|error| format!("decode joiner pre-key: {error}"))?;
    if prekey.group != GroupSessionId(binding.moot.0) {
        return Err("joiner pre-key belongs to another group".to_string());
    }
    let joiner_root = prekey
        .personae_root()
        .map_err(|error| format!("verify joiner pre-key: {error}"))?;

    let snapshot = pollster::block_on(moot.snapshot())
        .map_err(|error| format!("materialize Gemot: {error}"))?;
    // The recipient must already be a governed member. Inviting someone the
    // Moot has not admitted would mint an envelope that can only ever be
    // refused, and refusing here names the real reason instead.
    if !snapshot
        .membership
        .members
        .iter()
        .any(|member| member.member == joiner_root)
    {
        return Err("Gemot membership does not contain the invited root".to_string());
    }

    let mut evidence = Vec::new();
    pollster::block_on(moot.export_plain_drop(
        &mut evidence,
        DropExportProfile::default(),
        DropLimits::default(),
    ))
    .map_err(|error| format!("export Gemot evidence: {error}"))?;

    // Welcome the recipient into the crypto group, then persist: the epoch this
    // mints is the one the envelope names, so losing it would strand a welcome
    // this profile can no longer follow.
    let mut group = load_group_session(directory, identity, binding.moot.0)?;
    group
        .register_prekey(&prekey)
        .map_err(|error| format!("register joiner pre-key: {error}"))?;
    let dispatch = group
        .add(prekey.recipient)
        .map_err(|error| format!("welcome the joiner: {error}"))?;
    let direct = dispatch
        .direct_for(prekey.recipient)
        .ok_or_else(|| "welcome carries no frame for the invited recipient".to_string())?;
    let expected_epoch = group
        .current_epoch()
        .ok_or_else(|| "welcoming a member installed no epoch".to_string())?;
    save_group_session(directory, identity, &group)?;

    Ok(PlaceInviteV1 {
        version: crate::place::invite::PLACE_INVITE_VERSION,
        binding: binding.clone(),
        founder: snapshot.governance.founder,
        inviter: identity.master_public_key().to_bytes(),
        inviter_prekey: inline_artifact(&sealed_prekey_bytes(directory, identity, binding.moot.0)?),
        governance: inline_artifact(&evidence),
        key_welcome: inline_artifact(
            &dispatch
                .control
                .to_bytes()
                .map_err(|error| format!("encode welcome control: {error}"))?,
        ),
        key_direct: inline_artifact(
            &direct
                .to_bytes()
                .map_err(|error| format!("encode welcome frame: {error}"))?,
        ),
        expected_epoch,
        membership_heads: snapshot.membership.auth_heads,
        not_after_ms,
        rendezvous,
        // A reader's envelope carries none, which is the whole of the
        // difference at the projection door.
        projection_grant: projection_grant
            .as_ref()
            .map(crate::place::projection_host::encode_grant)
            .transpose()?
            .as_deref()
            .map(inline_artifact),
    })
}

/// This profile's own published pre-key bundle for one Moot.
fn sealed_prekey_bytes(
    directory: &Path,
    identity: &dyn IdentityProvider,
    moot: [u8; 32],
) -> Result<Vec<u8>, String> {
    sealed_group_store(directory, identity, moot)?
        .load_record(GROUP_PREKEY_RECORD)
        .map_err(|error| format!("load sealed group pre-key: {error}"))?
        .ok_or_else(|| "this profile has no prepared group identity".to_string())
}

/// Wrap bytes as a digest-checked inline artifact.
fn inline_artifact(bytes: &[u8]) -> crate::place::invite::ArtifactRefV1 {
    crate::place::invite::ArtifactRefV1::Inline {
        media_type: "application/vnd.mere.place-artifact".into(),
        digest: *Digest::blake3(bytes)
            .bytes
            .first_chunk::<32>()
            .expect("blake3 produces 32 bytes"),
        bytes: bytes.to_vec(),
    }
}

/// Create this place's crypto group with this profile as its first member.
///
/// The founding counterpart to [`prepare_group_identity`]. A Moot's governance
/// fold and its DCGKA group are founded separately and neither implies the
/// other: Gemot decides who belongs, Stickleback decides who can read. Until
/// this runs, the founder holds a group identity but is not an active member
/// of any group, so it cannot welcome anyone.
///
/// Idempotent. A group that already has members is left exactly as it is
/// rather than re-created, since re-creating would strand every epoch already
/// handed out.
pub fn found_place_group(
    directory: &Path,
    identity: &dyn IdentityProvider,
    moot: [u8; 32],
) -> Result<(), String> {
    prepare_group_identity(directory, identity, moot)?;
    let mut group = load_group_session(directory, identity, moot)?;
    let members = group
        .members()
        .map_err(|error| format!("read group membership: {error}"))?;
    if !members.is_empty() {
        return Ok(());
    }
    group
        .create(&[])
        .map_err(|error| format!("create place group: {error}"))?;
    save_group_session(directory, identity, &group)
}

/// The channel a founded place opens with. Renaming is a later product
/// decision; founding without one would leave nowhere to speak.
pub const DEFAULT_CHANNEL: &str = "general";

/// Domain-separated 32 bytes for one place-scoped derivation.
pub(crate) fn place_tag(domain: &[u8], moot: [u8; 32]) -> [u8; 32] {
    let mut input = Vec::with_capacity(domain.len() + 32);
    input.extend_from_slice(domain);
    input.extend_from_slice(&moot);
    *blake3::hash(&input).as_bytes()
}

/// Salt for the key that signs this place's constitutional genesis.
///
/// A derived key, not the master: `IdentityProvider` never surrenders the
/// master seed, and Gemot's genesis wants a signing seed. The constitution's
/// checkpoint signer is therefore this profile's place-founding key, while
/// membership and every capability grant still name the Personae root.
fn founding_salt(moot: [u8; 32]) -> Vec<u8> {
    let mut salt = Vec::with_capacity(61);
    salt.extend_from_slice(b"turnstone.place.founding.v1/");
    salt.extend_from_slice(&moot);
    salt
}

/// The constitutional root grant's id, derived from the Moot it governs.
/// Deterministic so a later revocation or audit can recompute it.
pub(crate) fn root_grant_id(moot: [u8; 32]) -> [u8; 32] {
    place_tag(b"turnstone.place.root-grant.v1/", moot)
}

/// `cap_path` encodes a scope as `scope/<path>`, and personae matches on a
/// slash boundary, so this one prefix covers both `commons/container/...`
/// and `commons/chat/...`. Deriving it beats writing the literal: the
/// encoding is servitor's to change.
pub fn place_capability_prefix() -> String {
    cap_path(&Cap::scope("commons").expect("the `commons` scope is well formed"))
}

/// The Moot-scoped capability a place delegation carries.
pub fn place_scope(moot: [u8; 32]) -> CapabilityScope {
    CapabilityScope {
        domain: MOOT_DELEGATION_DOMAIN.into(),
        resource: moot.to_vec(),
        path_prefix: place_capability_prefix(),
        actions: [MOOT_ACT_ACTION.to_string()].into_iter().collect(),
    }
}

/// Founder-only constitution with ONE root grant over both Commons domains.
///
/// `checkpoint_signer` is the key that authors constitutional events;
/// `grant_subject` is the Personae root that may delegate beneath the grant.
/// They differ in product (a derived founding key signs, the root holds
/// authority) and coincide in fixtures.
pub fn place_rules(
    checkpoint_signer: [u8; 32],
    grant_subject: [u8; 32],
    moot: [u8; 32],
    not_before_ms: u64,
    expires_at_ms: Option<u64>,
) -> ConstitutionRules {
    let mut rules = ConstitutionRules::founder_only(checkpoint_signer);
    rules.grant(CapabilityGrant {
        id: root_grant_id(moot),
        subject: grant_subject,
        path_prefix: place_capability_prefix(),
        not_before_ms,
        expires_at_ms,
        delegation_depth: 2,
    });
    rules
}

/// Gemot authors delegation facts under the scope-derived key that signed
/// the certificate, not the master key: the master secret stays behind the
/// provider.
pub fn founder_signing_key(
    identity: &dyn IdentityProvider,
    moot: [u8; 32],
) -> Result<identity::Ed25519Keypair, String> {
    identity
        .derive_keypair(&delegation_signing_salt(&place_scope(moot)))
        .map_err(|error| format!("derive place delegation signing key: {error}"))
}

/// One signed delegation admitting `subject` to both Commons domains.
///
/// Deterministic in its nonce, so a caller can recompute the certificate id
/// later to revoke it without having retained the certificate.
pub fn place_delegation(
    issuer: &dyn IdentityProvider,
    moot: [u8; 32],
    subject: [u8; 32],
    issued_at_ms: u64,
    not_before_ms: u64,
    expires_at_ms: Option<u64>,
) -> Result<SignedDelegationCertificate, String> {
    let mut nonce_input = Vec::with_capacity(64);
    nonce_input.extend_from_slice(&moot);
    nonce_input.extend_from_slice(&subject);
    let nonce = place_tag(
        b"turnstone.place.delegation.v1/",
        *blake3::hash(&nonce_input).as_bytes(),
    );
    SignedDelegationCertificate::issue(
        &ProviderRef(issuer),
        DelegationCertificate::new(
            DelegationParent::Root(root_grant_id(moot)),
            issuer.master_public_key().to_bytes(),
            subject,
            place_scope(moot),
            issued_at_ms,
            not_before_ms,
            expires_at_ms,
            0,
            nonce,
        ),
    )
    .map_err(|error| format!("issue place delegation: {error}"))
}

/// `SignedDelegationCertificate::issue` takes a sized provider and the worker
/// holds a trait object. Borrowing through this bridges the two without
/// copying key material or widening any public signature.
pub(crate) struct ProviderRef<'a>(pub(crate) &'a dyn IdentityProvider);

impl IdentityProvider for ProviderRef<'_> {
    fn master_public_key(&self) -> identity::Ed25519PublicKey {
        self.0.master_public_key()
    }

    fn derive_keypair(
        &self,
        salt: &[u8],
    ) -> Result<identity::Ed25519Keypair, identity::IdentityError> {
        self.0.derive_keypair(salt)
    }

    fn attest_derived_key(
        &self,
        salt: &[u8],
    ) -> Result<identity::DerivedKeyAttestation, identity::IdentityError> {
        self.0.attest_derived_key(salt)
    }
}

/// 32 unpredictable bytes for one place identifier.
fn random_id() -> Result<[u8; 32], String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|error| format!("draw place identifier: {error}"))?;
    Ok(bytes)
}

/// Found a new place in `directory` and return its durable binding.
///
/// The founding counterpart to [`admit_invitation`]: it creates the Moot,
/// its constitution, its membership fold, this profile's own delegation, the
/// crypto group, and the default channel. Nothing here dials, writes
/// `place.json`, or claims app state; the worker does that once this returns.
///
/// The self-delegation is not ceremony. Without it the founder's own writes
/// project as pending against its own Moot, because authority is evaluated
/// through the converged delegation fold for everyone, the founder included.
pub fn found_place(
    directory: &Path,
    identity: &dyn IdentityProvider,
    name: &str,
    settings: &PlaceWorkerSettings,
) -> Result<PlaceBindingV1, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a place needs a name".to_string());
    }
    let moot_id = random_id()?;
    let binding = PlaceBindingV1::new(
        PlaceId(moot_id),
        crate::place::SharedContainerId(random_id()?),
        crate::place::ChatSpaceId(random_id()?),
        DEFAULT_CHANNEL,
    )
    .map_err(|error| format!("place binding: {error}"))?;

    let now_ms = settings.authority_clock.now_ms();
    let root = identity.master_public_key().to_bytes();
    let founding = identity
        .derive_keypair(&founding_salt(moot_id))
        .map_err(|error| format!("derive place founding key: {error}"))?;
    let founder_id = founding.public_key().to_bytes();
    let rules = place_rules(founder_id, root, moot_id, now_ms, None);

    let stores = place_store_dir(directory);
    std::fs::create_dir_all(&stores).map_err(|error| format!("create place store: {error}"))?;
    let moot = pollster::block_on(MootFile::open(
        stores.join("gemot"),
        MootId(moot_id),
        founder_id,
        settings.retention.clone(),
    ))
    .map_err(|error| format!("open Gemot store: {error}"))?;
    pollster::block_on(moot.found(founding.to_seed(), None, None, rules.clone(), now_ms))
        .map_err(|error| format!("found Gemot constitution: {error}"))?;
    pollster::block_on(moot.membership_store().author_for_identity(
        identity,
        MootMembershipAction::Create {
            initial_members: vec![MootMember {
                member: root,
                access: MootAccessLevel::Manage,
            }],
        },
    ))
    .map_err(|error| format!("create Gemot membership: {error}"))?;
    pollster::block_on(moot.delegation_store().author_issue(
        &founder_signing_key(identity, moot_id)?,
        &rules,
        place_delegation(identity, moot_id, root, now_ms, now_ms, None)?,
    ))
    .map_err(|error| format!("issue founder delegation: {error}"))?;
    drop(moot);

    found_place_group(directory, identity, moot_id)?;
    let group = load_group_session(directory, identity, moot_id)?;
    let keyring = DataKeyring::from_bytes(
        &group
            .data_keyring_state()
            .map_err(|error| format!("read group data epochs: {error}"))?,
    )
    .map_err(|error| format!("decode group data epochs: {error}"))?;
    let chat_backend = RedbBackend::open(stores.join("commons-chat.redb"))
        .map_err(|error| format!("open Commons chat cache: {error}"))?;
    let mut chat = ChatReplica::for_identity(chat_backend, binding.chat.0, identity, keyring)
        .map_err(|error| format!("bind Commons chat writer: {error}"))?;
    // The place's name lives where peers converge on it, as the default
    // channel's title, rather than in a local sidecar only the founder reads.
    pollster::block_on(chat.author(commons::chat::ChatEvent::Channel(
        commons::chat::Channel {
            id: binding.default_channel.clone(),
            title: name.to_string(),
        },
    )))
    .map_err(|error| format!("open the default channel: {error}"))?;
    drop(chat);
    Ok(binding)
}

/// Add one invited root to this place's Moot membership at `access`, and for
/// a writer delegate both Commons domains to it. Idempotent in membership: a
/// root already admitted is left at the access it holds.
///
/// A reader gets membership and no delegation. That is the whole difference:
/// reading a place is a membership question answered by the welcome, while
/// authoring into it is a capability question, and a profile nobody delegated
/// to fails its own worker's preflight before anything is authored.
///
/// Takes the already-open Moot rather than opening its own, because the
/// caller is the worker holding this place's only store handle.
///
/// Returns what it authored so the caller can publish it. Storing is what
/// makes an admission survive; publishing is what makes it arrive at a peer
/// that is already connected.
fn admit_member(
    moot: &MootFile,
    binding: &PlaceBindingV1,
    identity: &dyn IdentityProvider,
    joiner_root: [u8; 32],
    access: crate::place::PlaceInviteAccess,
    now_ms: u64,
) -> Result<AdmittedOps, String> {
    use crate::place::PlaceInviteAccess;
    let moot_id = binding.moot.0;
    let snapshot = pollster::block_on(moot.snapshot())
        .map_err(|error| format!("materialize Gemot: {error}"))?;
    let mut authored = AdmittedOps::default();
    if !snapshot
        .membership
        .members
        .iter()
        .any(|member| member.member == joiner_root)
    {
        authored.membership = Some(
            pollster::block_on(moot.membership_store().author_for_identity(
                identity,
                MootMembershipAction::Add {
                    member: joiner_root,
                    access: match access {
                        PlaceInviteAccess::Writer => MootAccessLevel::Write,
                        PlaceInviteAccess::Reader => MootAccessLevel::Read,
                    },
                },
            ))
            .map_err(|error| format!("add the invited root to membership: {error}"))?,
        );
    }
    if matches!(access, PlaceInviteAccess::Writer) {
        authored.delegation = Some(
            pollster::block_on(moot.delegation_store().author_issue(
                &founder_signing_key(identity, moot_id)?,
                &snapshot.governance.rules,
                place_delegation(identity, moot_id, joiner_root, now_ms, now_ms, None)?,
            ))
            .map_err(|error| format!("delegate the Commons domains: {error}"))?,
        );
        // A separate grant, in a separate domain, under the same root
        // authority. It admits the holder to this host's projection door and
        // is authority over nothing in Commons or Gemot.
        authored.projection = Some(crate::place::projection_host::issue_projection_grant(
            identity,
            moot_id,
            joiner_root,
            now_ms,
        )?);
    }
    Ok(authored)
}

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

/// Reopen the sealed group session established by [`prepare_group_identity`].
pub(crate) fn load_group_session(
    directory: &Path,
    identity: &dyn IdentityProvider,
    moot: [u8; 32],
) -> Result<GroupSession, String> {
    let storage = sealed_group_store(directory, identity, moot)?;
    let bytes: Vec<u8> = storage
        .load_record(GROUP_SESSION_RECORD)
        .map_err(|error| format!("load sealed group session: {error}"))?
        .ok_or_else(|| "sealed group session is absent".to_string())?;
    GroupSession::from_bytes(&bytes)
        .map_err(|error| format!("decode sealed group session: {error}"))
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

fn admit_inner(
    directory: &Path,
    invite: &PlaceInviteV1,
    identity: &dyn IdentityProvider,
    settings: &PlaceWorkerSettings,
) -> Result<AdmittedPlace, String> {
    invite
        .validate()
        .map_err(|error| format!("place invitation: {error}"))?;
    let binding = &invite.binding;
    let local_root = identity.master_public_key().to_bytes();

    // 0. The inviter's own time bound, checked before anything is created.
    //    The heads pin below already invalidates an invitation whenever the
    //    roster moves; this additionally stops a forwarded envelope working
    //    forever in a Moot whose membership never changes.
    let now_ms = settings.authority_clock.now_ms();
    if now_ms > invite.not_after_ms {
        return Err(format!(
            "invitation expired at {} and it is now {now_ms}",
            invite.not_after_ms
        ));
    }

    // 5. The Commons scopes are distinct governed roots, not aliases of the
    //    Moot or of each other. Checked first because it is free, and because
    //    a collision here would silently point two domains at one store.
    if binding.root.0 == binding.moot.0 || binding.chat.0 == binding.moot.0 {
        return Err("invitation reuses the Moot id as a Commons scope".to_string());
    }
    if binding.root.0 == binding.chat.0 {
        return Err("invitation gives the graph and chat the same scope".to_string());
    }

    // 1. The Gemot evidence addresses this Moot. Imported into the place's own
    //    store so the fold is Gemot's, not a claim the envelope makes.
    let stores = place_store_dir(directory);
    std::fs::create_dir_all(&stores).map_err(|error| format!("create place store: {error}"))?;
    let moot = pollster::block_on(MootFile::open(
        stores.join("gemot"),
        MootId(binding.moot.0),
        invite.founder,
        settings.retention.clone(),
    ))
    .map_err(|error| format!("open Gemot store: {error}"))?;
    let evidence = invite
        .governance
        .verified_bytes("governance artifact")
        .map_err(|error| format!("place invitation: {error}"))?;
    let receipt = pollster::block_on(
        moot.import_plain_drop(std::io::Cursor::new(evidence), invite_drop_limits()),
    )
    .map_err(|error| format!("import Gemot evidence: {error}"))?;
    if receipt.snapshot.moot_id != MootId(binding.moot.0) {
        return Err("Gemot evidence addresses another Moot".to_string());
    }

    // 2. The converged membership fold contains the local Personae root, and
    //    the claimed inviter. The envelope names an author; Gemot decides
    //    whether that name is a member.
    let members = &receipt.snapshot.membership.members;
    if !members.iter().any(|member| member.member == local_root) {
        return Err("Gemot membership does not contain this Personae root".to_string());
    }
    if !members.iter().any(|member| member.member == invite.inviter) {
        return Err("invitation names an author outside Gemot membership".to_string());
    }

    // 3. The welcome is bound to this group and addressed to this recipient's
    //    authenticated crypto identity, and it produces a usable epoch.
    let control = GroupControlFrame::from_bytes(
        invite
            .key_welcome
            .verified_bytes("key welcome artifact")
            .map_err(|error| format!("place invitation: {error}"))?,
    )
    .map_err(|error| format!("decode group welcome: {error}"))?;
    let direct = GroupDirectFrame::from_bytes(
        invite
            .key_direct
            .verified_bytes("recipient welcome artifact")
            .map_err(|error| format!("place invitation: {error}"))?,
    )
    .map_err(|error| format!("decode recipient welcome: {error}"))?;
    let group_id = GroupSessionId(binding.moot.0);
    if control.group != group_id || direct.group != group_id {
        return Err("group welcome addresses another group".to_string());
    }
    if direct.control != control.id {
        return Err("recipient welcome belongs to another control frame".to_string());
    }
    // The group identity whose published pre-key this welcome answers. Absent
    // means nobody could have addressed a welcome to this profile yet.
    let mut group = load_group_session(directory, identity, binding.moot.0)
        .map_err(|error| format!("{error}; prepare a group identity before joining"))?;
    if direct.recipient != group.member() {
        return Err("group welcome is addressed to another recipient".to_string());
    }

    // The sender's authenticated pre-key. Its Personae attestation is what
    // makes `invite.inviter` a verified fact rather than an envelope claim, so
    // the two must agree before the bundle is registered.
    let inviter_prekey = GroupPrekeyBundle::from_bytes(
        invite
            .inviter_prekey
            .verified_bytes("inviter pre-key artifact")
            .map_err(|error| format!("place invitation: {error}"))?,
    )
    .map_err(|error| format!("decode inviter pre-key: {error}"))?;
    if inviter_prekey.group != group_id {
        return Err("inviter pre-key belongs to another group".to_string());
    }
    let attested = inviter_prekey
        .personae_root()
        .map_err(|error| format!("verify inviter pre-key: {error}"))?;
    if attested != invite.inviter {
        return Err("inviter pre-key attests a different Personae root".to_string());
    }
    group
        .register_prekey(&inviter_prekey)
        .map_err(|error| format!("register inviter pre-key: {error}"))?;
    group
        .process(invite.inviter, &control, Some(&direct))
        .map_err(|error| format!("process group welcome: {error}"))?;
    // 4. Gemot binds that epoch to the same membership heads.
    //
    // The epoch must be the one the invitation describes, and it must have been
    // minted against the membership state Gemot itself converged to from the
    // imported evidence. Without the second half, a welcome minted before a
    // removal would hand the joiner a key the departed member still holds, and
    // every other check would still pass.
    match group.current_epoch() {
        None => return Err("group welcome produced no current epoch".to_string()),
        Some(epoch) if epoch != invite.expected_epoch => {
            return Err(
                "group welcome installed an epoch the invitation does not name".to_string(),
            );
        },
        Some(_) => {},
    }
    // `auth_heads()` returns sorted heads, and the envelope's are bounded and
    // compared as given: a reordered or padded list is a different claim.
    if receipt.snapshot.membership.auth_heads != invite.membership_heads {
        return Err("invitation pins membership heads that Gemot did not converge to".to_string());
    }

    // Every domain has answered. Only now does anything durable exist, and the
    // binding is written last so the presence of `place.json` implies the whole
    // check list ran, not merely that an envelope parsed.
    save_group_session(directory, identity, &group)?;
    // 6. The projection grant, when the envelope carries one. A reader's does
    //    not, and its absence is not a refusal: it is the reader being refused
    //    at the holder's door later, which is where that decision belongs.
    if let Some(artifact) = &invite.projection_grant {
        let bytes = artifact
            .verified_bytes("projection grant artifact")
            .map_err(|error| format!("place invitation: {error}"))?;
        let grant = crate::place::projection_host::decode_grant(bytes)?;
        if !grant.verify() {
            return Err("projection grant does not verify".to_string());
        }
        if grant.certificate.subject != local_root {
            return Err("projection grant names another subject".to_string());
        }
        crate::place::rendezvous::save_projection_grant(directory, bytes)?;
    }
    crate::session::save_place_binding(directory, binding)
        .map_err(|error| format!("persist place binding: {error}"))?;
    Ok(AdmittedPlace {
        binding: binding.clone(),
        moot: MootCache {
            membership_epoch: receipt.snapshot.membership.epoch,
            members: members.len(),
            roster_members: receipt.snapshot.roster.members.len(),
            delegated_certificates: receipt.snapshot.delegated_certificates,
            standing_operations: receipt.snapshot.standing_operations,
        },
        group_members: group
            .members()
            .map_err(|error| format!("materialize group membership: {error}"))?
            .len(),
    })
}

/// Persist the group session inside the same Personae-derived sealed boundary
/// the worker reopens. [`admit_invitation`] calls this after every check.
pub fn save_group_session(
    session_dir: &Path,
    identity: &dyn IdentityProvider,
    session: &GroupSession,
) -> Result<(), String> {
    let storage = sealed_group_store(session_dir, identity, session.group().0)?;
    let bytes = session
        .to_bytes()
        .map_err(|error| format!("encode group session: {error}"))?;
    storage
        .save_record(GROUP_SESSION_RECORD, &bytes)
        .map_err(|error| format!("seal group session: {error}"))
}

/// How long a same-process reopen waits out the store lock a closed bind
/// still holds. p2panda runs each LogSync session's actor on its own
/// `std::thread` (ractor `ThreadLocalActorSpawner`), so the store clone in
/// that actor's state outlives both the awaited lane leave and the lane
/// runtime's shutdown by a few hundred milliseconds.
pub(crate) const RECONNECT_REOPEN_BUDGET: std::time::Duration =
    std::time::Duration::from_secs(2);

/// `open_cached_place`, retried while redb still reports the lock held.
///
/// Only the Reconnect path needs this: it is the one place that closes a live
/// bind and reopens the same stores inside one process.
fn reopen_cached_place(
    directory: &Path,
    binding: &PlaceBindingV1,
    identity: &dyn IdentityProvider,
    settings: &PlaceWorkerSettings,
) -> Result<(OpenPlace, OfflinePlaceSnapshot), String> {
    let deadline = std::time::Instant::now() + RECONNECT_REOPEN_BUDGET;
    loop {
        match open_cached_place(directory, binding, identity, settings) {
            Err(error)
                if error.contains("already open") && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            outcome => return outcome,
        }
    }
}

pub(crate) fn open_cached_place(
    directory: &Path,
    binding: &PlaceBindingV1,
    identity: &dyn IdentityProvider,
    settings: &PlaceWorkerSettings,
) -> Result<(OpenPlace, OfflinePlaceSnapshot), String> {
    binding
        .validate()
        .map_err(|error| format!("place binding: {error}"))?;
    let storage = sealed_group_store(directory, identity, binding.moot.0)?;
    let bytes: Vec<u8> = storage
        .load_record(GROUP_SESSION_RECORD)
        .map_err(|error| format!("load sealed group session: {error}"))?
        .ok_or_else(|| "sealed group session is absent".to_string())?;
    let group = GroupSession::from_bytes(&bytes)
        .map_err(|error| format!("decode sealed group session: {error}"))?;
    if group.group() != GroupSessionId(binding.moot.0) {
        return Err("sealed group session addresses another Moot".to_string());
    }
    let root = identity.master_public_key().to_bytes();
    if group.personae_root() != root {
        return Err("sealed group session belongs to another Personae root".to_string());
    }
    let keyring = DataKeyring::from_bytes(
        &group
            .data_keyring_state()
            .map_err(|error| format!("read group data epochs: {error}"))?,
    )
    .map_err(|error| format!("decode group data epochs: {error}"))?;

    let stores = place_store_dir(directory);
    let moot = pollster::block_on(MootFile::open_existing(
        stores.join("gemot"),
        MootId(binding.moot.0),
        settings.retention.clone(),
    ))
    .map_err(|error| format!("open Gemot cache: {error}"))?;

    let graph_backend = RedbBackend::open(stores.join("commons-graph.redb"))
        .map_err(|error| format!("open Commons graph cache: {error}"))?;
    let graph = Replica::for_identity(graph_backend, binding.root.0, identity)
        .map_err(|error| format!("bind Commons graph writer: {error}"))?;

    let chat_backend = RedbBackend::open(stores.join("commons-chat.redb"))
        .map_err(|error| format!("open Commons chat cache: {error}"))?;
    let chat = ChatReplica::for_identity(chat_backend, binding.chat.0, identity, keyring)
        .map_err(|error| format!("bind Commons chat writer: {error}"))?;

    let open = OpenPlace {
        directory: directory.to_path_buf(),
        lanes: None,
        binding: binding.clone(),
        subject: identity.master_public_key().to_bytes(),
        capture_directory: crate::trail_memory::memory_dir(directory),
        collection_selection: crate::session::load_place_collection(directory)?,
        moot,
        graph,
        chat,
        group,
    };
    let snapshot = place_snapshot(&open, binding.moot.0, settings)?;
    Ok((open, snapshot))
}

/// Author one fact locally, then push it onto the live lane.
///
/// Preflight first: Turnstone refuses its own unauthorized command rather
/// than authoring an operation that every peer would then filter out of its
/// projection. That is the local half of the two authority paths — the other
/// being that received operations are stored whatever their verdict, so a
/// later re-evaluation can still reverse it.
///
/// Publishing is separate from authoring on purpose. Authoring stores the
/// operation, which is what makes it survive; publishing is what makes it
/// arrive. A place with no live lanes authors happily and syncs when it next
/// joins.
fn author_into_place(
    open: &mut OpenPlace,
    binding: &PlaceBindingV1,
    identity: &dyn IdentityProvider,
    command: &PlaceCommand,
    settings: &PlaceWorkerSettings,
) -> Result<(), String> {
    let at_ms = settings.authority_clock.now_ms();
    let subject = identity.master_public_key().to_bytes();
    let needed = match command {
        PlaceCommand::SendMessage { .. } => commons::chat::chat_write_capability(binding.chat.0),
        PlaceCommand::ShareNode { .. } => commons::commons_write_capability(binding.root.0),
    };
    let moot_snapshot = pollster::block_on(open.moot.snapshot())
        .map_err(|error| format!("materialize Gemot: {error}"))?;
    let delegations = pollster::block_on(open.moot.delegations())
        .map_err(|error| format!("materialize Gemot delegations: {error}"))?;
    let authority = GemotAuthorityView {
        authority: MootAuthority {
            delegations: &delegations,
            rules: &moot_snapshot.governance.rules,
            moot_id: binding.moot.0,
            now_ms: at_ms,
        },
    };
    if !matches!(
        commons::CommonsAuthority::classify(
            &authority,
            servitor::Subject(subject),
            &needed,
            servitor::Mode::Write,
        ),
        commons::AuthorityState::Effective
    ) {
        return Err("this profile holds no effective capability to author here".to_string());
    }

    match command {
        PlaceCommand::SendMessage { channel, body } => {
            if body.trim().is_empty() {
                return Err("a message needs a body".to_string());
            }
            let operation = pollster::block_on(open.chat.author(
                commons::chat::ChatEvent::Message(commons::chat::Message {
                    channel: channel.clone(),
                    body: body.clone(),
                    sent_at_ms: settings.authority_clock.now_ms(),
                    reply_to: None,
                }),
            ))
            .map_err(|error| format!("author message: {error}"))?;
            if let Some(lanes) = &open.lanes {
                lanes.publish_chat(operation)?;
            }
        },
        PlaceCommand::ShareNode { address } => {
            if address.trim().is_empty() {
                return Err("a shared node needs an address".to_string());
            }
            // Address as identity, so sharing the same page twice converges on
            // one node instead of accumulating duplicates of the same thing.
            //
            // A Knot document held in this profile's vault is the one address
            // that cannot be shared as it stands: `file:///...` and
            // `knot://vault/...` both name a document only this mere can
            // reach. Rewritten here rather than at the app because this is
            // where the sharer's Personae root and its vault root are both
            // known, and because a rewrite that depended on app state would
            // be one more thing that could be stale when the node is authored.
            let address = settings
                .knot_root
                .as_deref()
                .filter(|_| crate::knot_authoring::is_knot_address(address))
                .and_then(|root| {
                    crate::knot_authoring::place_held_rewrite(address, root, &subject)
                })
                .unwrap_or_else(|| address.clone());
            let operation = pollster::block_on(open.graph.edit(move |log| {
                log.insert_node(
                    &chartulary::Author::new("turnstone"),
                    chartulary::Container::new(address.clone()).with_address(address),
                );
            }))
            .map_err(|error| format!("author shared node: {error}"))?;
            if let Some(lanes) = &open.lanes {
                lanes.publish_graph(operation)?;
            }
        },
    }
    Ok(())
}

/// Prepare the dial to one holder, or say why this profile cannot.
///
/// Three things have to be true and each failure reads differently: the place
/// is live (there is a transport at all), admission stored a projection grant
/// (this profile was admitted as a writer), and the holder's ticket is known.
/// For this slice the only ticket a member has is the founder's saved
/// rendezvous, so a holder who is not the founder is unreachable rather than
/// searched for.
fn visit_dial(
    open: &OpenPlace,
    directory: &Path,
    identity: &dyn IdentityProvider,
    holder_root: [u8; 32],
    settings: &PlaceWorkerSettings,
) -> Result<crate::place::lanes::HolderDial, String> {
    let lanes = open
        .lanes
        .as_ref()
        .ok_or_else(|| "this place is not live, so no holder can be reached".to_string())?;
    let grant = crate::place::rendezvous::load_projection_grant(directory).ok_or_else(|| {
        "this profile holds no projection grant for this place; a reader is not admitted \
         to a held document"
            .to_string()
    })?;
    let ticket = crate::place::rendezvous::load_rendezvous(
        directory,
        &open.binding,
        settings.authority_clock.now_ms(),
    )?
    .into_iter()
    .next()
    .ok_or_else(|| "no rendezvous for the mere holding this document".to_string())?;
    // Domain-separated per holder and per open, so two visits in one session
    // never present the same handshake nonce.
    let nonce = place_tag(
        b"turnstone.place.visit-nonce.v1/",
        *blake3::hash(
            &[
                &holder_root[..],
                &open.binding.moot.0[..],
                &settings.authority_clock.now_ms().to_le_bytes()[..],
            ]
            .concat(),
        )
        .as_bytes(),
    );
    crate::place::lanes::holder_dial(lanes, &ticket, &grant, identity, nonce)
}

/// One message as both converged peers see it. Sorted by the operation id,
/// which is content-derived, so two peers that hold the same messages
/// serialize the same bytes whatever order they arrived in.
fn canonical_chat(
    projection: &commons::chat::ChatProjection,
) -> Vec<(String, String, String, u64)> {
    let mut rows: Vec<_> = projection
        .messages
        .iter()
        .map(|authored| {
            (
                crate::place::hex32(&authored.operation),
                crate::place::hex32(&authored.author),
                authored.message.body.clone(),
                authored.message.sent_at_ms,
            )
        })
        .collect();
    rows.sort();
    rows
}

/// blake3 over a domain tag and one canonical projection, hex.
fn projection_digest<T: serde::Serialize>(domain: &[u8], value: &T) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(&serde_json::to_vec(value).unwrap_or_default());
    hasher.finalize().to_hex().to_string()
}

/// Fold the current app-owned snapshot from an open place's stores.
///
/// Factored from the open path so a resync can re-fold WITHOUT dropping the
/// live lanes: the lanes drain received operations into these same stores,
/// and this is where they become authority-filtered, product-visible state.
fn place_snapshot(
    open: &OpenPlace,
    moot_id: [u8; 32],
    settings: &PlaceWorkerSettings,
) -> Result<OfflinePlaceSnapshot, String> {
    let moot_snapshot = pollster::block_on(open.moot.snapshot())
        .map_err(|error| format!("materialize Gemot: {error}"))?;
    let at_ms = settings.authority_clock.now_ms();

    // The single authority view both Commons domains project through. It is
    // built from the Moot's own converged constitution and delegation fold, so
    // an operation whose author was never granted, or whose grant was
    // withdrawn, cannot reach a projection this worker emits.
    let delegations = pollster::block_on(open.moot.delegations())
        .map_err(|error| format!("materialize Gemot delegations: {error}"))?;
    let authority = GemotAuthorityView {
        authority: MootAuthority {
            delegations: &delegations,
            rules: &moot_snapshot.governance.rules,
            moot_id,
            now_ms: at_ms,
        },
    };

    // These are refresh-time facts for the local subject, evaluated through
    // the same converged authority and scoped capabilities used by the write
    // preflight above. They describe current effective authority only; a
    // subsequent authoring attempt must still recheck the live fold.
    let message_write = matches!(
        commons::CommonsAuthority::classify(
            &authority,
            servitor::Subject(open.subject),
            &commons::chat::chat_write_capability(open.binding.chat.0),
            servitor::Mode::Write,
        ),
        commons::AuthorityState::Effective
    );
    let graph_write = matches!(
        commons::CommonsAuthority::classify(
            &authority,
            servitor::Subject(open.subject),
            &commons::commons_write_capability(open.binding.root.0),
            servitor::Mode::Write,
        ),
        commons::AuthorityState::Effective
    );

    // The service view applies Gemot's complete converged read policy,
    // including membership and targeted withdrawals. Narrow the roster to
    // that set before any locally held capture text is grouped.
    let authorized_fauna = pollster::block_on(open.moot.authorized_fauna(at_ms))
        .map_err(|error| format!("materialize authorized Gemot fauna: {error}"))?;
    // Raw history only supplies candidate ids. Names and selectable versions
    // come from the service's current authorized projection.
    let collection_ids: BTreeSet<_> = moot_snapshot
        .roster
        .collections
        .iter()
        .map(|fact| match &fact.event {
            CollectionEvent::Declared { collection_id, .. } => *collection_id,
            CollectionEvent::Changed { collection, .. } => collection.collection_id,
        })
        .collect();
    let mut collection_choices = Vec::new();
    for id in collection_ids {
        if let Some(view) = pollster::block_on(open.moot.authorized_collection(id, at_ms))
            .map_err(|error| format!("materialize collection choice: {error}"))?
        {
            collection_choices.push(PlaceCollectionChoice {
                name: view.name,
                version: PlaceCollectionVersion {
                    moot: PlaceId(view.version.collection.moot_id),
                    collection: PlaceCollectionId(view.version.collection.collection_id.0),
                    frontier: view.version.frontier,
                    membership_commitment: view.version.membership_commitment,
                },
            });
        }
    }
    collection_choices.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.version.collection.0.cmp(&b.version.collection.0))
    });
    let (captured_fauna, captured_selection) = match &open.collection_selection {
        None => (
            authorized_fauna.clone(),
            CapturedCollectionSelection::AllEffective,
        ),
        Some(requested) if requested.moot.0 != moot_id => (
            Vec::new(),
            CapturedCollectionSelection::Collection {
                requested: requested.clone(),
                status: CapturedCollectionSelectionStatus::ForeignMoot,
            },
        ),
        Some(requested) => {
            let domain_requested = CollectionVersion {
                collection: CollectionRef {
                    moot_id: requested.moot.0,
                    collection_id: CollectionId(requested.collection.0),
                },
                frontier: requested.frontier.clone(),
                membership_commitment: requested.membership_commitment,
            };
            match pollster::block_on(
                open.moot
                    .authorized_collection(domain_requested.collection.collection_id, at_ms),
            )
            .map_err(|error| format!("materialize selected Gemot collection: {error}"))?
            {
                None => (
                    Vec::new(),
                    CapturedCollectionSelection::Collection {
                        requested: requested.clone(),
                        status: CapturedCollectionSelectionStatus::Unavailable,
                    },
                ),
                Some(view) if view.version != domain_requested => {
                    let current = PlaceCollectionVersion {
                        moot: PlaceId(view.version.collection.moot_id),
                        collection: PlaceCollectionId(view.version.collection.collection_id.0),
                        frontier: view.version.frontier,
                        membership_commitment: view.version.membership_commitment,
                    };
                    (
                        Vec::new(),
                        CapturedCollectionSelection::Collection {
                            requested: requested.clone(),
                            status: CapturedCollectionSelectionStatus::Stale { current },
                        },
                    )
                },
                Some(view) => {
                    let shares: BTreeSet<_> = view
                        .effective_selected
                        .iter()
                        .map(|reference| reference.share)
                        .collect();
                    let effective: Vec<_> = authorized_fauna
                        .iter()
                        .filter(|entry| shares.contains(&entry.op_hash))
                        .cloned()
                        .collect();
                    let status = CapturedCollectionSelectionStatus::Ready {
                        name: view.name,
                        effective_contributions: effective.len(),
                        pending_facts: view.pending.len(),
                    };
                    (
                        effective,
                        CapturedCollectionSelection::Collection {
                            requested: requested.clone(),
                            status,
                        },
                    )
                },
            }
        },
    };
    let captured_roster =
        crate::place::captured_collection::effective_roster(&moot_snapshot.roster, &captured_fauna);
    let captured = crate::place::captured_collection::remint(
        &captured_roster,
        &authority.authority,
        &open.capture_directory,
        &settings.capture_library,
    );

    let graph_projection = pollster::block_on(open.graph.projection_with_authority(&authority))
        .map_err(|error| format!("materialize Commons graph: {error}"))?;
    let chat_projection = pollster::block_on(open.chat.projection_with_authority(&authority))
        .map_err(|error| format!("materialize Commons chat: {error}"))?;

    let group_members = open
        .group
        .members()
        .map_err(|error| format!("materialize group membership: {error}"))?
        .len();
    let shared = crate::place::projection::SharedGraph::from_projection(&graph_projection);
    Ok(OfflinePlaceSnapshot {
        personae_root: open.subject,
        graph_digest: projection_digest(b"turnstone.place.graph-digest.v1", &shared),
        chat_digest: projection_digest(
            b"turnstone.place.chat-digest.v1",
            &canonical_chat(&chat_projection),
        ),
        moot: MootCache {
            membership_epoch: moot_snapshot.membership.epoch,
            members: moot_snapshot.membership.members.len(),
            roster_members: moot_snapshot.roster.members.len(),
            delegated_certificates: moot_snapshot.delegated_certificates,
            standing_operations: moot_snapshot.standing_operations,
        },
        graph: GraphCache {
            nodes: graph_projection.graph.graph().node_count(),
            edges: graph_projection.graph.graph().edge_count(),
            pending_causality: graph_projection.pending.len(),
            pending_authority: graph_projection.pending_authority.len(),
            revoked_authority: graph_projection.revoked.len(),
        },
        chat: ChatCache {
            channels: chat_projection.channels.len(),
            messages: chat_projection.messages.len(),
            deleted_messages: chat_projection.deleted_messages.len(),
            pending_causality: chat_projection.pending.len(),
            pending_authority: chat_projection.pending_authority.len(),
            revoked_authority: chat_projection.revoked.len(),
        },
        group: GroupCache {
            members: group_members,
            epochs: open.group.epoch_count(),
            has_current_epoch: open.group.current_epoch().is_some(),
        },
        captured,
        captured_selection,
        collection_choices,
        shared,
        sync: open.lanes.as_ref().map(|lanes| {
            let (local_rendezvous, dialed_rendezvous) = lanes.rendezvous();
            crate::place::PlaceSyncSnapshot {
                lanes: lanes.sync_snapshot(),
                projection: lanes.projection_snapshot(),
                local_rendezvous,
                dialed_rendezvous,
            }
        }),
        permissions: Some(crate::place::PlacePermissionSnapshot {
            message_write,
            graph_write,
        }),
    })
}

/// Spawn the retained-place worker. Each `Open` first releases the prior
/// session's database handles, so switch and trash can establish ordering with
/// the explicit `Release` acknowledgement.
pub fn spawn_place_worker(
    wake: Wake,
    identity: Arc<RootIdentity>,
    settings: PlaceWorkerSettings,
) -> (ActorHandle<PlaceWorkerCommand>, Receiver<Update>) {
    spawn_named(
        "turnstone-place",
        wake,
        move |commands, out: Emitter<Update>| {
            let mut live: Option<OpenPlace> = None;
            let mut live_scope: Option<(SessionId, u64)> = None;
            let mut lifecycle_generation = 0u64;
            while let Ok(command) = commands.recv() {
                match command {
                    PlaceWorkerCommand::Open {
                        session,
                        generation,
                        directory,
                        binding,
                    } => {
                        lifecycle_generation = lifecycle_generation.max(generation);
                        live = None;
                        live_scope = None;
                        match open_cached_place(&directory, &binding, identity.as_ref(), &settings)
                        {
                            Ok((opened, snapshot)) => {
                                live_scope = Some((session, generation));
                                live = Some(opened);
                                out.emit(Update::PlaceOpened {
                                    session,
                                    generation,
                                    result: Ok(snapshot),
                                });
                            },
                            Err(error) => out.emit(Update::PlaceOpened {
                                session,
                                generation,
                                result: Err(error),
                            }),
                        }
                    },
                    PlaceWorkerCommand::Join {
                        session,
                        generation,
                        directory,
                        invite,
                    } => {
                        lifecycle_generation = lifecycle_generation.max(generation);
                        // Admission first, then the ordinary cached open. The
                        // second step is not a formality: it proves the place
                        // admission just established actually reopens through
                        // the same path every later boot will use.
                        live = None;
                        live_scope = None;
                        let joined =
                            admit_invitation(&directory, &invite, identity.as_ref(), &settings)
                                .and_then(|admitted| {
                                    crate::place::rendezvous::save_admitted_rendezvous(
                                        &directory,
                                        &invite,
                                    )?;
                                    open_cached_place(
                                        &directory,
                                        &admitted.binding,
                                        identity.as_ref(),
                                        &settings,
                                    )
                                    .map(|(opened, snapshot)| (admitted.binding, opened, snapshot))
                                })
                                .and_then(|(binding, mut opened, snapshot)| {
                                    // Dial whatever the envelope offered. A ticketless
                                    // invitation still admits: the place is real and
                                    // offline, which Degraded-vs-Offline surfaces.
                                    let tickets: Vec<String> =
                                        invite.dialable().map(|entry| entry.hint.clone()).collect();
                                    if !tickets.is_empty() {
                                        opened.lanes = Some(crate::place::lanes::join_live(
                                            &opened,
                                            &binding,
                                            identity.as_ref(),
                                            &tickets,
                                            settings
                                                .projection_setup(&binding, identity.as_ref()),
                                            // The watcher reports arrivals under THIS
                                            // open's generation, so a nudge from a
                                            // departed place is dropped by the same
                                            // guard every other answer passes.
                                            Some((out.clone(), session, generation)),
                                        )?);
                                    }
                                    Ok((binding, opened, snapshot))
                                });
                        match joined {
                            Ok((binding, opened, snapshot)) => {
                                live_scope = Some((session, generation));
                                live = Some(opened);
                                out.emit(Update::PlaceJoined {
                                    session,
                                    generation,
                                    result: Ok((binding, snapshot)),
                                });
                            },
                            Err(error) => out.emit(Update::PlaceJoined {
                                session,
                                generation,
                                result: Err(error),
                            }),
                        }
                    },
                    PlaceWorkerCommand::Found {
                        session,
                        generation,
                        directory,
                        name,
                    } => {
                        lifecycle_generation = lifecycle_generation.max(generation);
                        live = None;
                        live_scope = None;
                        let founded = found_place(
                            &directory,
                            identity.as_ref(),
                            &name,
                            &settings,
                        )
                        .and_then(|binding| {
                            crate::session::save_place_binding(&directory, &binding)
                                .map_err(|error| format!("persist place binding: {error}"))?;
                            // An EMPTY descriptor, saved on purpose: reconnect
                            // must reach the same listen-only bind as this open.
                            crate::place::rendezvous::save_founder_rendezvous(
                                &directory, &binding,
                            )?;
                            open_cached_place(
                                &directory,
                                &binding,
                                identity.as_ref(),
                                &settings,
                            )
                            .map(|(opened, snapshot)| (binding, opened, snapshot))
                        })
                        .and_then(|(binding, mut opened, _stale)| {
                            opened.lanes = Some(crate::place::lanes::join_live(
                                &opened,
                                &binding,
                                identity.as_ref(),
                                &[],
                                settings.projection_setup(&binding, identity.as_ref()),
                                Some((out.clone(), session, generation)),
                            )?);
                            // Re-fold AFTER binding, so the answer already
                            // carries this bind's own rendezvous.
                            place_snapshot(&opened, binding.moot.0, &settings)
                                .map(|snapshot| (binding, opened, snapshot))
                        });
                        match founded {
                            Ok((binding, opened, snapshot)) => {
                                live_scope = Some((session, generation));
                                live = Some(opened);
                                out.emit(Update::PlaceFounded {
                                    session,
                                    generation,
                                    result: Ok((binding, snapshot)),
                                });
                            },
                            Err(error) => out.emit(Update::PlaceFounded {
                                session,
                                generation,
                                result: Err(error),
                            }),
                        }
                    },
                    PlaceWorkerCommand::OfferPrekey {
                        session,
                        generation,
                        directory,
                        moot,
                    } => {
                        let result =
                            prepare_group_identity(&directory, identity.as_ref(), moot);
                        out.emit(Update::PlacePrekeyOffered {
                            session,
                            generation,
                            result,
                        });
                    },
                    PlaceWorkerCommand::Invite {
                        session,
                        generation,
                        directory,
                        prekey,
                        access,
                    } => {
                        let result = match &live {
                            Some(_) if live_scope != Some((session, generation)) => Err(
                                "invitation belongs to a departed place generation".to_string(),
                            ),
                            None => Err("open a place before inviting anyone".to_string()),
                            Some(open) => {
                                let binding = open.binding.clone();
                                let now_ms = settings.authority_clock.now_ms();
                                // The rendezvous an invitation carries is THIS
                                // bind's ticket, minted when the endpoint came
                                // up. A founder must be live before it invites.
                                let rendezvous: Vec<_> = open
                                    .lanes
                                    .as_ref()
                                    .map(|lanes| lanes.rendezvous().0)
                                    .unwrap_or_default()
                                    .into_iter()
                                    .map(|hint| crate::place::invite::RendezvousV1 {
                                        carrier: crate::place::invite::P2PANDA_ENDPOINT_TICKET
                                            .into(),
                                        hint,
                                    })
                                    .collect();
                                GroupPrekeyBundle::from_bytes(&prekey)
                                    .map_err(|error| format!("decode offered pre-key: {error}"))
                                    .and_then(|bundle| {
                                        bundle.personae_root().map_err(|error| {
                                            format!("verify offered pre-key: {error}")
                                        })
                                    })
                                    .and_then(|joiner_root| {
                                        admit_member(
                                            &open.moot,
                                            &binding,
                                            identity.as_ref(),
                                            joiner_root,
                                            access,
                                            now_ms,
                                        )
                                    })
                                    .and_then(|authored| {
                                        let projection = authored.projection;
                                        // Retention alone leaves the admission
                                        // waiting on the next reconciliation
                                        // round; a connected peer hears it only
                                        // here. A founder with no lanes open
                                        // just retains it, as before.
                                        if let Some(lanes) = &open.lanes {
                                            if let Some(operation) = authored.membership {
                                                lanes.publish_membership(operation)?;
                                            }
                                            if let Some(operation) = authored.delegation {
                                                lanes.publish_delegation(operation)?;
                                            }
                                        }
                                        author_invitation_with(
                                            &open.moot,
                                            &directory,
                                            &binding,
                                            identity.as_ref(),
                                            &prekey,
                                            now_ms.saturating_add(INVITE_LIFETIME_MS),
                                            rendezvous,
                                            projection,
                                        )
                                    })
                                    .map(Box::new)
                            },
                        };
                        out.emit(Update::PlaceInvited {
                            session,
                            generation,
                            result,
                        });
                    },
                    PlaceWorkerCommand::Reconnect {
                        session,
                        generation,
                        directory,
                        binding,
                    } => {
                        if generation < lifecycle_generation {
                            out.emit(Update::PlaceOpened {
                                session,
                                generation,
                                result: Err("reconnect generation is stale".into()),
                            });
                            continue;
                        }
                        if generation == lifecycle_generation {
                            let result = if live_scope == Some((session, generation)) {
                                live.as_ref().map_or_else(
                                    || Err("reconnect has no open place".into()),
                                    |open| place_snapshot(open, open.binding.moot.0, &settings),
                                )
                            } else {
                                Err("reconnect generation was already released".into())
                            };
                            out.emit(Update::PlaceOpened {
                                session,
                                generation,
                                result,
                            });
                            continue;
                        }
                        lifecycle_generation = generation;
                        // This is the one close followed by a reopen of the
                        // same stores, so it is the one that pays for waiting.
                        if let Some(lanes) = live.as_mut().and_then(|open| open.lanes.as_mut()) {
                            lanes.leave_and_wait();
                        }
                        live = None;
                        live_scope = None;
                        let reconnected = reopen_cached_place(
                            &directory,
                            &binding,
                            identity.as_ref(),
                            &settings,
                        )
                        .and_then(|(mut opened, _cached)| {
                            let local_root = identity.master_public_key().to_bytes();
                            let membership = pollster::block_on(opened.moot.snapshot())
                                .map_err(|error| format!("materialize retained Moot: {error}"))?;
                            if !membership
                                .membership
                                .members
                                .iter()
                                .any(|member| member.member == local_root)
                            {
                                return Err(
                                    "this identity is no longer a member in retained place state"
                                        .into(),
                                );
                            }
                            let tickets = crate::place::rendezvous::load_rendezvous(
                                &directory,
                                &binding,
                                settings.authority_clock.now_ms(),
                            )?;
                            opened.lanes = Some(crate::place::lanes::join_live(
                                &opened,
                                &binding,
                                identity.as_ref(),
                                &tickets,
                                settings.projection_setup(&binding, identity.as_ref()),
                                Some((out.clone(), session, generation)),
                            )?);
                            place_snapshot(&opened, binding.moot.0, &settings)
                                .map(|snapshot| (opened, snapshot))
                        });
                        match reconnected {
                            Ok((opened, snapshot)) => {
                                live_scope = Some((session, generation));
                                live = Some(opened);
                                out.emit(Update::PlaceOpened {
                                    session,
                                    generation,
                                    result: Ok(snapshot),
                                });
                            }
                            Err(error) => out.emit(Update::PlaceOpened {
                                session,
                                generation,
                                result: Err(error),
                            }),
                        }
                    },
                    PlaceWorkerCommand::VisitDocument {
                        session,
                        generation,
                        directory,
                        holder_root,
                        path,
                        request,
                    } => {
                        let result = match &live {
                            _ if live_scope != Some((session, generation)) => {
                                Err("a visit belongs to a departed place generation".to_string())
                            },
                            Some(open) => visit_dial(
                                open,
                                &directory,
                                identity.as_ref(),
                                holder_root,
                                &settings,
                            ),
                            None => Err(
                                "a place-held document can only be visited from an open place"
                                    .to_string(),
                            ),
                        };
                        out.emit(Update::PlaceDocumentVisit {
                            session,
                            generation,
                            request,
                            holder_root,
                            path,
                            result: result.map(Box::new),
                        });
                    },
                    PlaceWorkerCommand::Resync {
                        session,
                        generation,
                    } => {
                        // Only meaningful with an open place; a resync of
                        // nothing answers with the error rather than silence.
                        let result = match &live {
                            Some(_) if live_scope != Some((session, generation)) => {
                                Err("resync belongs to a departed place generation".to_string())
                            }
                            Some(open) => place_snapshot(open, open.binding.moot.0, &settings),
                            None => Err("no open place to resync".to_string()),
                        };
                        out.emit(Update::PlaceOpened {
                            session,
                            generation,
                            result,
                        });
                    },
                    PlaceWorkerCommand::SetCollection {
                        session,
                        generation,
                        selection,
                    } => {
                        let result = match &mut live {
                            Some(_) if live_scope != Some((session, generation)) => Err(
                                "collection selection belongs to a departed place generation"
                                    .to_string(),
                            ),
                            Some(open) => {
                                let previous =
                                    std::mem::replace(&mut open.collection_selection, selection);
                                let result = place_snapshot(open, open.binding.moot.0, &settings)
                                    .and_then(|snapshot| {
                                        crate::session::save_place_collection(
                                            &open.directory,
                                            open.collection_selection.as_ref(),
                                        )?;
                                        Ok(snapshot)
                                    });
                                if result.is_err() {
                                    open.collection_selection = previous;
                                }
                                result
                            },
                            None => Err("no open place to select a collection in".to_string()),
                        };
                        out.emit(Update::PlaceCollectionSet {
                            session,
                            generation,
                            result,
                        });
                    },
                    PlaceWorkerCommand::Author {
                        session,
                        generation,
                        request,
                        command,
                    } => {
                        let result = match &mut live {
                            Some(open) => {
                                let binding = open.binding.clone();
                                author_into_place(
                                    open,
                                    &binding,
                                    identity.as_ref(),
                                    &command,
                                    &settings,
                                )
                                .and_then(|()| place_snapshot(open, binding.moot.0, &settings))
                            },
                            None => Err("no open place to author into".to_string()),
                        };
                        out.emit(Update::PlaceCommandDone {
                            session,
                            generation,
                            request,
                            result,
                        });
                    },
                    PlaceWorkerCommand::Release(ack) => {
                        live = None;
                        live_scope = None;
                        let _ = ack.send(());
                    },
                }
            }
            drop(live);
        },
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use chartulary::{Author, Container};
    use commons::chat::{Channel, ChatEvent, Message};
    use eidetic_fjall::FjallStore;
    use fleece::{TextPositionSelector, anchor_for_range, extract_document};
    use gemot::moot::constitution::{CapabilityGrant, ConstitutionRules};
    use gemot::moot::standing::Policy;
    use gemot::moot::{
        CollectionChange, ContributionRef, MOOT_ACT_ACTION, MOOT_DELEGATION_DOMAIN,
        MootAccessLevel, MootMember, MootMembershipAction,
    };
    use genet_static_dom::StaticDocument;
    use mere_document_lanes::FleeceAnnotationRecord;
    use mere_document_lanes::eidetic_bridge::{CaptureIdentity, FLEECE_ANNOTATION_SCHEMA_ID};
    use stickleback::DropExportProfile;

    use crate::place::invite::{ArtifactRefV1, PLACE_INVITE_VERSION};
    use identity::InMemoryProvider;
    use servitor::{Cap, cap_path};

    use identity::delegation::{
        CapabilityScope, DelegationCertificate, DelegationParent, DelegationRevocation,
        SignedDelegationCertificate, SignedDelegationRevocation, delegation_signing_salt,
    };

    use crate::place::PlaceInviteAccess::{Reader, Writer};
    use crate::place::{ChatSpaceId, PlaceId, SharedContainerId};

    /// Pinned so a delegation window, and therefore an authority verdict, is
    /// reproducible. `commons/container/...` and `commons/chat/...` both sit
    /// under the `commons` prefix these fixtures grant.
    const AUTHORITY_AT_MS: u64 = 50;
    const COLLECTION_GRANT: [u8; 32] = [0x68; 32];

    pub(crate) fn settings() -> PlaceWorkerSettings {
        PlaceWorkerSettings {
            authority_clock: AuthorityClock::Fixed(AUTHORITY_AT_MS),
            ..PlaceWorkerSettings::default()
        }
    }

    fn founder_for(binding: &PlaceBindingV1) -> InMemoryProvider {
        InMemoryProvider::from_seed([binding.moot.0[0].wrapping_add(40); 32])
    }

    /// The fixtures' constitution: the product rules at a pinned window, so
    /// an authority verdict stays reproducible. Same one grant, same id.
    pub(crate) fn place_rules(founder_id: [u8; 32], moot: [u8; 32]) -> ConstitutionRules {
        super::place_rules(founder_id, founder_id, moot, 10, Some(1_000))
    }

    pub(crate) fn founder_signing_key(
        founder: &InMemoryProvider,
        moot: [u8; 32],
    ) -> identity::Ed25519Keypair {
        super::founder_signing_key(founder, moot).unwrap()
    }

    /// The founder's signed delegation admitting one profile root to both
    /// Commons domains, at the pinned window. Deterministic, so a later test
    /// can recompute its id to revoke it.
    pub(crate) fn place_delegation(
        founder: &InMemoryProvider,
        moot: [u8; 32],
        subject: [u8; 32],
    ) -> SignedDelegationCertificate {
        super::place_delegation(founder, moot, subject, 15, 20, Some(900)).unwrap()
    }

    /// Withdraw the seeded delegation on the retained Gemot lane.
    fn revoke_place_delegation(directory: &Path, binding: &PlaceBindingV1, subject: [u8; 32]) {
        let founder = founder_for(binding);
        let founder_id = founder.master_public_key().to_bytes();
        let rules = place_rules(founder_id, binding.moot.0);
        let certificate = place_delegation(&founder, binding.moot.0, subject);
        let moot = pollster::block_on(MootFile::open(
            place_store_dir(directory).join("gemot"),
            MootId(binding.moot.0),
            founder_id,
            settings().retention,
        ))
        .unwrap();
        let revocation = SignedDelegationRevocation::issue(
            &founder,
            DelegationRevocation::new(
                certificate.certificate.id(),
                founder_id,
                certificate.certificate.scope.clone(),
                60,
                [2; 32],
            ),
        )
        .unwrap();
        pollster::block_on(moot.delegation_store().author_revoke(
            &founder_signing_key(&founder, binding.moot.0),
            &rules,
            revocation,
        ))
        .unwrap();
        drop(moot);
    }

    pub(crate) fn binding(seed: u8) -> PlaceBindingV1 {
        PlaceBindingV1::new(
            PlaceId([seed; 32]),
            SharedContainerId([seed.wrapping_add(1); 32]),
            ChatSpaceId([seed.wrapping_add(2); 32]),
            "hall",
        )
        .unwrap()
    }

    fn seed_profile(
        directory: &Path,
        identity: &RootIdentity,
        binding: &PlaceBindingV1,
        facts: usize,
    ) {
        let founder = founder_for(binding);
        let founder_id = founder.master_public_key().to_bytes();
        let settings = settings();
        let stores = place_store_dir(directory);
        std::fs::create_dir_all(&stores).unwrap();
        let rules = place_rules(founder_id, binding.moot.0);
        let moot = pollster::block_on(MootFile::open(
            stores.join("gemot"),
            MootId(binding.moot.0),
            founder_id,
            settings.retention.clone(),
        ))
        .unwrap();
        pollster::block_on(moot.found(
            founder.master_keypair().to_seed(),
            None,
            None,
            rules.clone(),
            1,
        ))
        .unwrap();
        // Without this the profile holds no capability and every fact it
        // authors below projects as pending, which is the correct verdict for
        // an unadmitted writer but not the fixture these tests need.
        pollster::block_on(moot.delegation_store().author_issue(
            &founder_signing_key(&founder, binding.moot.0),
            &rules,
            place_delegation(
                &founder,
                binding.moot.0,
                identity.master_public_key().to_bytes(),
            ),
        ))
        .unwrap();
        drop(moot);

        prepare_group_identity(directory, identity, binding.moot.0).unwrap();
        found_place_group(directory, identity, binding.moot.0).unwrap();
        let group = load_group_session(directory, identity, binding.moot.0).unwrap();
        let keyring = DataKeyring::from_bytes(&group.data_keyring_state().unwrap()).unwrap();

        let graph_backend = RedbBackend::open(stores.join("commons-graph.redb")).unwrap();
        let mut graph = Replica::for_identity(graph_backend, binding.root.0, identity).unwrap();
        for index in 0..facts {
            pollster::block_on(graph.edit(|log| {
                log.insert_node(
                    &Author::new("turnstone"),
                    Container::new(format!("node-{index}")),
                );
            }))
            .unwrap();
        }
        drop(graph);

        let chat_backend = RedbBackend::open(stores.join("commons-chat.redb")).unwrap();
        let mut chat =
            ChatReplica::for_identity(chat_backend, binding.chat.0, identity, keyring).unwrap();
        pollster::block_on(chat.author(ChatEvent::Channel(Channel {
            id: "hall".into(),
            title: "Hall".into(),
        })))
        .unwrap();
        for index in 0..facts {
            pollster::block_on(chat.author(ChatEvent::Message(Message {
                channel: "hall".into(),
                body: format!("message {index}"),
                sent_at_ms: index as u64,
                reply_to: None,
            })))
            .unwrap();
        }
    }

    /// Seed only the local Fleece records and their read-only projection.
    /// This deliberately creates no Moot or group state, so a live receiver
    /// can retain its own captures while its governed history arrives from a
    /// peer over the normal admission and lane paths.
    pub(crate) fn seed_exact_collection_captures(
        directory: &Path,
    ) -> (PlaceWorkerSettings, [u8; 32], [u8; 32]) {
        let capture_directory = crate::trail_memory::memory_dir(directory);
        let document = extract_document(&StaticDocument::parse(
            "<main><p>Exact collection selection keeps this field note.</p></main>",
        ));
        let anchor = anchor_for_range(
            &document.page.text,
            TextPositionSelector {
                start: 0,
                end: document.page.text.chars().count() as u64,
            },
            document.contract.quote_context,
        )
        .unwrap();
        let record = FleeceAnnotationRecord::from_fleece(
            CaptureIdentity::new(
                "https://collection.test/field-note",
                eidetic::Hash::of(b"exact-collection-source"),
            )
            .unwrap(),
            &document,
            &anchor,
        )
        .unwrap();
        let other_document = extract_document(&StaticDocument::parse(
            "<main><p>An unselected capture remains in the Moot.</p></main>",
        ));
        let other_anchor = anchor_for_range(
            &other_document.page.text,
            TextPositionSelector {
                start: 0,
                end: other_document.page.text.chars().count() as u64,
            },
            other_document.contract.quote_context,
        )
        .unwrap();
        let other_record = FleeceAnnotationRecord::from_fleece(
            CaptureIdentity::new(
                "https://collection.test/unselected",
                eidetic::Hash::of(b"unselected-collection-source"),
            )
            .unwrap(),
            &other_document,
            &other_anchor,
        )
        .unwrap();
        let (manifest, other_manifest) = {
            let mut store = FjallStore::open(&capture_directory).unwrap();
            pollster::block_on(mere_document_lanes::bootstrap_fleece_annotation_schema(
                &mut store,
            ))
            .unwrap();
            (
                pollster::block_on(mere_document_lanes::save_fleece_annotation(
                    &mut store, &record, 20,
                ))
                .unwrap(),
                pollster::block_on(mere_document_lanes::save_fleece_annotation(
                    &mut store,
                    &other_record,
                    21,
                ))
                .unwrap(),
            )
        };

        let settings = settings();
        {
            let mut store = FjallStore::open(&capture_directory).unwrap();
            crate::place::captured_collection::refresh_local_capture_library(
                &mut store,
                &capture_directory,
                &settings.capture_library,
            )
            .unwrap();
        }
        (
            settings,
            *manifest.0.as_bytes(),
            *other_manifest.0.as_bytes(),
        )
    }

    pub(crate) fn seed_exact_collection(
        directory: &Path,
        identity: &RootIdentity,
        binding: &PlaceBindingV1,
    ) -> (PlaceWorkerSettings, PlaceCollectionVersion, [u8; 32]) {
        seed_profile(directory, identity, binding, 0);
        let founder = founder_for(binding);
        assert_eq!(
            identity.master_public_key().to_bytes(),
            founder.master_public_key().to_bytes(),
            "the focused collection fixture uses the Moot founder"
        );
        let (settings, manifest, other_manifest) = seed_exact_collection_captures(directory);

        let collection_id = CollectionId([0xc7; 32]);
        let collection = CollectionRef {
            moot_id: binding.moot.0,
            collection_id,
        };
        let moot = pollster::block_on(MootFile::open_existing(
            place_store_dir(directory).join("gemot"),
            MootId(binding.moot.0),
            settings.retention.clone(),
        ))
        .unwrap();
        let mut rules = place_rules(founder.master_public_key().to_bytes(), binding.moot.0);
        rules.admission = Policy::MembersOnly {
            rate_limit: 20,
            rate_window_ms: 60_000,
        };
        rules.grant(CapabilityGrant {
            id: COLLECTION_GRANT,
            subject: founder.master_public_key().to_bytes(),
            path_prefix: cap_path(&Cap::scope("moot").unwrap()),
            not_before_ms: 1,
            expires_at_ms: Some(1_000),
            delegation_depth: 0,
        });
        pollster::block_on(moot.amend(founder.master_keypair().to_seed(), rules, 2)).unwrap();
        pollster::block_on(moot.membership_store().author_for_identity(
            &founder,
            MootMembershipAction::Create {
                initial_members: vec![MootMember {
                    member: founder.master_public_key().to_bytes(),
                    access: MootAccessLevel::Manage,
                }],
            },
        ))
        .unwrap();
        let share = pollster::block_on(moot.share_for_identity(
            &founder,
            manifest,
            FLEECE_ANNOTATION_SCHEMA_ID.into(),
            "Field note".into(),
            20,
        ))
        .unwrap();
        pollster::block_on(moot.share_for_identity(
            &founder,
            other_manifest,
            FLEECE_ANNOTATION_SCHEMA_ID.into(),
            "Unselected note".into(),
            21,
        ))
        .unwrap();
        pollster::block_on(moot.declare_collection_for_identity(
            &founder,
            collection_id,
            "Field notes".into(),
            None,
            22,
        ))
        .unwrap();
        let declared =
            pollster::block_on(moot.authorized_collection(collection_id, AUTHORITY_AT_MS))
                .unwrap()
                .unwrap();
        pollster::block_on(moot.set_collection_membership_for_identity(
            &founder,
            collection,
            declared.heads,
            CollectionChange::SetMembership {
                contribution: ContributionRef {
                    moot_id: binding.moot.0,
                    share: share.operation,
                },
                included: true,
            },
            23,
        ))
        .unwrap();
        let version =
            pollster::block_on(moot.authorized_collection(collection_id, AUTHORITY_AT_MS))
                .unwrap()
                .unwrap()
                .version;
        drop(moot);
        (
            settings,
            PlaceCollectionVersion {
                moot: PlaceId(version.collection.moot_id),
                collection: PlaceCollectionId(version.collection.collection_id.0),
                frontier: version.frontier,
                membership_commitment: version.membership_commitment,
            },
            share.operation,
        )
    }

    #[test]
    fn two_profiles_reopen_their_own_retained_place_state() {
        let root =
            std::env::temp_dir().join(format!("turnstone-place-worker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let first_dir = root.join("first");
        let second_dir = root.join("second");
        let first_identity = RootIdentity::Unsealed(InMemoryProvider::from_seed([0x81; 32]));
        let second_identity = RootIdentity::Unsealed(InMemoryProvider::from_seed([0x82; 32]));
        let first_binding = binding(0x21);
        let second_binding = binding(0x31);
        seed_profile(&first_dir, &first_identity, &first_binding, 1);
        seed_profile(&second_dir, &second_identity, &second_binding, 2);

        let (_, first) =
            open_cached_place(&first_dir, &first_binding, &first_identity, &settings()).unwrap();
        let (_, second) =
            open_cached_place(&second_dir, &second_binding, &second_identity, &settings()).unwrap();
        assert_eq!((first.graph.nodes, first.chat.messages), (1, 1));
        assert_eq!((second.graph.nodes, second.chat.messages), (2, 2));
        assert!(first.group.has_current_epoch);
        assert!(second.group.has_current_epoch);
        assert_eq!(first.moot.delegated_certificates, 1);
        assert_eq!(second.moot.delegated_certificates, 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Build a real invitation: a founded Moot whose membership contains both
    /// roots, exported as a plain drop, plus a genuine Stickleback welcome
    /// addressed to the joiner's registered pre-key.
    fn real_invitation(
        founder_dir: &Path,
        binding: &PlaceBindingV1,
        founder: &InMemoryProvider,
        joiner_published_prekey: Vec<u8>,
        joiner_id: [u8; 32],
    ) -> PlaceInviteV1 {
        let founder_id = founder.master_public_key().to_bytes();
        let stores = place_store_dir(founder_dir);
        std::fs::create_dir_all(&stores).unwrap();
        let moot = pollster::block_on(MootFile::open(
            stores.join("gemot"),
            MootId(binding.moot.0),
            founder_id,
            settings().retention,
        ))
        .unwrap();
        pollster::block_on(moot.found(
            founder.master_keypair().to_seed(),
            None,
            None,
            place_rules(founder_id, binding.moot.0),
            1,
        ))
        .unwrap();
        pollster::block_on(moot.membership_store().author_for_identity(
            founder,
            MootMembershipAction::Create {
                initial_members: vec![MootMember {
                    member: founder_id,
                    access: MootAccessLevel::Manage,
                }],
            },
        ))
        .unwrap();
        pollster::block_on(moot.membership_store().author_for_identity(
            founder,
            MootMembershipAction::Add {
                member: joiner_id,
                access: MootAccessLevel::Write,
            },
        ))
        .unwrap();

        let mut drop_bytes = Vec::new();
        pollster::block_on(moot.export_plain_drop(
            &mut drop_bytes,
            DropExportProfile::default(),
            DropLimits::default(),
        ))
        .unwrap();
        let membership_heads = pollster::block_on(moot.snapshot())
            .unwrap()
            .membership
            .auth_heads;
        drop(moot);

        // The joiner's group identity already exists and its pre-key is
        // published: that is the precondition for being invitable at all, since
        // the recipient id comes from the RNG rather than the Personae root.
        let joiner_prekey = GroupPrekeyBundle::from_bytes(&joiner_published_prekey).unwrap();
        let joiner_recipient = joiner_prekey.recipient;
        let (mut founder_group, founder_prekey) =
            GroupSession::new(GroupSessionId(binding.moot.0), founder).unwrap();
        founder_group.register_prekey(&joiner_prekey).unwrap();
        founder_group.create(&[]).unwrap();
        let dispatch = founder_group.add(joiner_recipient).unwrap();
        let direct = dispatch.direct_for(joiner_recipient).unwrap();

        PlaceInviteV1 {
            version: PLACE_INVITE_VERSION,
            binding: binding.clone(),
            founder: founder_id,
            inviter: founder_id,
            inviter_prekey: inline_artifact(&founder_prekey.to_bytes().unwrap()),
            governance: inline_artifact(&drop_bytes),
            key_welcome: inline_artifact(&dispatch.control.to_bytes().unwrap()),
            key_direct: inline_artifact(&direct.to_bytes().unwrap()),
            expected_epoch: founder_group
                .current_epoch()
                .expect("adding a member installs an epoch"),
            membership_heads,
            // Comfortably after the pinned AUTHORITY_AT_MS, so only the test
            // that moves the clock forward sees an expiry.
            not_after_ms: AUTHORITY_AT_MS + 1_000,
            rendezvous: Vec::new(),
            projection_grant: None,
        }
    }

    fn inline_artifact(bytes: &[u8]) -> ArtifactRefV1 {
        ArtifactRefV1::Inline {
            media_type: "application/vnd.mere.place-artifact".into(),
            digest: proofs::Digest::blake3(bytes)
                .bytes
                .as_slice()
                .try_into()
                .unwrap(),
            bytes: bytes.to_vec(),
        }
    }

    /// A joiner directory with a prepared identity, plus an invitation whose
    /// welcome is genuinely addressed to it.
    fn matched_case(
        root: &Path,
        name: &str,
        binding: &PlaceBindingV1,
        founder: &InMemoryProvider,
        joiner: &RootIdentity,
    ) -> (PathBuf, PlaceInviteV1) {
        let directory = root.join(name);
        let published = prepare_group_identity(&directory, joiner, binding.moot.0).unwrap();
        let invite = real_invitation(
            &root.join(format!("{name}-founder")),
            binding,
            founder,
            published,
            joiner.master_public_key().to_bytes(),
        );
        (directory, invite)
    }

    /// (Gemot store, sealed secrets, admitted binding).
    fn residue(directory: &Path) -> (bool, bool, bool) {
        (
            place_store_dir(directory).exists(),
            place_secrets_dir(directory).exists(),
            crate::session::place_binding_path(directory).exists(),
        )
    }

    #[test]
    fn a_valid_invitation_admits_and_a_refused_one_leaves_nothing_behind() {
        let root =
            std::env::temp_dir().join(format!("turnstone-place-admit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let founder = InMemoryProvider::from_seed([0xb1; 32]);
        let joiner = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xb2; 32]));
        let stranger = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xb3; 32]));
        let binding = binding(0x61);
        let joined = root.join("joiner");
        let joiner_id = joiner.master_public_key().to_bytes();

        // Being invitable is a precondition, not a consequence: the identity
        // exists and its pre-key is published before any envelope is authored.
        let published = prepare_group_identity(&joined, &joiner, binding.moot.0).unwrap();
        assert_eq!(
            prepare_group_identity(&joined, &joiner, binding.moot.0).unwrap(),
            published,
            "preparing twice must not rotate the identity a welcome is addressed to"
        );
        let invite = real_invitation(
            &root.join("founder"),
            &binding,
            &founder,
            published.clone(),
            joiner_id,
        );

        // The happy path: every domain answers, and only then is a secret sealed.
        let admitted =
            admit_invitation(&joined, &invite, &joiner, &settings()).expect("valid invitation");
        assert_eq!(admitted.binding, binding);
        assert_eq!(admitted.moot.members, 2, "Gemot's governance fold");
        assert_eq!(admitted.group_members, 2, "Stickleback's crypto fold");
        assert_eq!(residue(&joined), (true, true, true));

        // A tampered artifact never reaches a domain, and leaves no store.
        let tampered_dir = root.join("tampered");
        let mut tampered = invite.clone();
        let ArtifactRefV1::Inline { bytes, .. } = &mut tampered.governance else {
            unreachable!("fixture is inline")
        };
        bytes.push(0);
        let error = admit_invitation(&tampered_dir, &tampered, &joiner, &settings()).unwrap_err();
        assert!(error.contains("declared digest"), "{error}");
        assert_eq!(residue(&tampered_dir), (false, false, false));

        // A stranger holding the same envelope is refused by Gemot membership,
        // not by anything the envelope says about itself.
        let stranger_dir = root.join("stranger");
        let error = admit_invitation(&stranger_dir, &invite, &stranger, &settings()).unwrap_err();
        assert!(error.contains("membership does not contain"), "{error}");
        assert_eq!(residue(&stranger_dir), (false, false, false));

        // An envelope naming a non-member as its author is refused even though
        // the welcome frames themselves are genuine.
        let forged_dir = root.join("forged-author");
        let mut forged = invite.clone();
        forged.inviter = stranger.master_public_key().to_bytes();
        let error = admit_invitation(&forged_dir, &forged, &joiner, &settings()).unwrap_err();
        assert!(error.contains("outside Gemot membership"), "{error}");
        assert_eq!(residue(&forged_dir), (false, false, false));

        // Check 4's two refusals need a welcome genuinely addressed to the
        // directory under test, since every prepared identity draws a fresh
        // random recipient. Each gets its own matched invitation.
        //
        // An epoch other than the one the invitation names is refused even
        // though the welcome itself is genuine and processes cleanly.
        let (wrong_epoch_dir, mut wrong_epoch) =
            matched_case(&root, "wrong-epoch", &binding, &founder, &joiner);
        wrong_epoch.expected_epoch = [0xee; 32];
        let error =
            admit_invitation(&wrong_epoch_dir, &wrong_epoch, &joiner, &settings()).unwrap_err();
        assert!(error.contains("does not name"), "{error}");

        // Membership heads the inviter pinned but Gemot did not converge to.
        // This is what stops a welcome minted before a removal from handing the
        // joiner a key a departed member still holds.
        let (stale_heads_dir, mut stale_heads) =
            matched_case(&root, "stale-heads", &binding, &founder, &joiner);
        stale_heads.membership_heads = vec![[0xaa; 32]];
        let error =
            admit_invitation(&stale_heads_dir, &stale_heads, &joiner, &settings()).unwrap_err();
        assert!(error.contains("did not converge"), "{error}");

        // The inviter's own time bound, on an otherwise valid envelope, with a
        // clock moved past it. Nothing is created, so expiry is cheap and
        // cannot be reached by an envelope that would have failed anyway.
        let expired_dir = root.join("expired");
        let expired_clock = PlaceWorkerSettings {
            authority_clock: AuthorityClock::Fixed(invite.not_after_ms + 1),
            ..PlaceWorkerSettings::default()
        };
        let error = admit_invitation(&expired_dir, &invite, &joiner, &expired_clock).unwrap_err();
        assert!(error.contains("expired"), "{error}");
        assert_eq!(residue(&expired_dir), (false, false, false));

        // Aliased Commons scopes are refused before any store is created.
        let aliased_dir = root.join("aliased");
        let mut aliased = invite.clone();
        aliased.binding.chat = ChatSpaceId(aliased.binding.root.0);
        let error = admit_invitation(&aliased_dir, &aliased, &joiner, &settings()).unwrap_err();
        assert!(error.contains("same scope"), "{error}");
        assert_eq!(residue(&aliased_dir), (false, false, false));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_authored_invitation_admits_on_the_other_side() {
        let root =
            std::env::temp_dir().join(format!("turnstone-place-author-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let founder = InMemoryProvider::from_seed([0xc1; 32]);
        let joiner = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xc2; 32]));
        let outsider = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xc3; 32]));
        let binding = binding(0x71);
        let host = root.join("host");
        let guest = root.join("guest");

        // Both sides prepare a durable group identity before any envelope
        // exists. This is the precondition, not a step in the flow.
        let joiner_prekey = prepare_group_identity(&guest, &joiner, binding.moot.0).unwrap();
        found_place_for_authoring(
            &host,
            &binding,
            &founder,
            joiner.master_public_key().to_bytes(),
        );
        // The host founds its crypto group; the guest only prepares an
        // identity. Different preconditions for different roles.
        found_place_group(&host, &founder, binding.moot.0).unwrap();
        assert!(
            found_place_group(&host, &founder, binding.moot.0).is_ok(),
            "founding twice must not strand the epochs already handed out"
        );

        // Someone the Moot never admitted cannot be invited, and the refusal
        // names the real reason rather than minting an envelope that could
        // only ever be refused on arrival.
        let outsider_prekey =
            prepare_group_identity(&root.join("outsider"), &outsider, binding.moot.0).unwrap();
        let error = author_invitation(
            &host,
            &binding,
            &founder,
            &outsider_prekey,
            AUTHORITY_AT_MS + 1_000,
            Vec::new(),
            None,
            &settings(),
        )
        .unwrap_err();
        assert!(
            error.contains("does not contain the invited root"),
            "{error}"
        );

        let invite = author_invitation(
            &host,
            &binding,
            &founder,
            &joiner_prekey,
            AUTHORITY_AT_MS + 1_000,
            Vec::new(),
            None,
            &settings(),
        )
        .unwrap();

        // The product path produced it and the product path accepts it: no
        // test fixture stands between the two sides.
        let admitted = admit_invitation(&guest, &invite, &joiner, &settings())
            .expect("an authored invitation admits");
        assert_eq!(admitted.binding, binding);
        assert_eq!(admitted.moot.members, 2);
        assert_eq!(admitted.group_members, 2);
        assert_eq!(residue(&guest), (true, true, true));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Found a Moot with both roots in its membership fold, as the authoring
    /// side would already have.
    pub(crate) fn found_place_for_authoring(
        directory: &Path,
        binding: &PlaceBindingV1,
        founder: &InMemoryProvider,
        joiner_root: [u8; 32],
    ) {
        let founder_id = founder.master_public_key().to_bytes();
        let stores = place_store_dir(directory);
        std::fs::create_dir_all(&stores).unwrap();
        let moot = pollster::block_on(MootFile::open(
            stores.join("gemot"),
            MootId(binding.moot.0),
            founder_id,
            settings().retention,
        ))
        .unwrap();
        pollster::block_on(moot.found(
            founder.master_keypair().to_seed(),
            None,
            None,
            place_rules(founder_id, binding.moot.0),
            1,
        ))
        .unwrap();
        pollster::block_on(moot.membership_store().author_for_identity(
            founder,
            MootMembershipAction::Create {
                initial_members: vec![MootMember {
                    member: founder_id,
                    access: MootAccessLevel::Manage,
                }],
            },
        ))
        .unwrap();
        pollster::block_on(moot.membership_store().author_for_identity(
            founder,
            MootMembershipAction::Add {
                member: joiner_root,
                access: MootAccessLevel::Write,
            },
        ))
        .unwrap();
    }


    /// Found, offer, invite, admit — through the product functions only, with
    /// no fixture standing between the two profiles.
    ///
    /// The refusal in the middle is the point of the ordering: a pre-key whose
    /// root the Moot has never admitted cannot be invited, so an envelope is
    /// only ever minted for someone governance already knows.
    #[test]
    fn a_founded_place_invites_an_offered_prekey_and_admits_it() {
        let root = std::env::temp_dir()
            .join(format!("turnstone-place-found-{}", uuid::Uuid::new_v4()));
        let host = root.join("host");
        let guest = root.join("guest");
        std::fs::create_dir_all(&host).unwrap();
        std::fs::create_dir_all(&guest).unwrap();
        let host_identity = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xd1; 32]));
        let guest_identity = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xd2; 32]));
        // The product clock, not a fixture's: the windows under test are the
        // ones a real founding opens.
        let settings = PlaceWorkerSettings::default();
        let now_ms = settings.authority_clock.now_ms();

        let binding = found_place(&host, &host_identity, "Hearth", &settings).unwrap();
        assert_eq!(binding.default_channel, DEFAULT_CHANNEL);
        assert_ne!(binding.moot.0, binding.root.0);
        assert_ne!(binding.moot.0, binding.chat.0);
        assert_ne!(binding.root.0, binding.chat.0);

        // The founder's own writes read Effective against its own Moot; the
        // self-delegation is what makes that true.
        let (opened, snapshot) =
            open_cached_place(&host, &binding, &host_identity, &settings).unwrap();
        let permissions = snapshot.permissions.clone().unwrap();
        assert!(permissions.message_write, "the founder may speak here");
        assert!(permissions.graph_write, "the founder may share here");
        assert_eq!(snapshot.moot.members, 1);
        assert_eq!(snapshot.chat.channels, 1);
        assert_eq!(snapshot.personae_root, host_identity.master_public_key().to_bytes());
        assert!(!snapshot.graph_digest.is_empty() && !snapshot.chat_digest.is_empty());
        drop(opened);

        // The guest publishes its group identity for this Moot. An offer, not
        // an admission: nothing on the host has changed yet.
        let prekey = prepare_group_identity(&guest, &guest_identity, binding.moot.0).unwrap();
        let refused = author_invitation(
            &host,
            &binding,
            &host_identity,
            &prekey,
            now_ms + 60_000,
            Vec::new(),
            None,
            &settings,
        )
        .unwrap_err();
        assert!(
            refused.contains("membership does not contain the invited root"),
            "{refused}"
        );

        let moot = pollster::block_on(MootFile::open_existing(
            place_store_dir(&host).join("gemot"),
            MootId(binding.moot.0),
            settings.retention.clone(),
        ))
        .unwrap();
        let guest_root = guest_identity.master_public_key().to_bytes();
        admit_member(&moot, &binding, &host_identity, guest_root, Writer, now_ms).unwrap();
        // Idempotent: inviting the same root twice must not double the fold.
        let projection_grant =
            admit_member(&moot, &binding, &host_identity, guest_root, Writer, now_ms)
                .unwrap()
                .projection;
        assert!(
            projection_grant.is_some(),
            "a writer is admitted at the projection door as well as the Moot"
        );
        let invite = author_invitation_with(
            &moot,
            &host,
            &binding,
            &host_identity,
            &prekey,
            now_ms + 7 * 24 * 60 * 60 * 1000,
            Vec::new(),
            projection_grant,
        )
        .unwrap();
        drop(moot);

        let admitted = admit_invitation(&guest, &invite, &guest_identity, &settings).unwrap();
        assert_eq!(admitted.binding, binding);
        assert_eq!(admitted.moot.members, 2);
        assert_eq!(admitted.group_members, 2);

        // The admitted guest reopens through the ordinary path and holds the
        // capability the delegation granted.
        let (guest_open, guest_snapshot) =
            open_cached_place(&guest, &binding, &guest_identity, &settings).unwrap();
        assert!(guest_snapshot.permissions.clone().unwrap().message_write);
        assert_eq!(guest_snapshot.personae_root, guest_root);
        // Two peers that have converged on nothing yet still agree on the
        // digest of nothing; the point is that the digest is comparable.
        assert_eq!(guest_snapshot.graph_digest, snapshot.graph_digest);
        drop(guest_open);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A reader's invitation: membership without delegation.
    ///
    /// The two halves are separate questions and this proves they stay
    /// separate. Reading is answered by the welcome and the Moot's fold, so
    /// the founder's shared node reaches the reader's projection; authoring
    /// is answered by a capability nobody issued, so the reader's own worker
    /// refuses it before a store or a lane sees anything.
    #[test]
    fn a_reader_invitation_admits_a_member_that_cannot_author() {
        let root = std::env::temp_dir()
            .join(format!("turnstone-place-reader-{}", uuid::Uuid::new_v4()));
        let host = root.join("host");
        let guest = root.join("guest");
        std::fs::create_dir_all(&host).unwrap();
        std::fs::create_dir_all(&guest).unwrap();
        let host_identity = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xd3; 32]));
        let reader_identity = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xd4; 32]));
        let settings = PlaceWorkerSettings::default();
        let now_ms = settings.authority_clock.now_ms();

        let binding = found_place(&host, &host_identity, "Hearth", &settings).unwrap();

        // Authored BEFORE the invitation, so what the reader reads is content
        // that existed before it was anybody here.
        const SHARED: &str = "https://shared.example/page";
        let (mut host_open, _) =
            open_cached_place(&host, &binding, &host_identity, &settings).unwrap();
        author_into_place(
            &mut host_open,
            &binding,
            &host_identity,
            &PlaceCommand::ShareNode { address: SHARED.into() },
            &settings,
        )
        .unwrap();
        let host_snapshot = place_snapshot(&host_open, binding.moot.0, &settings).unwrap();
        assert_eq!(host_snapshot.graph.nodes, 1);
        drop(host_open);

        let prekey = prepare_group_identity(&guest, &reader_identity, binding.moot.0).unwrap();
        let reader_root = reader_identity.master_public_key().to_bytes();
        let moot = pollster::block_on(MootFile::open_existing(
            place_store_dir(&host).join("gemot"),
            MootId(binding.moot.0),
            settings.retention.clone(),
        ))
        .unwrap();
        admit_member(&moot, &binding, &host_identity, reader_root, Reader, now_ms).unwrap();
        // Idempotent for a reader too: no second membership fact, no
        // delegation sneaking in on the way through.
        let projection_grant =
            admit_member(&moot, &binding, &host_identity, reader_root, Reader, now_ms)
                .unwrap()
                .projection;
        assert!(
            projection_grant.is_none(),
            "a reader is admitted to the Moot and to no projection door"
        );
        let invite = author_invitation_with(
            &moot,
            &host,
            &binding,
            &host_identity,
            &prekey,
            now_ms + 7 * 24 * 60 * 60 * 1000,
            Vec::new(),
            projection_grant,
        )
        .unwrap();
        drop(moot);

        let admitted = admit_invitation(&guest, &invite, &reader_identity, &settings).unwrap();
        assert_eq!(admitted.moot.members, 2);
        assert_eq!(admitted.group_members, 2);

        // The graph lane's delivery, offline: the reader ends up holding the
        // operations the founder authored, exactly as a sync round would
        // leave it. Everything proved below is a verdict on those operations.
        std::fs::copy(
            place_store_dir(&host).join("commons-graph.redb"),
            place_store_dir(&guest).join("commons-graph.redb"),
        )
        .unwrap();

        let (mut reader_open, reader_snapshot) =
            open_cached_place(&guest, &binding, &reader_identity, &settings).unwrap();
        assert_eq!(
            reader_snapshot.permissions,
            Some(crate::place::PlacePermissionSnapshot {
                message_write: false,
                graph_write: false,
            }),
            "a reader holds neither write capability"
        );
        assert_eq!(reader_snapshot.personae_root, reader_root);
        // Reading needs the welcome, not a delegation: the founder's node is
        // here, and both peers digest it the same way.
        assert_eq!(reader_snapshot.graph.nodes, 1);
        assert_eq!(
            reader_snapshot.shared.addresses().collect::<Vec<_>>(),
            vec![SHARED]
        );
        assert_eq!(reader_snapshot.graph_digest, host_snapshot.graph_digest);
        assert_eq!(reader_snapshot.graph.pending_authority, 0);

        // Its own worker refuses both authoring paths, with the reason the
        // product surfaces as a place refusal.
        for command in [
            PlaceCommand::ShareNode { address: "https://reader.example/page".into() },
            PlaceCommand::SendMessage {
                channel: binding.default_channel.clone(),
                body: "refused".into(),
            },
        ] {
            let refused = author_into_place(
                &mut reader_open,
                &binding,
                &reader_identity,
                &command,
                &settings,
            )
            .unwrap_err();
            assert!(refused.contains("no effective capability"), "{refused}");
        }
        // The refusal authored nothing: the shared graph is still the
        // founder's one node.
        let after = place_snapshot(&reader_open, binding.moot.0, &settings).unwrap();
        assert_eq!(after.graph.nodes, 1);
        assert_eq!(after.chat.messages, 0);
        drop(reader_open);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_revoked_member_reaches_no_projected_place_state() {
        let root =
            std::env::temp_dir().join(format!("turnstone-place-revoked-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let directory = root.join("profile");
        let identity = RootIdentity::Unsealed(InMemoryProvider::from_seed([0xa1; 32]));
        let binding = binding(0x51);
        seed_profile(&directory, &identity, &binding, 2);

        let (_, admitted) =
            open_cached_place(&directory, &binding, &identity, &settings()).unwrap();
        assert_eq!((admitted.graph.nodes, admitted.chat.messages), (2, 2));
        assert_eq!(admitted.permissions, Some(crate::place::PlacePermissionSnapshot {
            message_write: true, graph_write: true,
        }));
        assert_eq!(admitted.chat.channels, 1);
        assert_eq!(
            (
                admitted.graph.pending_authority,
                admitted.graph.revoked_authority,
                admitted.chat.pending_authority,
                admitted.chat.revoked_authority,
            ),
            (0, 0, 0, 0)
        );

        revoke_place_delegation(
            &directory,
            &binding,
            identity.master_public_key().to_bytes(),
        );

        let (mut withdrawn_open, withdrawn) =
            open_cached_place(&directory, &binding, &identity, &settings()).unwrap();
        assert_eq!(withdrawn.permissions, Some(crate::place::PlacePermissionSnapshot {
            message_write: false, graph_write: false,
        }));
        assert!(author_into_place(
            &mut withdrawn_open, &binding, &identity,
            &PlaceCommand::SendMessage { channel: "hall".into(), body: "refused after revocation".into() },
            &settings(),
        ).unwrap_err().contains("no effective capability"));
        drop(withdrawn_open);
        assert_eq!(
            (withdrawn.graph.nodes, withdrawn.graph.edges),
            (0, 0),
            "a withdrawn member's graph facts must not reach PlaceState"
        );
        assert_eq!(
            (
                withdrawn.chat.messages,
                withdrawn.chat.channels,
                withdrawn.chat.deleted_messages
            ),
            (0, 0, 0),
            "a withdrawn member's chat facts must not reach PlaceState"
        );
        assert!(withdrawn.graph.pending_authority == 0);
        assert!(withdrawn.chat.pending_authority == 0);
        // The facts stay retained and attributable: revocation withholds them
        // from the projection, it does not erase them.
        assert_eq!(withdrawn.graph.revoked_authority, 2);
        assert_eq!(withdrawn.chat.revoked_authority, 3);
        assert_eq!(withdrawn.moot.delegated_certificates, 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn worker_sets_and_clears_an_exact_collection_selection() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("profile");
        let identity = Arc::new(RootIdentity::Unsealed(InMemoryProvider::from_seed(
            [0x93; 32],
        )));
        let binding = binding(0x43);
        seed_profile(&directory, identity.as_ref(), &binding, 1);

        let wake: Wake = Arc::new(|| {});
        let (worker, updates) = spawn_place_worker(wake, identity, settings());
        let session = SessionId::new();
        worker.command(PlaceWorkerCommand::Open {
            session,
            generation: 1,
            directory,
            binding,
        });
        assert!(matches!(
            updates
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            Update::PlaceOpened { result: Ok(_), .. }
        ));

        let requested = PlaceCollectionVersion {
            moot: PlaceId([0xfe; 32]),
            collection: PlaceCollectionId([7; 32]),
            frontier: Vec::new(),
            membership_commitment: [0; 32],
        };
        worker.command(PlaceWorkerCommand::SetCollection {
            session,
            generation: 1,
            selection: Some(requested.clone()),
        });
        let selected = updates
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let Update::PlaceCollectionSet {
            result: Ok(snapshot),
            ..
        } = selected
        else {
            panic!("collection selection did not return a snapshot");
        };
        assert!(snapshot.captured.groups.is_empty());
        assert!(snapshot.captured.rejected.is_empty());
        assert_eq!(
            snapshot.captured_selection,
            CapturedCollectionSelection::Collection {
                requested,
                status: CapturedCollectionSelectionStatus::ForeignMoot,
            }
        );

        worker.command(PlaceWorkerCommand::SetCollection {
            session,
            generation: 1,
            selection: None,
        });
        let cleared = updates
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(matches!(
            cleared,
            Update::PlaceCollectionSet {
                result: Ok(OfflinePlaceSnapshot {
                    captured_selection: CapturedCollectionSelection::AllEffective,
                    ..
                }),
                ..
            }
        ));
    }

    #[test]
    fn reconnect_refuses_removed_local_membership_before_dialing_and_keeps_cache() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("profile");
        let binding = binding(0x4c);
        let identity = Arc::new(RootIdentity::Unsealed(InMemoryProvider::from_seed(
            [0x95; 32],
        )));
        seed_profile(&directory, identity.as_ref(), &binding, 2);

        // This is retained membership state, not a claim about a remote
        // revocation that has not arrived. The founder removes this profile
        // before the worker tries to turn saved contact data into a live lane.
        let founder = founder_for(&binding);
        let moot = pollster::block_on(MootFile::open_existing(
            place_store_dir(&directory).join("gemot"),
            MootId(binding.moot.0),
            settings().retention,
        ))
        .unwrap();
        pollster::block_on(moot.membership_store().author_for_identity(
            &founder,
            MootMembershipAction::Create {
                initial_members: vec![
                    MootMember {
                        member: founder.master_public_key().to_bytes(),
                        access: MootAccessLevel::Manage,
                    },
                    MootMember {
                        member: identity.master_public_key().to_bytes(),
                        access: MootAccessLevel::Write,
                    },
                ],
            },
        ))
        .unwrap();
        pollster::block_on(moot.membership_store().author_for_identity(
            &founder,
            MootMembershipAction::Remove {
                member: identity.master_public_key().to_bytes(),
            },
        ))
        .unwrap();
        drop(moot);

        let session = SessionId::new();
        let (worker, updates) = spawn_place_worker(Arc::new(|| {}), identity.clone(), settings());
        worker.command(PlaceWorkerCommand::Reconnect {
            session,
            generation: 1,
            directory: directory.clone(),
            binding: binding.clone(),
        });
        assert!(matches!(
            updates
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            Update::PlaceOpened {
                session: received_session,
                generation: 1,
                result: Err(ref error),
            } if received_session == session
                && error == "this identity is no longer a member in retained place state"
        ));

        let (_, cached) =
            open_cached_place(&directory, &binding, identity.as_ref(), &settings()).unwrap();
        assert_eq!((cached.graph.nodes, cached.chat.messages), (2, 2));
    }

    /// Reconnect pressed while the place is already live. The app bumps the
    /// generation, so the worker closes the bind and reopens the same stores
    /// inside one process: the reopen must not land in Degraded on a lock the
    /// departed lanes are still letting go of.
    #[test]
    fn a_live_place_reconnects_into_a_new_generation_in_one_process() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("host");
        std::fs::create_dir_all(&directory).unwrap();
        let identity = Arc::new(RootIdentity::Unsealed(InMemoryProvider::from_seed(
            [0xc4; 32],
        )));

        let (worker, updates) =
            spawn_place_worker(Arc::new(|| {}), identity, PlaceWorkerSettings::default());
        let session = SessionId::new();
        worker.command(PlaceWorkerCommand::Found {
            session,
            generation: 1,
            directory: directory.clone(),
            name: "Hearth".into(),
        });
        let binding = match updates.recv_timeout(std::time::Duration::from_secs(60)) {
            Ok(Update::PlaceFounded {
                result: Ok((binding, snapshot)),
                ..
            }) => {
                assert!(snapshot.sync.is_some(), "founding binds lanes");
                binding
            },
            Ok(Update::PlaceFounded {
                result: Err(error), ..
            }) => panic!("founding refused: {error}"),
            _ => panic!("founding answered with an unrelated update"),
        };

        worker.command(PlaceWorkerCommand::Reconnect {
            session,
            generation: 2,
            directory,
            binding,
        });
        match updates.recv_timeout(std::time::Duration::from_secs(60)) {
            Ok(Update::PlaceOpened {
                result: Ok(snapshot),
                generation: 2,
                ..
            }) => {
                let sync = snapshot.sync.expect("the reconnected place binds lanes");
                assert_eq!(sync.lanes.len(), 9);
            },
            Ok(Update::PlaceOpened {
                result: Err(error), ..
            }) => panic!("same-process reconnect refused: {error}"),
            _ => panic!("reconnect answered with an unrelated update"),
        }

        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack_tx));
        ack_rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap();
    }

    #[test]
    fn stale_reconnect_cannot_disturb_a_newer_open_scope() {
        let root = tempfile::tempdir().unwrap();
        let stale_directory = root.path().join("stale");
        let current_directory = root.path().join("current");
        let identity = Arc::new(RootIdentity::Unsealed(InMemoryProvider::from_seed(
            [0x96; 32],
        )));
        let stale_binding = binding(0x4d);
        let current_binding = binding(0x4e);
        seed_profile(&stale_directory, identity.as_ref(), &stale_binding, 0);
        seed_profile(&current_directory, identity.as_ref(), &current_binding, 1);

        let (worker, updates) = spawn_place_worker(Arc::new(|| {}), identity, settings());
        let current_session = SessionId::new();
        worker.command(PlaceWorkerCommand::Open {
            session: current_session,
            generation: 2,
            directory: current_directory,
            binding: current_binding,
        });
        assert!(matches!(
            updates
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            Update::PlaceOpened {
                session,
                generation: 2,
                result: Ok(_),
            } if session == current_session
        ));

        let stale_session = SessionId::new();
        worker.command(PlaceWorkerCommand::Reconnect {
            session: stale_session,
            generation: 1,
            directory: stale_directory,
            binding: stale_binding,
        });
        assert!(matches!(
            updates
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            Update::PlaceOpened {
                session,
                generation: 1,
                result: Err(ref error),
            } if session == stale_session && error == "reconnect generation is stale"
        ));

        worker.command(PlaceWorkerCommand::Resync {
            session: current_session,
            generation: 2,
        });
        assert!(matches!(
            updates
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            Update::PlaceOpened {
                session,
                generation: 2,
                result: Ok(OfflinePlaceSnapshot {
                    graph: GraphCache { nodes: 1, .. },
                    ..
                }),
            } if session == current_session
        ));
    }

    #[test]
    fn reconnect_for_a_released_generation_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("profile");
        let binding = binding(0x4f);
        let identity = Arc::new(RootIdentity::Unsealed(InMemoryProvider::from_seed(
            [0x97; 32],
        )));
        seed_profile(&directory, identity.as_ref(), &binding, 0);

        let (worker, updates) = spawn_place_worker(Arc::new(|| {}), identity, settings());
        let session = SessionId::new();
        worker.command(PlaceWorkerCommand::Open {
            session,
            generation: 1,
            directory: directory.clone(),
            binding: binding.clone(),
        });
        assert!(matches!(
            updates
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            Update::PlaceOpened { result: Ok(_), .. }
        ));
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack_tx));
        ack_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();

        worker.command(PlaceWorkerCommand::Reconnect {
            session,
            generation: 1,
            directory,
            binding,
        });
        assert!(matches!(
            updates
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            Update::PlaceOpened {
                session: received_session,
                generation: 1,
                result: Err(ref error),
            } if received_session == session && error == "reconnect generation was already released"
        ));
    }

    #[test]
    fn worker_rejects_a_departed_generation_without_rescoping_the_open_place() {
        let root = tempfile::tempdir().unwrap();
        let first_directory = root.path().join("first");
        let second_directory = root.path().join("second");
        let identity = Arc::new(RootIdentity::Unsealed(InMemoryProvider::from_seed(
            [0x94; 32],
        )));
        let first_binding = binding(0x47);
        let second_binding = binding(0x48);
        seed_profile(&first_directory, identity.as_ref(), &first_binding, 0);
        seed_profile(&second_directory, identity.as_ref(), &second_binding, 0);

        let wake: Wake = Arc::new(|| {});
        let (worker, updates) = spawn_place_worker(wake, identity, settings());
        let first_session = SessionId::new();
        let second_session = SessionId::new();
        worker.command(PlaceWorkerCommand::Open {
            session: first_session,
            generation: 1,
            directory: first_directory,
            binding: first_binding.clone(),
        });
        assert!(matches!(
            updates
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            Update::PlaceOpened { result: Ok(_), .. }
        ));
        worker.command(PlaceWorkerCommand::Open {
            session: second_session,
            generation: 2,
            directory: second_directory,
            binding: second_binding,
        });
        assert!(matches!(
            updates
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            Update::PlaceOpened { result: Ok(_), .. }
        ));

        worker.command(PlaceWorkerCommand::SetCollection {
            session: first_session,
            generation: 1,
            selection: Some(PlaceCollectionVersion {
                moot: first_binding.moot,
                collection: PlaceCollectionId([0x49; 32]),
                frontier: Vec::new(),
                membership_commitment: [0x4a; 32],
            }),
        });
        assert!(matches!(
            updates
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            Update::PlaceCollectionSet {
                session,
                generation: 1,
                result: Err(ref error),
            } if session == first_session
                && error == "collection selection belongs to a departed place generation"
        ));

        worker.command(PlaceWorkerCommand::Resync {
            session: second_session,
            generation: 2,
        });
        assert!(matches!(
            updates
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            Update::PlaceOpened {
                session,
                generation: 2,
                result: Ok(OfflinePlaceSnapshot {
                    captured_selection: CapturedCollectionSelection::AllEffective,
                    ..
                }),
            } if session == second_session
        ));
    }

    #[test]
    fn worker_exact_collection_version_returns_only_its_ready_capture() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("profile");
        let binding = binding(0x44);
        let identity = Arc::new(RootIdentity::Unsealed(founder_for(&binding)));
        let (settings, requested, selected_share) =
            seed_exact_collection(&directory, identity.as_ref(), &binding);

        let wake: Wake = Arc::new(|| {});
        let (worker, updates) = spawn_place_worker(wake, identity, settings);
        let session = SessionId::new();
        worker.command(PlaceWorkerCommand::Open {
            session,
            generation: 1,
            directory: directory.clone(),
            binding: binding.clone(),
        });
        assert!(matches!(
            updates
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            Update::PlaceOpened { result: Ok(_), .. }
        ));
        worker.command(PlaceWorkerCommand::SetCollection {
            session,
            generation: 1,
            selection: Some(requested.clone()),
        });
        let Update::PlaceCollectionSet {
            result: Ok(snapshot),
            ..
        } = updates
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
        else {
            panic!("exact selection did not return a snapshot");
        };
        assert_eq!(snapshot.captured.groups.len(), 1);
        assert!(snapshot.captured.rejected.is_empty());
        assert_eq!(snapshot.captured.groups[0].contributions.len(), 1);
        assert_eq!(
            snapshot.captured.groups[0].contributions[0].share_operation,
            selected_share
        );
        assert_eq!(
            snapshot.captured_selection,
            CapturedCollectionSelection::Collection {
                requested: requested.clone(),
                status: CapturedCollectionSelectionStatus::Ready {
                    name: "Field notes".into(),
                    effective_contributions: 1,
                    pending_facts: 0,
                },
            }
        );
        assert_eq!(
            snapshot.collection_choices,
            vec![PlaceCollectionChoice {
                name: "Field notes".into(),
                version: requested,
            }]
        );
        // Same-handle refresh and a complete release/reopen retain the exact
        // chosen version before exposing the first snapshot.
        worker.command(PlaceWorkerCommand::Resync {
            session,
            generation: 1,
        });
        let Update::PlaceOpened {
            result: Ok(resynced),
            ..
        } = updates
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
        else {
            panic!("selection did not resync");
        };
        assert_eq!(resynced.captured_selection, snapshot.captured_selection);
        assert_eq!(resynced.captured, snapshot.captured);
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack_tx));
        ack_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        worker.command(PlaceWorkerCommand::Open {
            session,
            generation: 2,
            directory: directory.clone(),
            binding: binding.clone(),
        });
        let Update::PlaceOpened {
            result: Ok(reopened),
            ..
        } = updates
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
        else {
            panic!("saved selection did not reopen");
        };
        assert_eq!(reopened.captured_selection, snapshot.captured_selection);
        assert_eq!(reopened.captured, snapshot.captured);
        worker.command(PlaceWorkerCommand::SetCollection {
            session,
            generation: 2,
            selection: None,
        });
        assert!(matches!(
            updates
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            Update::PlaceCollectionSet { result: Ok(_), .. }
        ));
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack_tx));
        ack_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        worker.command(PlaceWorkerCommand::Open {
            session,
            generation: 3,
            directory,
            binding,
        });
        let Update::PlaceOpened {
            result: Ok(cleared),
            ..
        } = updates
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
        else {
            panic!("cleared selection did not reopen");
        };
        assert_eq!(
            cleared.captured_selection,
            CapturedCollectionSelection::AllEffective
        );
        assert_eq!(cleared.captured.groups.len(), 2);
    }

    #[test]
    fn worker_collection_save_failure_keeps_the_previous_scope() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("profile");
        let binding = binding(0x4b);
        let identity = Arc::new(RootIdentity::Unsealed(founder_for(&binding)));
        let (settings, requested, _) =
            seed_exact_collection(&directory, identity.as_ref(), &binding);
        let (worker, updates) = spawn_place_worker(Arc::new(|| {}), identity, settings);
        let session = SessionId::new();
        worker.command(PlaceWorkerCommand::Open {
            session,
            generation: 1,
            directory: directory.clone(),
            binding,
        });
        assert!(matches!(
            updates
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            Update::PlaceOpened { result: Ok(_), .. }
        ));
        // Replacing a directory with a file must fail on every supported host.
        std::fs::create_dir(directory.join(crate::session::PLACE_COLLECTION_FILE)).unwrap();
        worker.command(PlaceWorkerCommand::SetCollection {
            session,
            generation: 1,
            selection: Some(requested),
        });
        assert!(matches!(
            updates
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            Update::PlaceCollectionSet { result: Err(_), .. }
        ));
        worker.command(PlaceWorkerCommand::Resync {
            session,
            generation: 1,
        });
        let Update::PlaceOpened {
            result: Ok(snapshot),
            ..
        } = updates
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
        else {
            panic!("prior scope did not survive a failed save");
        };
        assert_eq!(
            snapshot.captured_selection,
            CapturedCollectionSelection::AllEffective
        );
        assert_eq!(snapshot.captured.groups.len(), 2);
    }

    #[test]
    fn worker_stale_collection_version_withholds_available_capture() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("profile");
        let binding = binding(0x45);
        let founder = founder_for(&binding);
        let identity = Arc::new(RootIdentity::Unsealed(founder_for(&binding)));
        let (settings, requested, selected_share) =
            seed_exact_collection(&directory, identity.as_ref(), &binding);

        crate::session::save_place_collection(&directory, Some(&requested)).unwrap();

        let moot = pollster::block_on(MootFile::open_existing(
            place_store_dir(&directory).join("gemot"),
            MootId(binding.moot.0),
            settings.retention.clone(),
        ))
        .unwrap();
        let current = pollster::block_on(
            moot.authorized_collection(CollectionId(requested.collection.0), AUTHORITY_AT_MS),
        )
        .unwrap()
        .unwrap();
        pollster::block_on(moot.set_collection_membership_for_identity(
            &founder,
            current.collection,
            current.heads,
            CollectionChange::SetMembership {
                contribution: ContributionRef {
                    moot_id: binding.moot.0,
                    share: selected_share,
                },
                included: false,
            },
            30,
        ))
        .unwrap();
        drop(moot);

        let wake: Wake = Arc::new(|| {});
        let (worker, updates) = spawn_place_worker(wake, identity, settings);
        let session = SessionId::new();
        worker.command(PlaceWorkerCommand::Open {
            session,
            generation: 1,
            directory,
            binding,
        });
        let Update::PlaceOpened {
            result: Ok(all_effective),
            ..
        } = updates
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
        else {
            panic!("place did not open");
        };
        assert!(all_effective.captured.groups.is_empty());
        assert!(all_effective.captured.rejected.is_empty());
        assert!(matches!(&all_effective.captured_selection,
            CapturedCollectionSelection::Collection {
                requested: retained, status: CapturedCollectionSelectionStatus::Stale { .. },
            } if retained == &requested));
        assert_eq!(all_effective.collection_choices.len(), 1);
        assert_ne!(all_effective.collection_choices[0].version, requested);

        worker.command(PlaceWorkerCommand::SetCollection {
            session,
            generation: 1,
            selection: Some(requested.clone()),
        });
        let Update::PlaceCollectionSet {
            result: Ok(snapshot),
            ..
        } = updates
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
        else {
            panic!("stale selection did not return a snapshot");
        };
        assert!(snapshot.captured.groups.is_empty());
        assert!(snapshot.captured.rejected.is_empty());
        let CapturedCollectionSelection::Collection {
            requested: retained,
            status: CapturedCollectionSelectionStatus::Stale { current },
        } = snapshot.captured_selection
        else {
            panic!("advanced collection history did not report stale");
        };
        assert_eq!(retained, requested);
        assert_ne!(current, requested);
    }

    #[test]
    fn worker_authority_shrink_keeps_the_exact_version_and_withholds_its_share() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("profile");
        let binding = binding(0x46);
        let identity = Arc::new(RootIdentity::Unsealed(founder_for(&binding)));
        let (settings, requested, selected_share) =
            seed_exact_collection(&directory, identity.as_ref(), &binding);

        let wake: Wake = Arc::new(|| {});
        let (worker, updates) = spawn_place_worker(wake, identity, settings.clone());
        let session = SessionId::new();
        worker.command(PlaceWorkerCommand::Open {
            session,
            generation: 1,
            directory: directory.clone(),
            binding: binding.clone(),
        });
        let Update::PlaceOpened {
            result: Ok(all_effective),
            ..
        } = updates
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
        else {
            panic!("place did not open before the withdrawal");
        };
        assert_eq!(
            all_effective.captured.groups.len(),
            2,
            "both effective captures are initially visible"
        );
        worker.command(PlaceWorkerCommand::SetCollection {
            session,
            generation: 1,
            selection: Some(requested.clone()),
        });
        assert!(matches!(
            updates
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            Update::PlaceCollectionSet {
                result: Ok(OfflinePlaceSnapshot {
                    captured_selection: CapturedCollectionSelection::Collection {
                        status: CapturedCollectionSelectionStatus::Ready {
                            effective_contributions: 1,
                            ..
                        },
                        ..
                    },
                    ..
                }),
                ..
            }
        ));

        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack_tx));
        ack_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let founder = founder_for(&binding);
        let moot = pollster::block_on(MootFile::open_existing(
            place_store_dir(&directory).join("gemot"),
            MootId(binding.moot.0),
            settings.retention.clone(),
        ))
        .unwrap();
        pollster::block_on(moot.withdraw_share_for_identity(&founder, selected_share, 30)).unwrap();
        drop(moot);

        worker.command(PlaceWorkerCommand::Open {
            session,
            generation: 2,
            directory,
            binding,
        });
        let Update::PlaceOpened {
            result: Ok(all_effective),
            ..
        } = updates
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
        else {
            panic!("place did not reopen after the withdrawal");
        };
        assert_eq!(
            all_effective.captured.groups.len(),
            0,
            "the restored selection must not expose the other capture"
        );
        assert!(matches!(&all_effective.captured_selection,
            CapturedCollectionSelection::Collection { requested: retained, .. }
                if retained == &requested));
        worker.command(PlaceWorkerCommand::SetCollection {
            session,
            generation: 2,
            selection: Some(requested.clone()),
        });
        let Update::PlaceCollectionSet {
            result: Ok(snapshot),
            ..
        } = updates
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
        else {
            panic!("post-withdrawal selection did not return a snapshot");
        };
        assert!(snapshot.captured.groups.is_empty());
        assert!(snapshot.captured.rejected.is_empty());
        assert_eq!(
            snapshot.captured_selection,
            CapturedCollectionSelection::Collection {
                requested,
                status: CapturedCollectionSelectionStatus::Ready {
                    name: "Field notes".into(),
                    effective_contributions: 0,
                    pending_facts: 1,
                },
            },
            "fauna authority changes effectiveness without retargeting collection history"
        );
    }

    #[test]
    fn worker_releases_files_before_reopen_and_advances_generation() {
        let root =
            std::env::temp_dir().join(format!("turnstone-place-release-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let original = root.join("original");
        let moved = root.join("moved");
        let identity = Arc::new(RootIdentity::Unsealed(InMemoryProvider::from_seed(
            [0x91; 32],
        )));
        let binding = binding(0x41);
        seed_profile(&original, identity.as_ref(), &binding, 1);

        let wake: Wake = Arc::new(|| {});
        let (worker, updates) = spawn_place_worker(wake, identity, settings());
        worker.command(PlaceWorkerCommand::Open {
            session: SessionId::new(),
            generation: 1,
            directory: original.clone(),
            binding: binding.clone(),
        });
        let first = updates
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(matches!(
            first,
            Update::PlaceOpened {
                generation: 1,
                result: Ok(_),
                ..
            }
        ));

        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack_tx));
        ack_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        std::fs::rename(&original, &moved)
            .expect("release acknowledgement means the session directory can move");

        worker.command(PlaceWorkerCommand::Open {
            session: SessionId::new(),
            generation: 2,
            directory: moved,
            binding,
        });
        let second = updates
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(matches!(
            second,
            Update::PlaceOpened {
                generation: 2,
                result: Ok(_),
                ..
            }
        ));
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        worker.command(PlaceWorkerCommand::Release(ack_tx));
        ack_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }
}
