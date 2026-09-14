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
pub mod invite;
pub(crate) mod lanes;
pub mod projection;
pub(crate) mod rendezvous;
pub mod worker;

/// The only binding version this Turnstone build understands.
pub const PLACE_BINDING_VERSION: u16 = 1;

fn encode_hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(kind: &'static str, value: &str) -> Result<[u8; 32], PlaceIdError> {
    if value.len() != 64 {
        return Err(PlaceIdError {
            kind,
            reason: "expected 64 lowercase hexadecimal characters",
        });
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(PlaceIdError {
            kind,
            reason: "identifier contains a non-lowercase-hexadecimal character",
        });
    }
    let mut bytes = [0; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(pair).map_err(|_| PlaceIdError {
            kind,
            reason: "identifier is not UTF-8 hexadecimal",
        })?;
        bytes[index] =
            u8::from_str_radix(text, 16).expect("validated hexadecimal pair always parses");
    }
    Ok(bytes)
}

/// A malformed serialized place identifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceIdError {
    kind: &'static str,
    reason: &'static str,
}

impl std::fmt::Display for PlaceIdError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid {}: {}", self.kind, self.reason)
    }
}

impl std::error::Error for PlaceIdError {}

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

/// App-owned form of one exact Gemot collection version.
///
/// The worker converts this at the Gemot boundary. Keeping it here prevents
/// durable app state and surfaces from acquiring domain-crate types.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceCollectionVersion {
    pub moot: PlaceId,
    pub collection: PlaceCollectionId,
    pub frontier: Vec<[u8; 32]>,
    pub membership_commitment: [u8; 32],
}

/// A currently authorized collection offered by the local search picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceCollectionChoice {
    pub name: String,
    pub version: PlaceCollectionVersion,
}

/// The public, durable binding between one Turnstone session and one governed
/// shared place.
///
/// These ids remain distinct because the live domain APIs address different
/// replicated spaces with them. Knot documents are addressed nodes inside the
/// shared graph and therefore need no fourth root here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceBindingV1 {
    pub version: u16,
    pub moot: PlaceId,
    pub root: SharedContainerId,
    pub chat: ChatSpaceId,
    pub default_channel: String,
}

impl PlaceBindingV1 {
    pub fn new(
        moot: PlaceId,
        root: SharedContainerId,
        chat: ChatSpaceId,
        default_channel: impl Into<String>,
    ) -> Result<Self, PlaceBindingError> {
        let binding = Self {
            version: PLACE_BINDING_VERSION,
            moot,
            root,
            chat,
            default_channel: default_channel.into(),
        };
        binding.validate()?;
        Ok(binding)
    }

    /// Validate product-level bounds after deserialization. Domain membership
    /// and authority are intentionally not guessed here.
    pub fn validate(&self) -> Result<(), PlaceBindingError> {
        if self.version != PLACE_BINDING_VERSION {
            return Err(PlaceBindingError::UnsupportedVersion(self.version));
        }
        if self.default_channel.trim().is_empty() {
            return Err(PlaceBindingError::EmptyDefaultChannel);
        }
        if self.default_channel.len() > 128 {
            return Err(PlaceBindingError::DefaultChannelTooLong(
                self.default_channel.len(),
            ));
        }
        if self.default_channel.chars().any(char::is_control) {
            return Err(PlaceBindingError::DefaultChannelHasControl);
        }
        Ok(())
    }
}

/// A malformed or unsupported public place binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceBindingError {
    UnsupportedVersion(u16),
    EmptyDefaultChannel,
    DefaultChannelTooLong(usize),
    DefaultChannelHasControl,
}

impl std::fmt::Display for PlaceBindingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported place binding version {version}")
            },
            Self::EmptyDefaultChannel => write!(formatter, "default channel is empty"),
            Self::DefaultChannelTooLong(length) => {
                write!(
                    formatter,
                    "default channel is {length} bytes; maximum is 128"
                )
            },
            Self::DefaultChannelHasControl => {
                write!(formatter, "default channel contains a control character")
            },
        }
    }
}

impl std::error::Error for PlaceBindingError {}

/// The only card and pre-key-offer version this build understands.
pub const PLACE_CARD_VERSION: u16 = 1;

/// A place's public calling card: where it is and who founded it.
///
/// Carries no authority whatsoever. It exists so someone who wants in can
/// name the Moot their pre-key belongs to; the invitation stays the only
/// envelope admission reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceCardV1 {
    pub version: u16,
    pub binding: PlaceBindingV1,
    /// The founder's Personae root, hex.
    pub founder_root: String,
    /// The founder's dialable contact hints at export time.
    pub rendezvous: Vec<invite::RendezvousV1>,
}

