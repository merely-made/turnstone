// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use chartulary::{Author, Container};
use commons::chat::{Channel, ChatEvent, Message};
use fleece::FleeceAnnotationRecord;
use fleece::eidetic_bridge::{CaptureIdentity, FLEECE_ANNOTATION_SCHEMA_ID};
use fleece::{TextPositionSelector, anchor_for_range, extract_document};
use gemot::moot::constitution::{CapabilityGrant, ConstitutionRules};
use gemot::moot::{
    CollectionChange, ContributionRef, MOOT_ACT_ACTION, MOOT_DELEGATION_DOMAIN, MootAccessLevel,
    MootMember, MootMembershipAction,
};
use genet_static_dom::StaticDocument;
use mien::Policy;
use muniment::FjallStore;
use stickleback::DropExportProfile;

use crate::place::invite::{ArtifactRefV1, PLACE_INVITE_VERSION};
use identity::InMemoryProvider;
use servitor::{Cap, cap_path};

use insigne::delegation::{
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

/// How long a test waits for an answer the worker must give. Generous,
/// since a full parallel suite can stall a worker for seconds; a
/// passing run never waits it out.
const ANSWER: std::time::Duration = std::time::Duration::from_secs(60);

#[path = "tests/a_place_with_only_a_plaintext_graph_is_cases.rs"]
mod a_place_with_only_a_plaintext_graph_is_cases;
#[path = "tests/a_revoked_member_reaches_no_projected_place_state_cases.rs"]
mod a_revoked_member_reaches_no_projected_place_state_cases;
#[path = "tests/graph_only_admission_and_revocation_need_no_projection_cases.rs"]
mod graph_only_admission_and_revocation_need_no_projection_cases;
#[path = "tests/worker_rejects_a_departed_generation_without_rescoping_the_cases.rs"]
mod worker_rejects_a_departed_generation_without_rescoping_the_cases;
#[path = "tests/worker_releases_files_before_reopen_and_advances_generation_cases.rs"]
mod worker_releases_files_before_reopen_and_advances_generation_cases;

#[path = "tests/settings.rs"]
mod settings;
pub(crate) use settings::settings;
#[path = "tests/open_file_room.rs"]
mod open_file_room;
use open_file_room::open_file_room;
#[path = "tests/founder_for.rs"]
mod founder_for;
use founder_for::founder_for;
#[path = "tests/place_rules.rs"]
mod place_rules;
pub(crate) use place_rules::place_rules;
#[path = "tests/founder_signing_key.rs"]
mod founder_signing_key;
pub(crate) use founder_signing_key::founder_signing_key;
#[path = "tests/place_delegation.rs"]
mod place_delegation;
pub(crate) use place_delegation::place_delegation;
#[path = "tests/revoke_place_delegation.rs"]
mod revoke_place_delegation;
use revoke_place_delegation::revoke_place_delegation;
#[path = "tests/binding.rs"]
mod binding;
pub(crate) use binding::binding;
#[path = "tests/seed_profile.rs"]
mod seed_profile;
use seed_profile::seed_profile;
#[path = "tests/seed_exact_collection_captures.rs"]
mod seed_exact_collection_captures;
pub(crate) use seed_exact_collection_captures::seed_exact_collection_captures;
#[path = "tests/seed_exact_collection.rs"]
mod seed_exact_collection;
pub(crate) use seed_exact_collection::seed_exact_collection;
#[path = "tests/real_invitation.rs"]
mod real_invitation;
use real_invitation::real_invitation;
#[path = "tests/inline_artifact.rs"]
mod inline_artifact;
use inline_artifact::inline_artifact;
#[path = "tests/matched_case.rs"]
mod matched_case;
use matched_case::matched_case;
#[path = "tests/residue.rs"]
mod residue;
use residue::residue;
#[path = "tests/found_place_for_authoring.rs"]
mod found_place_for_authoring;
pub(crate) use found_place_for_authoring::found_place_for_authoring;
