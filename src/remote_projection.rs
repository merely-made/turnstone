// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's Graphshell endpoint.
//!
//! The adapter reads Turnstone's live graph, asks Mere cartography to disclose a
//! product-free score and scene, and keeps all rendered card data beside that
//! scene. Incoming intents return through Turnstone's ordinary [`Action`] spine
//! only after Servitor has evaluated the endpoint's projected grant.
//!
//! Graph coverage is local context beside the disclosed scene. The current
//! Chirograph V1 snapshot, FrozenScene and accessibility scene outputs do not
//! report that context; they must not be described as coverage-aware outputs.

use std::collections::{BTreeMap, HashMap};

use chartulary::{Container, EditSpec, GraphLog, Relation};
use chirograph::{
    AdvertisedAction, BoundsRelationship, CachePolicy, CapabilityProfile, CardValueV1, ContentHash,
    EndpointDescriptor, IntentEffect, IntentInvocation, IntentReference, IntentResult,
    NativeGlyphV1, PortableCardV1, PresentationBinding, PresentationCapability, PresentationCodec,
    PresentationKey, PresentationManifest, PresentationOffer, PresentationSemantics,
    ProjectionOffer, ProjectionRequest, ProjectionSession, ProjectionSnapshot, ProtocolVersion,
    ResourceRequest, ResourceResponse, SemanticRole,
};
use graphshell_client::{
    ClientState, PresentationResolution, ResolutionError, ResolvedPresentation,
};
use graphshell_endpoint::{IntentSink, PresentationSource, ProjectionCatalog, ProjectionSource};
use identity::IdentityProvider;
use mere::kernel::graph::{Author, CoverageNote, NodeKey};
use sceno::{Arrangement, Score, Spiral};
use scenotime::{Revision, SceneEpoch, SceneSnapshot};
use servitor::delegation::{DelegationTable, root_certificate};
use servitor::{Cap, Gate, Grant, Mode, ScopePath, Subject};

use crate::action::Action;
use crate::app::App;

const SESSION: &str = "loopback:turnstone:g3";
const LAYOUT_SCOPE: &str = "projection/layout/";
const GRAPH_SCOPE: &str = "graph/open/";

type EndpointAuthorRestore = crate::host_journal::AuthorRestore;

const FIT_INTENT: &str = "turnstone.fit-view";

const OPEN_INTENT: &str = "turnstone.open-address";

/// Typed refusals at the projection boundary. Version mismatch is checked
/// before scene/resource state is changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectionRefusal {
    WrongSession,
    UnsupportedProtocol,
    UnsupportedScore { received: u16, supported: u16 },
    UnsupportedArrangement,
    InvalidScene(String),
    Presentation(String),
}

/// The product endpoint. Its default card extent can be replaced by the host
/// once a real retained card has measured itself.
pub struct TurnstoneEndpoint {
    app: App,
    session: ProjectionSession,
    card_extent: (f32, f32),
    snapshot: Option<ProjectionSnapshot>,
    snapshot_coverage: Option<CoverageNote>,
    resources: BTreeMap<ContentHash, Vec<u8>>,
    gate: Gate,
    authority: DelegationTable,
    audit: GraphLog<Container, Relation>,
    subject: Subject,
}

/// Material returned by the executable G3 canary and its receipt tests.
pub struct G3Run {
    pub session: ProjectionSession,
    pub presentations: Vec<ResolvedPresentation>,
    pub layout: graphshell_host::view::ProjectionLayoutView,
    pub fit_result: IntentResult,
    pub open_result: IntentResult,
    pub graph_revision_before: u64,
    pub graph_revision_after: u64,
    pub graph_nodes_before: usize,
    pub graph_nodes_after: usize,
    pub audit_revision: u64,
    pub audit_author: String,
}

#[cfg(test)]
#[path = "remote_projection/tests.rs"]
mod tests;

#[path = "remote_projection/layout_scope.rs"]
mod layout_scope;
use layout_scope::layout_scope;
#[path = "remote_projection/graph_scope.rs"]
mod graph_scope;
use graph_scope::graph_scope;
#[path = "remote_projection/fit_label.rs"]
mod fit_label;
use fit_label::fit_label;
#[path = "remote_projection/disclose_scene.rs"]
mod disclose_scene;
#[path = "remote_projection/impl_std_error_error_for_projection.rs"]
mod impl_std_error_error_for_projection;
#[path = "remote_projection/impl_std_fmt_display_for_projection.rs"]
mod impl_std_fmt_display_for_projection;
#[path = "remote_projection/turnstone_endpoint_new.rs"]
mod turnstone_endpoint_new;
pub(crate) use disclose_scene::disclose_scene;
#[path = "remote_projection/impl_intent_sink_for_turnstone_endpoint.rs"]
mod impl_intent_sink_for_turnstone_endpoint;
#[path = "remote_projection/impl_presentation_source_for_turnstone_endpoint.rs"]
mod impl_presentation_source_for_turnstone_endpoint;
#[path = "remote_projection/impl_projection_catalog_for_turnstone_endpoint.rs"]
mod impl_projection_catalog_for_turnstone_endpoint;
#[path = "remote_projection/impl_projection_source_for_turnstone_endpoint.rs"]
mod impl_projection_source_for_turnstone_endpoint;
#[path = "remote_projection/run_g3_canary.rs"]
mod run_g3_canary;
pub use run_g3_canary::run_g3_canary;
#[path = "remote_projection/resolve_all.rs"]
mod resolve_all;
use resolve_all::resolve_all;
#[path = "remote_projection/resolution_error.rs"]
mod resolution_error;
use resolution_error::resolution_error;
#[path = "remote_projection/render_g3_receipt.rs"]
mod render_g3_receipt;
pub use render_g3_receipt::render_g3_receipt;