impl PlaceCardV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != PLACE_CARD_VERSION {
            return Err(format!("unsupported place card version {}", self.version));
        }
        self.binding.validate().map_err(|error| error.to_string())?;
        decode_hex("founder root", &self.founder_root).map_err(|error| error.to_string())?;
        Ok(())
    }
}

/// One profile's published group pre-key for one Moot.
///
/// Also authority-free: the bundle carries its own Personae attestation, and
/// the inviter re-derives the root from it rather than trusting `root` here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacePrekeyOfferV1 {
    pub version: u16,
    pub moot: PlaceId,
    /// The offering profile's Personae root, hex.
    pub root: String,
    /// The published pre-key bundle, base64.
    pub prekey: String,
}

impl PlacePrekeyOfferV1 {
    /// Bounded, decoded bundle bytes, or the reason they cannot be read.
    pub fn prekey_bytes(&self) -> Result<Vec<u8>, String> {
        use base64::Engine as _;
        if self.version != PLACE_CARD_VERSION {
            return Err(format!(
                "unsupported pre-key offer version {}",
                self.version
            ));
        }
        decode_hex("offering root", &self.root).map_err(|error| error.to_string())?;
        base64::engine::general_purpose::STANDARD
            .decode(&self.prekey)
            .map_err(|error| format!("decode pre-key bundle: {error}"))
    }
}

/// Hex for a 32-byte public identifier, the one spelling the card uses.
pub fn hex32(bytes: &[u8; 32]) -> String {
    encode_hex(bytes)
}

/// How much of an elided value's head and tail survives. A rendezvous ticket
/// is ~200 characters and a chrome row is one clipped line, so the status
/// surface shows the ends and the palette's copy action carries the whole.
const ELIDE_HEAD: usize = 12;
const ELIDE_TAIL: usize = 8;

/// `value` with its middle replaced by an ellipsis, or `value` unchanged when
/// eliding would not shorten it. Char-based, so a multi-byte value is never
/// cut mid-character.
pub fn elide_middle(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= ELIDE_HEAD + ELIDE_TAIL + 1 {
        return value.to_string();
    }
    let head: String = chars[..ELIDE_HEAD].iter().collect();
    let tail: String = chars[chars.len() - ELIDE_TAIL..].iter().collect();
    format!("{head}…{tail}")
}

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
    /// This bind's own dialable ticket(s). What a peer needs to reach here;
    /// holding one says nothing about whether anyone did.
    pub local_rendezvous: Vec<String>,
    /// How many peer tickets this bind dialed. Zero is listen-only.
    pub dialed_rendezvous: usize,
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

