// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Participant residency.
//!
//! Terminology (2026-09-20): denizen now names an Isometry simulation inhabitant.
//! This module retains its existing API names, saved directories, facet IDs,
//! participant URLs and certificate scope bytes for compatibility. Display
//! text uses participant; this reassignment does not migrate stored sessions.
//!
//! Participant gate B1, the turnstone half: install a local
//! scenario pack as a resident helper, review its grant visibly, run it from
//! the palette, and read its edits back attributed.
//!
//! The substrate is already built and this module only wires it: the node IS
//! the participant (the `denizen.binding` facet carries subject + kind — agency;
//! the world it bears hangs on `Node.nested` — structure, the kernel's
//! `GraphBearing` impl), its inner world is a chartulary `GraphLog` the `servitor::Gate`
//! commits into (grant projections read-only, petitions attributed and
//! revision-checked), and its runnable body is a piccolo control script whose
//! emitted Actions lower through the ordinary spine — under the participant's
//! author in mere's attributed `GraphJournal`.
//!
//! Identity: the subject is **content-derived** — `blake3(source)` is the
//! 32-byte keyholder — so the same script is the same participant everywhere, and
//! a modified script is a different subject facing a fresh grant review.
//! (Signed personae subjects arrive with packs at B4; the gate does not care
//! which mints the bytes.)

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chartulary::{Container, GraphLog, Relation};
use identity::IdentityProvider;
use insigne::delegation::SignedDelegationCertificate;
use muniment::{Journal, LogId};
use servitor::delegation::{DelegationTable, root_certificate};
use servitor::resident::{BodyRevision, Lifecycle, ResidentBinding};
use servitor::{Cap, Gate, Grant, Mode, Subject};
use uuid::Uuid;

use crate::app::App;

/// The facet carrying a scenario participant's runnable source (turnstone's own
/// namespace beside `denizen.binding`; the binding stays app-agnostic).
pub const SCENARIO_SOURCE_FACET: &str = "scenario.source";

/// The facet naming a component participant's `.wasm` file, relative to the
/// session's `denizens/` dir. The bytes live on disk (never in a facet); the
/// facet is the pointer, exactly like the world's log id is a pointer.
pub const COMPONENT_FACET: &str = "component.file";

/// The capability path a rung-1 scenario participant is granted over its own
/// nested world (`Mode::Write`). The visible review names it.
pub const SCENARIO_SCOPE: &str = "scenario/";

/// The piccolo step budget a participant run gets — generous for a control
/// script, hard against a runaway loop.
pub const RUN_BUDGET: u64 = 20_000;

/// What a pack actually IS: a control script's source, or a wasm component's
/// bytes. Both are content-addressed the same way (blake3 over the bytes), so
/// identity does not care which lane runs it.
#[derive(Clone, Debug, PartialEq)]
pub enum PackBody {
    /// A piccolo control script (`.lua`).
    Scenario(String),
    /// An `app-core` wasm component (`.wasm`).
    Component(Vec<u8>),
}

/// A staged install awaiting the VISIBLE grant review: nothing is minted, no
/// grant exists, until the user confirms from the palette.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingInstall {
    /// Where the pack came from (display + provenance).
    pub path: PathBuf,
    /// The participant's display label (the file stem).
    pub label: String,
    /// The runnable body.
    pub body: PackBody,
    /// The content-derived subject.
    pub subject: Subject,
    /// The action RINGS this install would grant — the default profile,
    /// PRESELECTED for the review, never silently granted: the confirm row
    /// names them, and only confirming turns the ask into a grant.
    pub rings: Vec<crate::ring::Ring>,
    /// The address this pack asks to wake on, from a `-- @watch <url>` line
    /// in its source, or `None` for a pack that only runs when asked.
    ///
    /// Declared as an address rather than a node id because a pack author
    /// cannot know a UUID; install resolves it. And declared *in the source*
    /// because the subject is `blake3(source)`, so changing what a pack wakes
    /// on changes its identity and forces a fresh review. The widening rule
    /// falls out of the identity rule rather than needing its own machinery.
    pub watch_url: Option<String>,
    /// Actuation stability declared by `-- @deadband <change> <interval-ms>`.
    /// The script must pair it with `mere.output(value)` on every run.
    pub deadband: Option<servitor::Deadband>,
}

/// One resident participant's live half: its subject and its nested world,
/// rebuilt from the binding facet + the persisted log on adopt.
pub struct Resident {
    pub subject: Subject,
    pub label: String,
    pub nested: GraphLog<Container, Relation>,
    /// Host-persisted instance/body/lifecycle snapshot. The graph binding is
    /// agency truth; this is the run-admission truth keyed by that node.
    pub binding: ResidentBinding,
}