impl PlaceLaneSnapshot {
    fn label(&self) -> &str {
        match self.name {
            "gemot/constitution/v1" => "Constitution",
            "gemot/delegation/v1" => "Delegation",
            "gemot/membership/v1" => "Membership",
            "gemot/records/v1" => "Records",
            "gemot/standing/v1" => "Standing",
            "gemot/tulpa/v1" => "Tulpa",
            "gemot/flora/v1" => "Flora",
            "commons/graph/v1" => "Shared graph",
            "commons/chat/v1" => "Chat",
            other => other,
        }
    }
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

impl CapturedCollectionCache {
    /// Search the current derived view. The index is transient and one result
    /// represents one exact canonical-text group.
    pub fn search(&self, query: &str, limit: usize) -> Vec<CapturedSearchHit> {
        use eidetic_search::{DocumentIndex, DocumentIndexConfig, SearchDocument};

        let index = DocumentIndex::from_documents(
            DocumentIndexConfig::default(),
            self.groups.iter().enumerate().map(|(position, group)| {
                let latest = group
                    .contributions
                    .last()
                    .expect("a captured content group always has a contribution");
                let mut aliases: Vec<_> = group
                    .contributions
                    .iter()
                    .map(|contribution| contribution.source.clone())
                    .filter(|source| source != &latest.source)
                    .collect();
                aliases.sort();
                aliases.dedup();
                SearchDocument {
                    key: position,
                    primary_address: latest.source.clone(),
                    aliases,
                    title: Some(latest.title.clone()),
                    body: Some(group.canonical_text.clone()),
                }
            }),
        );
        index
            .search(query, limit)
            .into_iter()
            .filter_map(|hit| {
                self.groups.get(hit.key).map(|group| CapturedSearchHit {
                    canonical_text_hash: group.canonical_text_hash.clone(),
                    title: group
                        .contributions
                        .last()
                        .map(|contribution| contribution.title.clone())
                        .unwrap_or_default(),
                    source: hit.primary_address,
                    contribution_count: group.contributions.len(),
                })
            })
            .collect()
    }
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
}

impl Default for PlaceState {
    fn default() -> Self {
        Self::Personal
    }
}

impl PlaceState {
    /// Shared by the status surface and observation. Facts are from the last
    /// worker snapshot; opening this view requests a fresh local snapshot.
    pub fn status_lines(&self) -> Vec<String> {
        let mut rows = match self {
            Self::Personal => vec!["Personal session: no shared place joined".into()],
            Self::Joining { .. } => vec!["Place: checking invitation".into()],
            Self::Opening { .. } => vec!["Place: opening retained state or reconnecting".into()],
            Self::Degraded { error, .. } => vec![format!("Place unavailable: {error}")],
            Self::Failed { error } => vec![format!("Place not joined: {error}")],
            Self::Offline { snapshot, .. } => {
                let mut rows = vec![match &snapshot.sync {
                    None => "Last refresh: retained data only; no local sync lanes opened".into(),
                    Some(_) => "Last refresh: local sync lanes opened; peer reachability unknown".into(),
                }];
                rows.push(format!("Retained history: {} shared nodes, {} messages",
                    snapshot.graph.nodes, snapshot.chat.messages));
                match &snapshot.permissions {
                    Some(permissions) => {
                        let permission = |effective| if effective { "effective" } else { "not effective" };
                        rows.push(format!("Message permission: {} locally at last refresh", permission(permissions.message_write)));
                        rows.push(format!("Shared graph permission: {} locally at last refresh", permission(permissions.graph_write)));
                    },
                    None => rows.push("Writing permissions: not evaluated".into()),
                }
                rows.push("Writing: permissions are checked again for each action".into());
                rows.push("Delivery: these sync observations do not confirm message delivery".into());
                if let Some(sync) = &snapshot.sync {
                    // Listen-only is a real state and must never read as
                    // connected: a founder alone in a place has lanes open and
                    // nobody on them.
                    if sync.dialed_rendezvous == 0 {
                        rows.push("Listening for peers: none dialed".into());
                    }
                    rows.extend(
                        sync.local_rendezvous
                            .iter()
                            // The full ticket stays in the snapshot, the
                            // observation, and the exported card; only this
                            // one-line row is elided.
                            .map(|ticket| {
                                format!("Local rendezvous: {}", elide_middle(ticket))
                            }),
                    );
                    // "at last refresh" is dropped here: the header row above
                    // already states every fact on this card is from the last
                    // refresh, so repeating it per lane only pushed the row
                    // past the card's text budget. (Mark, 2026-09-13)
                    rows.extend(sync.lanes.iter().map(|lane| format!(
                        "{}: {}; {} rounds; {} accepted ops",
                        lane.label(), if lane.syncing { "syncing" } else { "idle" },
                        lane.sync_rounds, lane.ops_received)));
                }
                rows
            },
        };
        if self.binding().is_some() {
            rows.push("Reconnect: saved contacts must still be valid".into());
            rows.push("Leave: detach this session and retain its history".into());
        }
        rows
    }

    /// This bind's own dialable tickets at the last refresh, whole. Empty
    /// unless a place is open and its lanes reported one.
    pub fn local_rendezvous(&self) -> &[String] {
        match self {
            Self::Offline { snapshot, .. } => snapshot
                .sync
                .as_ref()
                .map(|sync| sync.local_rendezvous.as_slice())
                .unwrap_or_default(),
            _ => &[],
        }
    }

    pub fn binding(&self) -> Option<&PlaceBindingV1> {
        match self {
            Self::Opening { binding, .. }
            | Self::Offline { binding, .. }
            | Self::Degraded { binding, .. } => Some(binding),
            Self::Personal | Self::Joining { .. } | Self::Failed { .. } => None,
        }
    }

    pub fn generation(&self) -> Option<u64> {
        match self {
            Self::Joining { generation }
            | Self::Opening { generation, .. }
            | Self::Offline { generation, .. }
            | Self::Degraded { generation, .. } => Some(*generation),
            Self::Personal | Self::Failed { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> PlaceBindingV1 {
        PlaceBindingV1::new(
            PlaceId([1; 32]),
            SharedContainerId([2; 32]),
            ChatSpaceId([3; 32]),
            "hall",
        )
        .unwrap()
    }

    /// A realistic p2panda endpoint ticket: ~200 base32 characters.
    fn long_ticket() -> String {
        "abcdefghijklmnopqrstuvwxyz234567"
            .chars()
            .cycle()
            .take(200)
            .collect()
    }

    #[test]
    fn elide_middle_keeps_the_ends_and_leaves_short_values_alone() {
        assert_eq!(elide_middle(""), "");
        assert_eq!(elide_middle("short"), "short");
        // Exactly the length eliding would produce: nothing is gained, so the
        // value is left whole.
        let same = "a".repeat(ELIDE_HEAD + ELIDE_TAIL + 1);
        assert_eq!(elide_middle(&same), same);

        let ticket = long_ticket();
        let elided = elide_middle(&ticket);
        assert_eq!(elided.chars().count(), ELIDE_HEAD + ELIDE_TAIL + 1);
        assert!(elided.starts_with(&ticket[..ELIDE_HEAD]));
        assert!(elided.ends_with(&ticket[ticket.len() - ELIDE_TAIL..]));
        assert!(elided.contains('…'));

        // Multi-byte input is cut on character boundaries, not bytes.
        let wide = "é".repeat(40);
        assert_eq!(
            elide_middle(&wide).chars().count(),
            ELIDE_HEAD + ELIDE_TAIL + 1
        );
    }

    /// Every status row is one `nowrap; overflow: hidden` line in a 560px
    /// card, so a row that does not fit is cut off mid-string rather than
    /// wrapped. The ~200-character rendezvous ticket is what broke this, and
    /// six-digit lane counters against the longest lane labels are the other
    /// edge: both are exercised here so the guard covers every row, lane rows
    /// included.
    #[test]
    fn every_status_row_fits_one_palette_row() {
        let state = PlaceState::Offline {
            binding: binding(),
            generation: 1,
            snapshot: OfflinePlaceSnapshot {
                sync: Some(PlaceSyncSnapshot {
                    lanes: vec![
                        PlaceLaneSnapshot {
                            name: "commons/graph/v1",
                            syncing: true,
                            sync_rounds: 123456,
                            ops_received: 987654,
                            last_activity_ms: Some(42),
                        },
                        PlaceLaneSnapshot {
                            name: "gemot/constitution/v1",
                            syncing: false,
                            sync_rounds: 123456,
                            ops_received: 987654,
                            last_activity_ms: None,
                        },
                    ],
                    local_rendezvous: vec![long_ticket()],
                    dialed_rendezvous: 0,
                }),
                permissions: Some(PlacePermissionSnapshot {
                    message_write: true,
                    graph_write: false,
                }),
                ..Default::default()
            },
        };

        let rows = state.status_lines();
        assert!(
            rows.iter().any(|row| row.starts_with("Local rendezvous:")),
            "the rendezvous row is the one under test",
        );
        for row in rows.iter() {
            let width = crate::ui::chrome_row_width(row);
            assert!(
                width <= crate::ui::ROW_TEXT_BUDGET,
                "status row must fit one palette row: {width}px > {}px: {row}",
                crate::ui::ROW_TEXT_BUDGET,
            );
        }
    }

    #[test]
    fn binding_json_uses_distinct_readable_hex_ids() {
        let json = serde_json::to_string_pretty(&binding()).unwrap();
        assert!(json.contains(&"01".repeat(32)));
        assert!(json.contains(&"02".repeat(32)));
        assert!(json.contains(&"03".repeat(32)));
        assert!(!json.contains("[\n    1,"));
        let restored: PlaceBindingV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, binding());
    }

    #[test]
    fn binding_validation_rejects_version_and_channel_ambiguity() {
        let mut candidate = binding();
        candidate.version = 2;
        assert_eq!(
            candidate.validate(),
            Err(PlaceBindingError::UnsupportedVersion(2))
        );

        candidate.version = PLACE_BINDING_VERSION;
        candidate.default_channel = " \t".into();
        assert_eq!(
            candidate.validate(),
            Err(PlaceBindingError::EmptyDefaultChannel)
        );

        candidate.default_channel = "hall\nannouncements".into();
        assert_eq!(
            candidate.validate(),
            Err(PlaceBindingError::DefaultChannelHasControl)
        );
    }

    #[test]
    fn serialized_id_requires_exact_hex() {
        let error = serde_json::from_str::<PlaceId>("\"abcd\"").unwrap_err();
        assert!(error.to_string().contains("expected 64"));
        let error =
            serde_json::from_str::<PlaceId>(&format!("\"{}g\"", "0".repeat(63))).unwrap_err();
        assert!(error.to_string().contains("non-lowercase-hexadecimal"));
        let error =
            serde_json::from_str::<PlaceId>(&format!("\"{}A\"", "0".repeat(63))).unwrap_err();
        assert!(error.to_string().contains("non-lowercase-hexadecimal"));
    }
}