/// The session's participant runtime: residents by member node, the authority
/// provider the gate consults, and the gate itself. Rebuilt on adopt; the
/// facts it derives from (binding facets + nested logs) are the durable truth.
#[derive(Default)]
pub struct Denizens {
    pub residents: HashMap<Uuid, Resident>,
    /// Verified delegation chains: what each participant may do, descending from
    /// the profile's root identity. Certificates are the AUTHORITY; the grant
    /// projections in each world are the browsable audit record of the same
    /// facts (capability-model C4).
    pub authority: DelegationTable,
    pub gate: Gate,
    /// Residents whose world id came from a LEGACY binding facet
    /// (`nested_log` written before the containment ruling) rather than
    /// `Node.nested`. The adopt path heals each: set the node's `nested`,
    /// rewrite the binding without the field.
    pub legacy_heals: Vec<(Uuid, String)>,
    /// Versioned lifecycle sidecar. Kept beside the runtime so every run and
    /// every transition consult the same durable state.
    pub admissions: crate::resident_admission::ResidentAdmissions,
    /// A corrupt or unreadable lifecycle sidecar blocks install as well as
    /// runs. Replacing it with an empty table would erase tombstones.
    pub admission_error: Option<String>,
}

/// Why a gesture that speaks as the profile refuses while the identity is
/// pending (djinn absent or Locked, dramatis D12).
pub const PENDING_IDENTITY: &str = crate::identity::PENDING;

#[cfg(test)]
#[path = "denizen/tests.rs"]
mod tests;

#[path = "denizen/read_cap.rs"]
mod read_cap;
pub use read_cap::read_cap;
#[path = "denizen/world_cap.rs"]
mod world_cap;
pub use world_cap::world_cap;
#[path = "denizen/pack_body_kind.rs"]
mod pack_body_kind;
#[path = "denizen/parse_watch.rs"]
mod parse_watch;
pub use parse_watch::parse_watch;
#[path = "denizen/parse_deadband.rs"]
mod parse_deadband;
pub use parse_deadband::parse_deadband;
#[path = "denizen/default_rings.rs"]
mod default_rings;
pub use default_rings::default_rings;
#[path = "denizen/denizens_new.rs"]
mod denizens_new;
#[path = "denizen/stage_install.rs"]
mod stage_install;
pub use stage_install::stage_install;
#[path = "denizen/component_path.rs"]
mod component_path;
pub use component_path::component_path;
#[path = "denizen/review_line.rs"]
mod review_line;
pub use review_line::review_line;
#[path = "denizen/denizen_url.rs"]
mod denizen_url;
pub use denizen_url::denizen_url;
#[path = "denizen/nested_log_path.rs"]
mod nested_log_path;
pub use nested_log_path::nested_log_path;
#[path = "denizen/archived_world_path.rs"]
mod archived_world_path;
pub use archived_world_path::archived_world_path;
#[path = "denizen/archive_world.rs"]
mod archive_world;
pub use archive_world::archive_world;
#[path = "denizen/unarchive_world.rs"]
mod unarchive_world;
pub use unarchive_world::unarchive_world;
#[path = "denizen/purge_archived_world.rs"]
mod purge_archived_world;
pub use purge_archived_world::purge_archived_world;
#[path = "denizen/save_nested.rs"]
mod save_nested;
pub use save_nested::save_nested;
#[path = "denizen/load_nested.rs"]
mod load_nested;
pub use load_nested::load_nested;
#[path = "denizen/rebuild.rs"]
mod rebuild;
pub use rebuild::rebuild;
#[path = "denizen/resident_body.rs"]
mod resident_body;
use resident_body::resident_body;
#[path = "denizen/hex_to_bytes.rs"]
mod hex_to_bytes;
use hex_to_bytes::hex_to_bytes;
#[path = "denizen/watches_path.rs"]
mod watches_path;
pub use watches_path::watches_path;
#[path = "denizen/save_watches.rs"]
mod save_watches;
pub use save_watches::save_watches;
#[path = "denizen/load_watches.rs"]
mod load_watches;
pub use load_watches::load_watches;
#[path = "denizen/certs_path.rs"]
mod certs_path;
pub use certs_path::certs_path;
#[path = "denizen/save_certs.rs"]
mod save_certs;
pub use save_certs::save_certs;
#[path = "denizen/load_certs.rs"]
mod load_certs;
pub use load_certs::load_certs;
#[path = "denizen/residency_resource.rs"]
mod residency_resource;
pub use residency_resource::residency_resource;
#[path = "denizen/cert_nonce.rs"]
mod cert_nonce;
use cert_nonce::cert_nonce;
#[path = "denizen/now_ms.rs"]
mod now_ms;
pub use now_ms::now_ms;
#[path = "denizen/issue_install_certificates.rs"]
mod issue_install_certificates;
pub use issue_install_certificates::issue_install_certificates;
#[path = "denizen/save_certs_strict.rs"]
mod save_certs_strict;
use save_certs_strict::save_certs_strict;
#[path = "denizen/issue_install_certificates_for_generation.rs"]
mod issue_install_certificates_for_generation;
pub use issue_install_certificates_for_generation::issue_install_certificates_for_generation;
#[path = "denizen/install_caps.rs"]
mod install_caps;
pub use install_caps::install_caps;
#[path = "denizen/install.rs"]
mod install;
pub use install::install;
