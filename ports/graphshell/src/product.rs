// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Daily graph-product operations composed from Mere's portable graph and canvas surfaces.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use chartulary::AcceptAll;
use chirograph::{PortableCardV1, Sha256NamedInformation};
use mere::canvas::CartographyGeometry;
use mere::kernel::geometry::PortablePoint;
use mere::kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta, assert_relation};
use mere::kernel::graph::capture::{CapturedDelta, replay_captured_deltas_onto};
use mere::kernel::graph::node_facets::SEMANTIC_PROPERTIES;
use mere::kernel::graph::predicate_declarations::PREDICATE_DECLARATIONS_FACET;
use mere::kernel::graph::resource::TAGGED_WITH_IRI;
use mere::kernel::graph::{
    ArrangementSubKind, ContainmentSubKind, EdgeAssertion, EdgeFamily, Graph, NodeFacetStore,
    RelationKind, SemanticSubKind, import_edits,
};
use mere::kernel::persistence::GraphSnapshot;
use mere::kernel::types::NodeProperty;
use muniment::Backend;
use pandect::graph_placement::{PlacementProfile, materialize_snapshot};
use sceno::SourceRef;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::mere_host::{MereHost, MereHostError};
use crate::projection_compile::ProjectionSnapshot;

/// What an undo kept because another author changed it since, in words for
/// a status line (Scenograph editor plan, SE21). `None` when nothing was kept.
pub fn kept_summary(kept: &[pandect::Kept]) -> Option<String> {
    use mere::kernel::graph::Part;
    if kept.is_empty() {
        return None;
    }
    let parts: Vec<String> = kept
        .iter()
        .map(|kept| {
            let part = match &kept.part {
                Part::Node(_) => "the node".to_string(),
                Part::Title(_) => "the title".to_string(),
                Part::Facet(_, facet) => format!("the {facet} facet"),
                other => format!("{other:?}"),
            };
            let by: Vec<String> = kept.by.iter().map(ToString::to_string).collect();
            format!("{part}, changed since by {}", by.join(" and "))
        })
        .collect();
    Some(format!("kept {}", parts.join("; ")))
}

/// The address a saved projection definition lives at.
pub fn projection_address(definition_id: &str) -> String {
    format!("mere://projection/{definition_id}")
}

pub const LOCAL_FILE_FACET: &str = "graphshell.local-file/v1";
pub const CONTENT_FACET: &str = "graphshell.content/v1";
/// Where a scene is saved: [`SavedSceneV3`], the dynamics spec its only
/// physics and arrangement record (dynamics grammar plan, F142, F151).
pub const SAVED_SCENE_FACET: &str = "graphshell.saved-scene/v4";
/// Where a scene was saved with flat physics fields and roles (G7, F44).
/// Read only, as [`SavedSceneV2`], and converted (F142).
pub const SAVED_SCENE_FACET_V3: &str = "graphshell.saved-scene/v3";
/// Where a scene was saved before the arrangement roles. Read only: its
/// anchor pull is read as the roles it acted as ([`SavedSceneV2::roles`]),
/// and converted as a version-3 scene is.
pub const SAVED_SCENE_FACET_V2: &str = "graphshell.saved-scene/v2";
/// The arrangement a scene that names none opens on.
pub const DEFAULT_ARRANGEMENT: &str = "phyllotaxis.default";
pub const PINNED_PROJECTION_FACET: &str = "graphshell.pinned-projection/v1";
/// Where the projection editor saves a definition and its selected
/// occurrence, on a node at [`projection_address`] (Scenograph editor plan,
/// SE14).
pub const PROJECTION_DEFINITION_FACET: &str = "graphshell.projection-definition/v1";
/// The channel the projection editor's saves come through, so its Undo save
/// reaches only them and Graphshell's session undo never does (SE18).
pub const PROJECTION_EDITOR_VIA: &str = "graphshell.projection-editor";
/// A codicil carrying a version-4 scene ([`ProductCodicilV3`], F159).
pub const PRODUCT_CODICIL_SCHEMA: &str = "graphshell.graph-codicil/v3";
/// Read only: a codicil whose scene is a [`SavedSceneV2`], converted on read.
pub const PRODUCT_CODICIL_SCHEMA_V2: &str = "graphshell.graph-codicil/v2";
/// Read-only compatibility tag for graph selections exported before the
/// Engram-to-Codicil vocabulary migration.
pub const LEGACY_PRODUCT_ENGRAM_SCHEMA: &str = "graphshell.graph-engram/v1";

/// The host-owned elapsed clock for one projection transition.
///
/// Browser frame timestamps enter through [`observe`](Self::observe). Pausing
/// keeps observing the host so resuming cannot charge the paused interval to
/// the transition. Scenotime sees only the resulting elapsed value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectionClock {
    elapsed_ms: f32,
    last_host_ms: Option<f64>,
    paused: bool,
}

impl ProjectionClock {
    pub fn observe(&mut self, host_ms: f64) -> f32 {
        if !host_ms.is_finite() {
            return self.elapsed_ms;
        }
        let previous = self.last_host_ms.replace(host_ms);
        if !self.paused
            && let Some(previous) = previous
        {
            self.elapsed_ms += (host_ms - previous).max(0.0) as f32;
        }
        self.elapsed_ms
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn elapsed_ms(&self) -> f32 {
        self.elapsed_ms
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransferScope {
    ObjectOnly,
    DirectRelations,
    SelectedSubgraph,
    SavedScene,
}

impl TransferScope {
    pub fn code(self) -> &'static str {
        match self {
            Self::ObjectOnly => "object-only",
            Self::DirectRelations => "direct-relations",
            Self::SelectedSubgraph => "selected-subgraph",
            Self::SavedScene => "saved-scene",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "object-only" => Some(Self::ObjectOnly),
            "direct-relations" => Some(Self::DirectRelations),
            "selected-subgraph" => Some(Self::SelectedSubgraph),
            "saved-scene" => Some(Self::SavedScene),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationFamilyFilter {
    All,
    Semantic,
    Traversal,
    Containment,
    Arrangement,
    Imported,
    Provenance,
}

impl RelationFamilyFilter {
    pub fn code(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Semantic => "semantic",
            Self::Traversal => "traversal",
            Self::Containment => "containment",
            Self::Arrangement => "arrangement",
            Self::Imported => "imported",
            Self::Provenance => "provenance",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "all" => Some(Self::All),
            "semantic" => Some(Self::Semantic),
            "traversal" => Some(Self::Traversal),
            "containment" => Some(Self::Containment),
            "arrangement" => Some(Self::Arrangement),
            "imported" => Some(Self::Imported),
            "provenance" => Some(Self::Provenance),
            _ => None,
        }
    }

    fn accepts(self, family: EdgeFamily) -> bool {
        self == Self::All
            || matches!(
                (self, family),
                (Self::Semantic, EdgeFamily::Semantic)
                    | (Self::Traversal, EdgeFamily::Traversal)
                    | (Self::Containment, EdgeFamily::Containment)
                    | (Self::Arrangement, EdgeFamily::Arrangement)
                    | (Self::Imported, EdgeFamily::Imported)
                    | (Self::Provenance, EdgeFamily::Provenance)
            )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EditableRelation {
    UserGrouped,
    Cites,
    Hyperlink,
    CollectionMember,
    FrameMember,
}

impl EditableRelation {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "user-grouped" => Some(Self::UserGrouped),
            "cites" => Some(Self::Cites),
            "hyperlink" => Some(Self::Hyperlink),
            "collection-member" => Some(Self::CollectionMember),
            "frame-member" => Some(Self::FrameMember),
            _ => None,
        }
    }

    fn assertion(self) -> EdgeAssertion {
        match self {
            Self::UserGrouped => EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::UserGrouped,
                label: None,
                decay_progress: None,
            },
            Self::Cites => EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Cites,
                label: None,
                decay_progress: None,
            },
            Self::Hyperlink => EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Hyperlink,
                label: None,
                decay_progress: None,
            },
            Self::CollectionMember => EdgeAssertion::Containment {
                sub_kind: ContainmentSubKind::CollectionMember,
            },
            Self::FrameMember => EdgeAssertion::Arrangement {
                sub_kind: ArrangementSubKind::FrameMember,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalFileMetadata {
    pub content_hash: String,
    pub name: String,
    pub media_type: String,
    pub byte_len: u64,
    pub last_modified_ms: u64,
}

/// A saved scene as written under [`SAVED_SCENE_FACET_V3`] and
/// [`SAVED_SCENE_FACET_V2`]: read only since G4b1, and converted to a
/// [`SavedSceneV3`] ([`Self::into_v3`], F142). Version 2 (dynamics grammar
/// plan, G7, F44) added the arrangement's roles, and `arrangement_pull`
/// became the anchored role's stiffness; a version-1 scene, saved under
/// [`SAVED_SCENE_FACET_V2`], reads into this type with no roles and is read
/// as it behaved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedSceneV2 {
    pub name: String,
    pub selected: Vec<Uuid>,
    pub layout_strategy: Option<String>,
    pub physics_paused: bool,
    pub physics_damping: f32,
    /// The physics law id (`mere::canvas::PhysicsLaw::id`); Springs when absent,
    /// so a scene saved before the catalog reads as it always did.
    #[serde(default = "default_physics_law")]
    pub physics_law: String,
    /// The overlay ids composed onto the law, in run order.
    #[serde(default)]
    pub physics_overlays: Vec<String>,
    /// Where the Kinds law reads a node's kind from (`site`, `cluster`, `degree`).
    #[serde(default = "default_physics_kind_source")]
    pub physics_kind_source: String,
    /// Where Group pull reads its groups from (any kind source); site when
    /// absent, so a scene saved before the channel reads as it did. (Dynamics
    /// grammar plan, G2, F37.)
    #[serde(default = "default_physics_group_source")]
    pub physics_group_source: String,
    /// Where Orbit's masses and the hub overlays' weights come from (`degree`, `pagerank`).
    #[serde(default = "default_physics_mass_source")]
    pub physics_mass_source: String,
    /// Where the Depth overlay reads depth from (`roots`, `layers`, `focus`).
    #[serde(default = "default_physics_depth_source")]
    pub physics_depth_source: String,
    /// The anchored role's return stiffness. A scene saved before G7 stored
    /// its anchor pull here, and with no `arrangement_roles` that pull is read
    /// as the roles it acted as (see [`SavedSceneV2::roles`]).
    #[serde(default = "default_anchor_stiffness")]
    pub arrangement_pull: f32,
    /// The arrangement's roles: recipe default, site groups, items by member
    /// (dynamics grammar plan, G7). Absent in a scene saved before G7.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrangement_roles: Option<SavedRolesV1>,
    /// The scene's dynamics spec (dynamics grammar plan, G4a): absent from
    /// every scene saved before it and written by nothing before G4b1 (F100).
    /// It rides opaque and is read when the scene opens, where a refused spec
    /// fails that open alone (F97, F114); converted, it wins over the flat
    /// fields, which fill only what it lacks (F153).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamics: Option<SavedDynamics>,
    pub camera_offset: (f32, f32),
    pub camera_zoom: f32,
    pub default_handler: String,
    pub cartography: CartographyGeometry,
}

/// An arrangement's roles as a scene saves them, by role id
/// (`seeded`, `anchored`, `pinned`; `mere::canvas::Role::id`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedRolesV1 {
    pub default: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub groups: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub items: BTreeMap<Uuid, String>,
}

/// A scene's dynamics spec as it rides: an opaque value, so whatever carries
/// the scene (the facet, a codicil, a personal graph record) reads it whole
/// and the spec is read only when the scene opens (F114). Read through
/// seiche's path-tracking reader, so a refusal names where it sits (F113).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SavedDynamics(serde_json::Value);

impl SavedDynamics {
    pub fn from_spec(spec: &mere::canvas::dynamics_spec::DynamicsSpec) -> Self {
        Self(serde_json::to_value(spec).expect("a dynamics spec always serializes"))
    }

    /// The spec as written, unread.
    pub fn value(&self) -> &serde_json::Value {
        &self.0
    }

    /// The spec, or where and why it is refused while read.
    pub fn spec(&self) -> Result<mere::canvas::dynamics_spec::DynamicsSpec, String> {
        mere::canvas::dynamics_spec::read(&self.0)
            .map_err(|error| format!("dynamics spec: {error}"))
    }
}

/// A saved scene, version 4 of the facet ([`SAVED_SCENE_FACET`]; dynamics
/// grammar plan, F142, F151): the dynamics spec is its only physics and
/// arrangement record, the law, overlays, sources, seed, damping, the
/// arrangement and its roles all inside it. What stays beside it is view
/// state: the selection, the camera, whether physics plays (F152), the
/// handler and the representations.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedSceneV3 {
    pub name: String,
    pub selected: Vec<Uuid>,
    /// Whether physics is paused when the scene opens: view state, as the
    /// camera is (F152).
    pub physics_paused: bool,
    /// The record (F142), opaque until the scene opens (F114).
    pub dynamics: SavedDynamics,
    pub camera_offset: (f32, f32),
    pub camera_zoom: f32,
    pub default_handler: String,
    pub cartography: CartographyGeometry,
}

impl SavedSceneV3 {
    /// The scene's spec, read, derived and bound over the canvas's catalog
    /// (graph-free); a refusal names the id and where it sits (F97, F113,
    /// F149).
    pub fn dynamics_spec(&self) -> Result<mere::canvas::dynamics_spec::DynamicsSpec, String> {
        let spec = self.dynamics.spec()?;
        mere::canvas::dynamics_spec::bind(&spec)
            .map_err(|error| format!("dynamics spec: {error}"))?;
        Ok(spec)
    }

    /// Whether the scene's spec reads, derives and binds.
    pub fn check_dynamics(&self) -> Result<(), String> {
        self.dynamics_spec().map(|_| ())
    }

    /// The arrangement the scene's target names, or [`DEFAULT_ARRANGEMENT`].
    pub fn arrangement(&self) -> Result<String, String> {
        Ok(self
            .dynamics_spec()?
            .target
            .map(|target| target.arrangement)
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| DEFAULT_ARRANGEMENT.to_string()))
    }
}

impl SavedSceneV2 {
    /// The scene as a version-4 scene (F142). A spec it carries wins, and
    /// the flat fields fill only what it lacks: the target when it has none,
    /// the damping when its realization leaves it unset (F153). Without a
    /// spec the flat fields become one, an unknown law, overlay or source id
    /// failing with the id (F143, F154), and an unknown role id as before.
    pub fn into_v3(self) -> Result<SavedSceneV3, String> {
        use mere::canvas::dynamics_spec::Realization;
        let mut spec = match &self.dynamics {
            Some(dynamics) => dynamics.spec()?,
            None => self.flat_choice()?.into_spec(),
        };
        if spec.target.is_none() {
            spec.target = Some(self.flat_target()?);
        }
        let Realization::Integrate { damping } = &mut spec.realization;
        if damping.is_none() {
            *damping = Some(f64::from(self.physics_damping));
        }
        Ok(SavedSceneV3 {
            name: self.name,
            selected: self.selected,
            physics_paused: self.physics_paused,
            dynamics: SavedDynamics::from_spec(&spec),
            camera_offset: self.camera_offset,
            camera_zoom: self.camera_zoom,
            default_handler: self.default_handler,
            cartography: self.cartography,
        })
    }

    /// The flat physics fields as a choice; an unknown id fails with it.
    fn flat_choice(&self) -> Result<mere::canvas::PhysicsChoice, String> {
        use mere::canvas::{
            PhysicsDepthSource, PhysicsKindSource, PhysicsLaw, PhysicsMassSource, PhysicsOverlay,
        };
        let kind = |id: &str, what: &str| {
            PhysicsKindSource::parse(id).ok_or_else(|| format!("unknown {what} source {id}"))
        };
        let mut overlays: Vec<PhysicsOverlay> = Vec::new();
        for id in &self.physics_overlays {
            let overlay =
                PhysicsOverlay::parse(id).ok_or_else(|| format!("unknown physics overlay {id}"))?;
            if !overlays.contains(&overlay) {
                overlays.push(overlay);
            }
        }
        Ok(mere::canvas::PhysicsChoice {
            law: PhysicsLaw::parse(&self.physics_law)
                .ok_or_else(|| format!("unknown physics law {}", self.physics_law))?,
            overlays,
            kind: kind(&self.physics_kind_source, "kind")?,
            groups: kind(&self.physics_group_source, "group")?,
            mass: PhysicsMassSource::parse(&self.physics_mass_source)
                .ok_or_else(|| format!("unknown mass source {}", self.physics_mass_source))?,
            depth: PhysicsDepthSource::parse(&self.physics_depth_source)
                .ok_or_else(|| format!("unknown depth source {}", self.physics_depth_source))?,
        })
    }

    /// The flat arrangement fields as the spec's target: the layout, the
    /// anchored stiffness and the roles, read as [`Self::roles`] reads them;
    /// group roles keyed by site, the only group source before G4b1.
    fn flat_target(&self) -> Result<mere::canvas::dynamics_spec::Target, String> {
        let (roles, stiffness) = self.roles()?;
        Ok(mere::canvas::dynamics_spec::Target {
            arrangement: self
                .layout_strategy
                .clone()
                .unwrap_or_else(|| DEFAULT_ARRANGEMENT.to_string()),
            anchored_pull: f64::from(stiffness),
            default_role: roles.default,
            groups: (!roles.groups.is_empty()).then(|| mere::canvas::dynamics_spec::GroupRoles {
                channel: "groups.site".to_string(),
                roles: roles.groups,
            }),
            items: roles
                .items
                .into_iter()
                .map(|(member, role)| (member.to_string(), role))
                .collect(),
        })
    }

    /// The roles this scene opens with, and the anchored stiffness. A scene
    /// saved before G7 has only its pull: the canvas anchored every item at
    /// that pull while playing, so a positive pull reads as anchored at it and
    /// zero as seeded, and the scene behaves as it did. An unknown role id
    /// fails rather than falling back.
    pub fn roles(&self) -> Result<(SavedRoles, f32), String> {
        let parse = |id: &str| {
            mere::canvas::Role::parse(id).ok_or_else(|| format!("unknown arrangement role {id}"))
        };
        let Some(saved) = &self.arrangement_roles else {
            return Ok(if self.arrangement_pull > 0.0 {
                (
                    SavedRoles::uniform(mere::canvas::Role::Anchored),
                    self.arrangement_pull,
                )
            } else {
                (
                    SavedRoles::uniform(mere::canvas::Role::Seeded),
                    default_anchor_stiffness(),
                )
            });
        };
        let mut roles = SavedRoles::uniform(parse(&saved.default)?);
        for (group, id) in &saved.groups {
            roles.groups.insert(group.clone(), parse(id)?);
        }
        for (member, id) in &saved.items {
            roles.items.insert(*member, parse(id)?);
        }
        Ok((roles, self.arrangement_pull))
    }
}

/// [`SavedRolesV1`] parsed: items by member, since node keys are a canvas's
/// own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedRoles {
    pub default: mere::canvas::Role,
    pub groups: BTreeMap<String, mere::canvas::Role>,
    pub items: BTreeMap<Uuid, mere::canvas::Role>,
}

impl SavedRoles {
    pub fn uniform(role: mere::canvas::Role) -> Self {
        Self {
            default: role,
            groups: BTreeMap::new(),
            items: BTreeMap::new(),
        }
    }

    /// The canvas's table, items rebound from members to `graph`'s keys; a
    /// member the graph lacks is dropped.
    pub fn table(&self, graph: &mere::kernel::graph::Graph) -> mere::canvas::RoleTable {
        mere::canvas::RoleTable {
            default: self.default,
            groups: self.groups.clone(),
            items: self
                .items
                .iter()
                .filter_map(|(member, role)| Some((graph.get_node_key_by_id(*member)?, *role)))
                .collect(),
        }
    }

    /// A canvas's table, items by member.
    pub fn from_table(table: &mere::canvas::RoleTable, graph: &mere::kernel::graph::Graph) -> Self {
        Self {
            default: table.default,
            groups: table.groups.clone(),
            items: table
                .items
                .iter()
                .filter_map(|(key, role)| Some((graph.get_node(*key)?.id, *role)))
                .collect(),
        }
    }

    /// The saved form, by role id.
    pub fn saved(&self) -> SavedRolesV1 {
        SavedRolesV1 {
            default: self.default.id().to_string(),
            groups: self
                .groups
                .iter()
                .map(|(group, role)| (group.clone(), role.id().to_string()))
                .collect(),
            items: self
                .items
                .iter()
                .map(|(member, role)| (*member, role.id().to_string()))
                .collect(),
        }
    }
}

fn default_anchor_stiffness() -> f32 {
    mere::canvas::DEFAULT_ANCHOR_STIFFNESS
}

fn default_physics_law() -> String {
    mere::canvas::PhysicsLaw::Springs.id().to_string()
}

fn default_physics_kind_source() -> String {
    mere::canvas::PhysicsKindSource::Site.id().to_string()
}

fn default_physics_group_source() -> String {
    mere::canvas::PhysicsKindSource::Site.id().to_string()
}

fn default_physics_mass_source() -> String {
    mere::canvas::PhysicsMassSource::Degree.id().to_string()
}

fn default_physics_depth_source() -> String {
    mere::canvas::PhysicsDepthSource::Roots.id().to_string()
}

/// User-selected public card copied from a mounted endpoint into local graph
/// truth. The source remains authoritative. Actions are deliberately absent,
/// because they are valid only in the live admitted session that advertised
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PinnedProjectionAuthorityV1 {
    SourceOwned,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedProjectionCardV1 {
    pub source: SourceRef,
    pub observed_session: String,
    pub observed_epoch: u64,
    pub observed_revision: u64,
    pub authority: PinnedProjectionAuthorityV1,
    pub card: PortableCardV1,
}

/// A graph selection, with a version-4 scene when it carries one (F159).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProductCodicilV3 {
    pub schema: String,
    pub scope: TransferScope,
    pub exported_at_ms: u64,
    pub graph: GraphSnapshot,
    pub facets: NodeFacetStore,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<SavedSceneV3>,
}

/// A codicil as written before version 3, read and converted.
#[derive(Deserialize)]
struct ProductCodicilV2 {
    schema: String,
    scope: TransferScope,
    exported_at_ms: u64,
    graph: GraphSnapshot,
    facets: NodeFacetStore,
    #[serde(default)]
    scene: Option<SavedSceneV2>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ProfiledProductCodicil {
    #[serde(flatten)]
    pub product: ProductCodicilV3,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<PlacementProfile>,
}

impl std::ops::Deref for ProfiledProductCodicil {
    type Target = ProductCodicilV3;
    fn deref(&self) -> &Self::Target {
        &self.product
    }
}

impl std::ops::DerefMut for ProfiledProductCodicil {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.product
    }
}

#[derive(Clone, Debug)]
pub struct ExportRequest {
    pub focused: Uuid,
    pub selected: Vec<Uuid>,
    pub scope: TransferScope,
    pub exported_at_ms: u64,
    pub include_local_file_locations: bool,
    pub scene: Option<SavedSceneV3>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportReceipt {
    pub nodes: usize,
    pub relations: usize,
    pub facets: usize,
}

#[derive(Debug)]
pub enum ProductError {
    Host(MereHostError),
    UnknownNode(Uuid),
    UnknownAddress(String),
    InvalidFacetJson(String),
    EmptySelection,
    InvalidCodicil(String),
    InvalidContentReference(String),
    /// A saved scene that reads but does not open: its dynamics spec is
    /// refused (dynamics grammar plan, F97).
    InvalidScene(String),
}

impl std::fmt::Display for ProductError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host(error) => write!(formatter, "{error}"),
            Self::UnknownNode(id) => write!(formatter, "unknown graph object {id}"),
            Self::UnknownAddress(address) => write!(formatter, "unknown address {address}"),
            Self::InvalidFacetJson(error) => write!(formatter, "invalid facet JSON: {error}"),
            Self::EmptySelection => write!(formatter, "the transfer scope contains no objects"),
            Self::InvalidCodicil(error) => write!(formatter, "invalid graph codicil: {error}"),
            Self::InvalidContentReference(error) => {
                write!(formatter, "invalid portable content reference: {error}")
            },
            Self::InvalidScene(error) => write!(formatter, "the scene does not open: {error}"),
        }
    }
}

impl std::error::Error for ProductError {}

impl From<MereHostError> for ProductError {
    fn from(value: MereHostError) -> Self {
        Self::Host(value)
    }
}

impl<B: Backend> MereHost<B> {
    pub fn create_address(&mut self, address: &str, title: &str) -> Result<Uuid, ProductError> {
        if let Some((_, node)) = self.graph().get_node_by_url(address) {
            return Ok(node.id);
        }
        let id = Uuid::new_v4();
        self.mutate_product_graph(|graph| {
            let key = add_node(
                graph,
                Some(id),
                address.to_string(),
                PortablePoint::new(0.0, 0.0),
            );
            if !title.trim().is_empty() {
                apply_graph_delta(
                    graph,
                    GraphDelta::SetNodeTitle {
                        key,
                        title: title.trim().to_string(),
                    },
                );
            }
        })?;
        Ok(id)
    }

    pub fn create_file_metadata(
        &mut self,
        metadata: LocalFileMetadata,
    ) -> Result<Uuid, ProductError> {
        let portable_id = Sha256NamedInformation::from_hex(&metadata.content_hash)
            .map_err(|error| ProductError::InvalidContentReference(error.to_string()))?;
        let address = portable_id.to_string();
        let id = self.create_address(&address, &metadata.name)?;
        let key = self
            .graph()
            .get_node_key_by_id(id)
            .ok_or(ProductError::UnknownNode(id))?;
        self.mutate_product_graph(|graph| {
            apply_graph_delta(
                graph,
                GraphDelta::SetNodeMimeHint {
                    key,
                    mime_hint: (!metadata.media_type.is_empty())
                        .then_some(metadata.media_type.clone()),
                },
            );
        })?;
        self.set_facet(
            key,
            CONTENT_FACET,
            serde_json::json!({
                "portable_id": portable_id,
                "byte_len": metadata.byte_len,
                "media_type": metadata.media_type,
            }),
        )?;
        self.set_facet(
            key,
            LOCAL_FILE_FACET,
            serde_json::json!({
                "name": metadata.name,
                "last_modified_ms": metadata.last_modified_ms,
                "source": "browser-file-picker",
            }),
        )?;
        Ok(id)
    }

    pub fn edit_node(
        &mut self,
        id: Uuid,
        title: &str,
        tags: impl IntoIterator<Item = String>,
    ) -> Result<(), ProductError> {
        let key = self
            .graph()
            .get_node_key_by_id(id)
            .ok_or(ProductError::UnknownNode(id))?;
        let wanted: BTreeSet<_> = tags
            .into_iter()
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect();
        let current: BTreeSet<_> = self
            .graph()
            .node_content_tags(key)
            .expect("key resolved above")
            .into_iter()
            .collect();
        self.mutate_product_graph(|graph| {
            apply_graph_delta(
                graph,
                GraphDelta::SetNodeTitle {
                    key,
                    title: title.trim().to_string(),
                },
            );
            for tag in current.difference(&wanted) {
                apply_graph_delta(
                    graph,
                    GraphDelta::RemoveNodeTag {
                        key,
                        tag: tag.clone(),
                    },
                );
            }
            for tag in &wanted {
                apply_graph_delta(
                    graph,
                    GraphDelta::InsertNodeTag {
                        key,
                        tag: tag.clone(),
                    },
                );
            }
        })?;
        Ok(())
    }

    pub fn set_product_facet(
        &mut self,
        id: Uuid,
        facet: &str,
        json_value: &str,
    ) -> Result<(), ProductError> {
        let key = self
            .graph()
            .get_node_key_by_id(id)
            .ok_or(ProductError::UnknownNode(id))?;
        let value = serde_json::from_str(json_value)
            .map_err(|error| ProductError::InvalidFacetJson(error.to_string()))?;
        self.set_facet(key, facet.trim(), value)?;
        Ok(())
    }

    pub fn assert_product_relation(
        &mut self,
        from: Uuid,
        to: Uuid,
        relation: EditableRelation,
    ) -> Result<(), ProductError> {
        let from = self
            .graph()
            .get_node_key_by_id(from)
            .ok_or(ProductError::UnknownNode(from))?;
        let to = self
            .graph()
            .get_node_key_by_id(to)
            .ok_or(ProductError::UnknownNode(to))?;
        self.mutate_product_graph(|graph| {
            assert_relation(graph, from, to, relation.assertion());
        })?;
        Ok(())
    }

    pub fn matching_members(&self, query: &str, family: RelationFamilyFilter) -> Vec<Uuid> {
        let query = query.trim().to_lowercase();
        let related: HashSet<_> = self
            .graph()
            .projected_relations()
            .filter(|(_, relation)| family.accepts(family_of(relation.kind)))
            .flat_map(|(_, relation)| [relation.from, relation.to])
            .collect();
        self.graph()
            .nodes()
            .filter(|(key, _)| family == RelationFamilyFilter::All || related.contains(key))
            .filter(|(key, node)| {
                if query.is_empty() {
                    return true;
                }
                node.title.to_lowercase().contains(&query)
                    || node.url().to_lowercase().contains(&query)
                    || self.graph().node_content_tags(*key).is_some_and(|tags| {
                        tags.iter().any(|tag| tag.to_lowercase().contains(&query))
                    })
                    || self
                        .graph()
                        .facets()
                        .facets_of(&node.id)
                        .is_some_and(|facets| {
                            facets.iter().any(|(id, value)| {
                                id.as_str().to_lowercase().contains(&query)
                                    || value.to_string().to_lowercase().contains(&query)
                            })
                        })
            })
            .map(|(_, node)| node.id)
            .collect()
    }

    /// Save `scene` at `address` under [`SAVED_SCENE_FACET`] (F142: every
    /// save writes version 4).
    pub fn save_product_scene(
        &mut self,
        address: &str,
        scene: &SavedSceneV3,
    ) -> Result<Uuid, ProductError> {
        let id = self.create_address(address, &scene.name)?;
        let key = self
            .graph()
            .get_node_key_by_id(id)
            .ok_or(ProductError::UnknownNode(id))?;
        self.set_facet(key, SAVED_SCENE_FACET, serde_json::to_value(scene).unwrap())?;
        Ok(id)
    }

    /// Save a projection definition, with its selected occurrence, into the
    /// session as one change through [`PROJECTION_EDITOR_VIA`]: the node at
    /// its [`projection_address`] is made on the first save, and its facet
    /// replaced on each one after.
    pub fn save_projection(&mut self, snapshot: &ProjectionSnapshot) -> Result<Uuid, ProductError> {
        let address = projection_address(&snapshot.definition.id);
        let title = snapshot.definition.label.trim().to_string();
        let value = serde_json::to_value(snapshot)
            .map_err(|error| ProductError::InvalidCodicil(error.to_string()))?;
        let id = self
            .graph()
            .get_node_by_url(&address)
            .map(|(_, node)| node.id)
            .unwrap_or_else(Uuid::new_v4);
        self.through(PROJECTION_EDITOR_VIA.to_string(), |host| {
            host.mutate_product_graph(|graph| {
                let key = match graph.get_node_key_by_id(id) {
                    Some(key) => key,
                    None => {
                        let key = add_node(graph, Some(id), address, PortablePoint::new(0.0, 0.0));
                        if !title.is_empty() {
                            apply_graph_delta(graph, GraphDelta::SetNodeTitle { key, title });
                        }
                        key
                    },
                };
                apply_graph_delta(
                    graph,
                    GraphDelta::SetNodeFacet {
                        key,
                        facet: PROJECTION_DEFINITION_FACET.to_string(),
                        value,
                    },
                );
            })
        })?;
        Ok(id)
    }

    /// The projection saved for `definition_id`, if the session holds one.
    pub fn saved_projection(
        &self,
        definition_id: &str,
    ) -> Result<Option<ProjectionSnapshot>, ProductError> {
        self.facet_value(
            &projection_address(definition_id),
            PROJECTION_DEFINITION_FACET,
        )
        .map(|value| {
            serde_json::from_value(value.clone())
                .map_err(|error| ProductError::InvalidCodicil(error.to_string()))
        })
        .transpose()
    }

    /// The scene saved at `address`: a version-4 scene, else a version-3 or
    /// version-2 one, converted (F142). A scene whose spec is refused, or
    /// whose conversion meets an unknown id, does not open (F97, F143, F154).
    pub fn product_scene(&self, address: &str) -> Result<SavedSceneV3, ProductError> {
        let scene = if let Some(value) = self.facet_value(address, SAVED_SCENE_FACET) {
            serde_json::from_value::<SavedSceneV3>(value.clone())
                .map_err(|error| ProductError::InvalidCodicil(error.to_string()))?
        } else {
            let value = self
                .facet_value(address, SAVED_SCENE_FACET_V3)
                .or_else(|| self.facet_value(address, SAVED_SCENE_FACET_V2))
                .ok_or_else(|| ProductError::UnknownAddress(address.to_string()))?;
            serde_json::from_value::<SavedSceneV2>(value.clone())
                .map_err(|error| ProductError::InvalidCodicil(error.to_string()))?
                .into_v3()
                .map_err(ProductError::InvalidScene)?
        };
        scene.check_dynamics().map_err(ProductError::InvalidScene)?;
        Ok(scene)
    }

    pub fn export_product_codicil(&self, request: ExportRequest) -> Result<Vec<u8>, ProductError> {
        let members = transfer_members(self.graph(), &request)?;
        if members.is_empty() {
            return Err(ProductError::EmptySelection);
        }
        let graph = filtered_snapshot(self.graph(), &members, request.exported_at_ms);
        let facets = filtered_facets(
            self.graph().facets(),
            &members,
            request.include_local_file_locations,
        );
        let scene = request.scene.map(|scene| filter_scene(scene, &members));
        serde_json::to_vec_pretty(&ProfiledProductCodicil {
            placement: self.snapshot_placement(),
            product: ProductCodicilV3 {
                schema: PRODUCT_CODICIL_SCHEMA.to_string(),
                scope: request.scope,
                exported_at_ms: request.exported_at_ms,
                graph,
                facets,
                scene,
            },
        })
        .map_err(|error| ProductError::InvalidCodicil(error.to_string()))
    }

    /// Import a codicil into the graph as one journaled change: its new nodes
    /// arrive whole, its relations join the graph's, and its facets overwrite
    /// (reservoir plan §7 item 27).
    pub fn import_product_codicil(&mut self, bytes: &[u8]) -> Result<ImportReceipt, ProductError> {
        let input = decode_profiled_codicil(bytes)?;
        let codicil = input.product;
        let mut known: HashSet<String> = self
            .graph()
            .nodes()
            .map(|(_, node)| node.id.to_string())
            .collect();
        let receipt = ImportReceipt {
            nodes: codicil
                .graph
                .nodes
                .iter()
                .filter(|node| known.insert(node.node_id.clone()))
                .count(),
            relations: codicil.graph.edges.len(),
            facets: codicil.facets.iter().map(|(_, facets)| facets.len()).sum(),
        };
        let edits =
            product_import_edits(self.graph(), codicil.graph, codicil.facets, input.placement)?;
        self.apply_edits(edits)?;
        Ok(receipt)
    }
}

impl<B: Backend + Clone> MereHost<B> {
    /// Open a codicil as a new session begun from its graph, leaving the
    /// current session in the mere (reservoir plan §7 item 27).
    pub fn replace_with_product_codicil(
        &mut self,
        bytes: &[u8],
    ) -> Result<(ImportReceipt, Option<SavedSceneV3>), ProductError> {
        let input = decode_profiled_codicil(bytes)?;
        let codicil = input.product;
        if let Some(scene) = &codicil.scene {
            scene.check_dynamics().map_err(ProductError::InvalidScene)?;
        }
        let receipt = ImportReceipt {
            nodes: codicil.graph.nodes.len(),
            relations: codicil.graph.edges.len(),
            facets: codicil.facets.iter().map(|(_, facets)| facets.len()).sum(),
        };
        self.begin_profiled_session(
            profiled_codicil_graph(codicil.graph, codicil.facets, input.placement)?,
            input.placement,
        )?;
        Ok((receipt, codicil.scene))
    }
}

/// A codicil's graph with its facet store laid in whole: no codicil carries
/// legacy column data, so nothing the snapshot's columns import is kept.
fn codicil_graph(snapshot: GraphSnapshot, facets: NodeFacetStore) -> Result<Graph, ProductError> {
    profiled_codicil_graph(snapshot, facets, None)
}

fn profiled_codicil_graph(
    snapshot: GraphSnapshot,
    facets: NodeFacetStore,
    placement: Option<PlacementProfile>,
) -> Result<Graph, ProductError> {
    let mut graph = materialize_snapshot(&snapshot, placement)
        .map_err(|error| ProductError::InvalidCodicil(error.to_string()))?;
    *graph.facets_mut() = facets;
    graph
        .validate_active_resource_assertion_handles()
        .map_err(|error| ProductError::InvalidCodicil(error.to_string()))?;
    Ok(graph)
}

pub(crate) fn product_import_edits(
    live: &Graph,
    mut snapshot: GraphSnapshot,
    facets: NodeFacetStore,
    placement: Option<PlacementProfile>,
) -> Result<Vec<CapturedDelta>, ProductError> {
    let current = live.to_snapshot();
    // C1's missing-source marker is canonical even before raw-handle validation.
    for edge in snapshot
        .edges
        .iter_mut()
        .chain(&mut snapshot.resource_edges)
    {
        for statement in edge
            .semantic
            .iter_mut()
            .flat_map(|semantic| &mut semantic.statements)
        {
            statement.provenance_iri.get_or_insert_with(|| {
                mere::kernel::graph::edge_data::UNKNOWN_LEGACY_ASSERTER_IRI.into()
            });
        }
    }
    // Validate raw handles before legacy surface materialization can coalesce them.
    pandect::snapshot_merge::try_merge_snapshots(&current, &snapshot)
        .map_err(|error| ProductError::InvalidCodicil(error.to_string()))?;
    let incoming = profiled_codicil_graph(snapshot, facets, placement)?;
    let incoming_snapshot = incoming.to_snapshot();
    let (joined, _) = pandect::snapshot_merge::try_merge_snapshots(&current, &incoming_snapshot)
        .map_err(|error| ProductError::InvalidCodicil(error.to_string()))?;
    let target = materialize_snapshot(&joined, placement)
        .map_err(|error| ProductError::InvalidCodicil(error.to_string()))?
        .to_snapshot();

    // Keep the existing surface-field and incoming-facet import rules.
    let mut surface_snapshot = incoming_snapshot;
    surface_snapshot.resources.clear();
    surface_snapshot.resource_edges.clear();
    surface_snapshot.shown_resources.clear();
    let surfaces = profiled_codicil_graph(surface_snapshot, incoming.facets().clone(), placement)?;
    let mut edits = import_edits(live, &surfaces);
    for record in &target.resources {
        if !current.resources.contains(record) {
            edits.push(CapturedDelta::ReplaySetResourceRecordById {
                resource_id: chartulary::resource_id_from_canonical_iri(&record.canonical_iri)
                    .to_string(),
                record: Some(record.clone()),
            });
        }
    }
    let pairs = |snapshot: &GraphSnapshot| {
        let mut pairs = BTreeMap::<_, Vec<_>>::new();
        for edge in &snapshot.resource_edges {
            pairs
                .entry((edge.from_node_id.clone(), edge.to_node_id.clone()))
                .or_default()
                .push(edge.clone());
        }
        pairs
    };
    let held = pairs(&current);
    for ((from, to), edges) in pairs(&target) {
        if held.get(&(from.clone(), to.clone())) != Some(&edges) {
            edits.push(CapturedDelta::ReplaySetResourceEdgesByIds {
                from_resource_id: from,
                to_resource_id: to,
                edges,
            });
        }
    }
    for shown in &target.shown_resources {
        if !current.shown_resources.contains(shown) {
            edits.push(CapturedDelta::ReplaySetShownResourceById {
                surface_id: shown.surface_id.clone(),
                resource_id: Some(shown.resource_id.clone()),
            });
        }
    }

    // Replay setters can refuse input without an error result; prove the whole batch first.
    let mut scratch = live.clone();
    if edits.iter().any(|edit| edit.replay_delta().is_none()) {
        return Err(ProductError::InvalidCodicil(
            "an import edit cannot be replayed".into(),
        ));
    }
    replay_captured_deltas_onto(&mut scratch, edits.clone());
    scratch
        .validate_active_resource_assertion_handles()
        .map_err(|error| ProductError::InvalidCodicil(error.to_string()))?;
    let actual = scratch.to_snapshot();
    let resources = |snapshot: &GraphSnapshot| {
        snapshot
            .resources
            .iter()
            .map(|record| (record.canonical_iri.clone(), record.clone()))
            .collect::<BTreeMap<_, _>>()
    };
    if resources(&actual) != resources(&target)
        || actual.resource_edges != target.resource_edges
        || actual.shown_resources != target.shown_resources
    {
        return Err(ProductError::InvalidCodicil(
            "the import edits did not preserve the composed resource graph".into(),
        ));
    }
    let same_id = |a: &str, b: &str| match (Uuid::parse_str(a), Uuid::parse_str(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    };
    for (resource, expected) in joined
        .edges
        .iter()
        .map(|edge| (false, edge))
        .chain(joined.resource_edges.iter().map(|edge| (true, edge)))
    {
        for claim in expected
            .semantic
            .iter()
            .flat_map(|semantic| &semantic.statements)
        {
            let edges = if resource {
                &actual.resource_edges
            } else {
                &actual.edges
            };
            if !edges.iter().any(|edge| {
                same_id(&edge.from_node_id, &expected.from_node_id)
                    && same_id(&edge.to_node_id, &expected.to_node_id)
                    && edge
                        .semantic
                        .as_ref()
                        .is_some_and(|semantic| semantic.statements.contains(claim))
            }) {
                return Err(ProductError::InvalidCodicil(format!(
                    "the import edits did not preserve assertion {:?}",
                    claim.statement_id
                )));
            }
        }
    }
    Ok(edits)
}

fn family_of(kind: RelationKind) -> EdgeFamily {
    kind.family()
}

fn transfer_members(graph: &Graph, request: &ExportRequest) -> Result<HashSet<Uuid>, ProductError> {
    if graph.get_node_by_id(request.focused).is_none() {
        return Err(ProductError::UnknownNode(request.focused));
    }
    let mut members = HashSet::new();
    match request.scope {
        TransferScope::ObjectOnly => {
            members.insert(request.focused);
        },
        TransferScope::DirectRelations => {
            let focused = graph
                .get_node_key_by_id(request.focused)
                .expect("validated above");
            members.insert(request.focused);
            for (_, relation) in graph.projected_relations() {
                if relation.from == focused || relation.to == focused {
                    if let Some(node) = graph.get_node(relation.from) {
                        members.insert(node.id);
                    }
                    if let Some(node) = graph.get_node(relation.to) {
                        members.insert(node.id);
                    }
                }
            }
        },
        TransferScope::SelectedSubgraph => {
            members.extend(
                request
                    .selected
                    .iter()
                    .copied()
                    .filter(|id| graph.get_node_by_id(*id).is_some()),
            );
        },
        TransferScope::SavedScene => {
            let scene = request.scene.as_ref().ok_or(ProductError::EmptySelection)?;
            members.extend(
                scene
                    .selected
                    .iter()
                    .copied()
                    .filter(|id| graph.get_node_by_id(*id).is_some()),
            );
        },
    }
    Ok(members)
}

fn filtered_snapshot(graph: &Graph, members: &HashSet<Uuid>, exported_at_ms: u64) -> GraphSnapshot {
    let mut snapshot = graph.to_snapshot();
    let ids: HashSet<_> = members.iter().map(Uuid::to_string).collect();
    snapshot.nodes.retain(|node| ids.contains(&node.node_id));
    snapshot
        .edges
        .retain(|edge| ids.contains(&edge.from_node_id) && ids.contains(&edge.to_node_id));
    snapshot
        .shown_resources
        .retain(|shown| ids.contains(&shown.surface_id));
    let mut resource_ids: HashSet<_> = snapshot
        .shown_resources
        .iter()
        .map(|shown| shown.resource_id.clone())
        .collect();
    let tag_targets: Vec<_> = snapshot
        .resource_edges
        .iter()
        .filter(|edge| {
            resource_ids.contains(&edge.from_node_id)
                && edge.semantic.as_ref().is_some_and(|semantic| {
                    semantic
                        .statements
                        .iter()
                        .any(|statement| statement.predicate == TAGGED_WITH_IRI)
                })
        })
        .map(|edge| edge.to_node_id.clone())
        .collect();
    resource_ids.extend(tag_targets);
    let declaration_ids: HashSet<_> = snapshot
        .resources
        .iter()
        .filter(|resource| {
            resource
                .facets
                .iter()
                .any(|facet| facet.facet == PREDICATE_DECLARATIONS_FACET)
        })
        .map(|resource| {
            chartulary::resource_id_from_canonical_iri(&resource.canonical_iri).to_string()
        })
        .collect();
    resource_ids.extend(
        members
            .iter()
            .filter_map(|id| graph.get_node_key_by_id(*id))
            .flat_map(|key| graph.node_properties(key).unwrap_or_default())
            .map(|property| {
                chartulary::resource_id_from_canonical_iri(&property.predicate).to_string()
            })
            .filter(|id| declaration_ids.contains(id)),
    );
    loop {
        let mut needed: Vec<_> = snapshot
            .edges
            .iter()
            .chain(snapshot.resource_edges.iter().filter(|edge| {
                resource_ids.contains(&edge.from_node_id) && resource_ids.contains(&edge.to_node_id)
            }))
            .flat_map(|edge| &edge.semantic)
            .flat_map(|semantic| &semantic.statements)
            .map(|statement| {
                chartulary::resource_id_from_canonical_iri(&statement.predicate).to_string()
            })
            .filter(|id| declaration_ids.contains(id))
            .collect();
        needed.extend(
            snapshot
                .resources
                .iter()
                .filter(|resource| {
                    resource_ids.contains(
                        &chartulary::resource_id_from_canonical_iri(&resource.canonical_iri)
                            .to_string(),
                    )
                })
                .flat_map(|resource| &resource.facets)
                .filter(|facet| facet.facet == SEMANTIC_PROPERTIES)
                .flat_map(|facet| {
                    serde_json::from_str::<Vec<NodeProperty>>(&facet.value_json).unwrap_or_default()
                })
                .map(|property| {
                    chartulary::resource_id_from_canonical_iri(&property.predicate).to_string()
                })
                .filter(|id| declaration_ids.contains(id)),
        );
        let before = resource_ids.len();
        resource_ids.extend(needed);
        if resource_ids.len() == before {
            break;
        }
    }
    snapshot.resources.retain(|resource| {
        resource_ids.contains(
            &chartulary::resource_id_from_canonical_iri(&resource.canonical_iri).to_string(),
        )
    });
    snapshot.resource_edges.retain(|edge| {
        resource_ids.contains(&edge.from_node_id) && resource_ids.contains(&edge.to_node_id)
    });
    snapshot.import_records.clear();
    snapshot.fields.clear();
    snapshot.couplings.clear();
    snapshot.navigation = Default::default();
    snapshot.timestamp_secs = exported_at_ms / 1_000;
    snapshot
}

fn filtered_facets(
    source: &NodeFacetStore,
    members: &HashSet<Uuid>,
    include_local_file_locations: bool,
) -> NodeFacetStore {
    let mut facets = NodeFacetStore::new();
    for (node, node_facets) in source.iter() {
        if !members.contains(node) {
            continue;
        }
        for (facet, value) in node_facets.iter() {
            if !include_local_file_locations && facet.as_str() == LOCAL_FILE_FACET {
                continue;
            }
            facets
                .set(node.to_owned(), facet.clone(), value.clone(), &AcceptAll)
                .expect("AcceptAll cannot reject an exported facet");
        }
    }
    facets
}

/// The scene an export carries: its selection, representations and target
/// items narrowed to the exported members (F159). A spec this reader
/// refuses rides on unchanged, to be refused where the scene opens (F114).
fn filter_scene(mut scene: SavedSceneV3, members: &HashSet<Uuid>) -> SavedSceneV3 {
    scene.selected.retain(|id| members.contains(id));
    if let Ok(mut spec) = scene.dynamics.spec() {
        if let Some(target) = spec.target.as_mut() {
            target
                .items
                .retain(|id, _| Uuid::parse_str(id).is_ok_and(|member| members.contains(&member)));
        }
        scene.dynamics = SavedDynamics::from_spec(&spec);
    }
    scene.cartography = CartographyGeometry::from_positions(
        scene
            .cartography
            .iter()
            .filter(|(id, _)| members.contains(id)),
    )
    .with_sizes(
        scene
            .cartography
            .size_iter()
            .filter(|(id, _)| members.contains(id)),
    )
    .with_size_by_degree(scene.cartography.size_by_degree())
    .with_size_by_importance(scene.cartography.size_by_importance())
    .with_importance_metric(scene.cartography.importance_metric())
    .with_sprites(
        scene
            .cartography
            .sprite_iter()
            .filter(|(id, _)| members.contains(id))
            .map(|(id, uri)| (id, uri.to_string())),
    )
    .with_sprite_hulls(
        scene
            .cartography
            .sprite_hull_iter()
            .filter(|(id, _)| members.contains(id)),
    )
    .with_materials(
        scene
            .cartography
            .material_iter()
            .filter(|(id, _)| members.contains(id)),
    )
    .with_faces(
        scene
            .cartography
            .face_iter()
            .filter(|(id, _)| members.contains(id))
            .map(|(id, face)| (id, face.to_string())),
    );
    scene
}

pub(crate) fn decode_codicil(bytes: &[u8]) -> Result<ProductCodicilV3, ProductError> {
    Ok(decode_profiled_codicil(bytes)?.product)
}

pub(crate) fn decode_profiled_codicil(
    bytes: &[u8],
) -> Result<ProfiledProductCodicil, ProductError> {
    #[derive(Deserialize)]
    struct Head {
        schema: String,
        #[serde(default)]
        placement: Option<PlacementProfile>,
    }
    let invalid = |error: serde_json::Error| ProductError::InvalidCodicil(error.to_string());
    let Head { schema, placement } = serde_json::from_slice(bytes).map_err(invalid)?;
    let codicil = if schema == PRODUCT_CODICIL_SCHEMA {
        serde_json::from_slice::<ProductCodicilV3>(bytes).map_err(invalid)?
    } else if schema == PRODUCT_CODICIL_SCHEMA_V2 || schema == LEGACY_PRODUCT_ENGRAM_SCHEMA {
        let old: ProductCodicilV2 = serde_json::from_slice(bytes).map_err(invalid)?;
        ProductCodicilV3 {
            schema: old.schema,
            scope: old.scope,
            exported_at_ms: old.exported_at_ms,
            graph: old.graph,
            facets: old.facets,
            scene: old
                .scene
                .map(SavedSceneV2::into_v3)
                .transpose()
                .map_err(ProductError::InvalidScene)?,
        }
    } else {
        return Err(ProductError::InvalidCodicil(format!(
            "expected {PRODUCT_CODICIL_SCHEMA}, or {PRODUCT_CODICIL_SCHEMA_V2} or legacy \
             {LEGACY_PRODUCT_ENGRAM_SCHEMA} read and converted, found {schema}"
        )));
    };
    let ids: HashSet<_> = codicil
        .graph
        .nodes
        .iter()
        .map(|node| node.node_id.as_str())
        .collect();
    if codicil.graph.edges.iter().any(|edge| {
        !ids.contains(edge.from_node_id.as_str()) || !ids.contains(edge.to_node_id.as_str())
    }) {
        return Err(ProductError::InvalidCodicil(
            "a relation names an object outside the codicil".to_string(),
        ));
    }
    let input = ProfiledProductCodicil {
        product: codicil,
        placement,
    };
    profiled_codicil_graph(input.graph.clone(), input.facets.clone(), input.placement)?;
    Ok(input)
}

#[cfg(test)]
mod tests {
    use mere::kernel::graph::{Author, ProvenanceSubKind, RelationKind};
    use muniment::MemoryBackend;

    use super::*;
    use crate::access::AccessContext;
    use crate::mere_host::{
        FIXTURE_DEVICE_TWO_ADDRESS, FIXTURE_GRANT_ADDRESS, FIXTURE_PERSONA_ADDRESS,
        FIXTURE_RECEIPT_ADDRESS, FIXTURE_WEB_ADDRESS, GRAPHSHELL, SelectedPersonaRef,
        fixture_handlers,
    };

    fn selected_persona() -> SelectedPersonaRef {
        SelectedPersonaRef {
            persona: FIXTURE_PERSONA_ADDRESS.to_string(),
            profile: "profile:graphshell-h3".to_string(),
        }
    }

    fn projection(label: &str, selected: Option<&str>) -> ProjectionSnapshot {
        use crate::projection_editor::*;
        let definition = ProjectionDraft {
            dynamics: None,
            version: PROJECTION_DEFINITION_VERSION,
            id: "notes-by-topic".into(),
            label: label.into(),
            source: SourceBinding {
                authority: "local-device".into(),
                domain: "notes".into(),
                resource: "graph:notes".into(),
            },
            reading: Reading {
                kind: "nodes".into(),
                key: "topic".into(),
                value: None,
            },
            encoding: Encoding {
                x: Channel::Field("topic_x".into()),
                y: Channel::Field("topic_y".into()),
                color: None,
                label: Some(Channel::Field("title".into())),
            },
            arrangement: Arrangement::default(),
            interaction: Interaction::default(),
            appearance: Appearance {
                realization: "canvas".into(),
                title: label.into(),
                theme: "light".into(),
            },
            provenance: Provenance {
                author: "mark".into(),
                source_revision: Some("rev-7".into()),
                revision_evidence: RevisionEvidence::PublicGeneration,
                note: String::new(),
            },
        }
        .to_definition()
        .expect("a valid definition");
        ProjectionSnapshot {
            definition,
            selected_occurrence: selected.map(str::to_string),
        }
    }

    async fn open_on(backend: &MemoryBackend) -> MereHost<MemoryBackend> {
        MereHost::open(
            backend.clone(),
            selected_persona(),
            fixture_handlers(),
            AccessContext {
                persona: FIXTURE_PERSONA_ADDRESS.to_string(),
                device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                at_ms: 3_000,
            },
        )
        .await
        .expect("open")
    }

    /// Write the host's pending changes the way a browser host does: take the
    /// batch, write it through a clone of the store, mark it stored (SE22).
    async fn store_split(host: &mut MereHost<MemoryBackend>) {
        let staged = host.prepare_store(1_800_000_000).expect("prepare");
        staged.write(&host.store()).await.expect("write");
        host.staged(staged);
    }

    #[test]
    fn a_saved_projection_reopens_from_the_store() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let mut host = open_on(&backend).await;
            let saved = projection("Practice", Some("card:2"));
            host.save_projection(&saved).expect("save");
            store_split(&mut host).await;
            let reopened = open_on(&backend).await;
            let id = saved.definition.id.clone();
            assert_eq!(reopened.saved_projection(&id).unwrap(), Some(saved));
            assert_eq!(reopened.saved_projection("absent").unwrap(), None);
        });
    }

    #[test]
    fn undo_save_reaches_only_the_editors_channel() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let mut host = open_on(&backend).await;
            let first = projection("First", None);
            let second = projection("Second", Some("card:1"));
            let id = first.definition.id.clone();
            host.save_projection(&first).expect("first save");
            host.save_projection(&second).expect("second save");
            host.create_address("https://example.net/after", "After")
                .expect("a Graphshell change after the saves");

            let undone = host.undo_now(PROJECTION_EDITOR_VIA).expect("undo save");
            assert!(undone.is_some_and(|reverted| reverted.kept.is_empty()));
            assert_eq!(host.saved_projection(&id).unwrap(), Some(first.clone()));
            assert!(
                host.graph()
                    .get_node_by_url("https://example.net/after")
                    .is_some(),
                "Graphshell's own change is out of the editor's reach"
            );
            host.redo_now(PROJECTION_EDITOR_VIA).expect("redo save");
            assert_eq!(host.saved_projection(&id).unwrap(), Some(second.clone()));

            host.undo_now(PROJECTION_EDITOR_VIA).expect("undo again");
            host.undo_now(PROJECTION_EDITOR_VIA)
                .expect("undo the first save");
            assert_eq!(
                host.saved_projection(&id).unwrap(),
                None,
                "the first save made the node"
            );
            store_split(&mut host).await;
            let reopened = open_on(&backend).await;
            assert_eq!(
                reopened.saved_projection(&id).unwrap(),
                None,
                "the undo was stored"
            );

            host.undo_now(GRAPHSHELL).expect("session undo");
            assert!(
                host.graph()
                    .get_node_by_url("https://example.net/after")
                    .is_none(),
                "the session undo reaches Graphshell's change"
            );
        });
    }

    #[test]
    fn undo_keeps_a_save_another_channel_changed_since() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let mut host = open_on(&backend).await;
            let first = projection("First", None);
            let id = first.definition.id.clone();
            host.save_projection(&first).expect("save");
            let changed = projection("Changed elsewhere", None);
            host.through("turnstone".to_string(), |host| {
                let key = host
                    .graph()
                    .get_node_key_by_id(
                        host.graph()
                            .get_node_by_url(&projection_address(&id))
                            .unwrap()
                            .1
                            .id,
                    )
                    .unwrap();
                host.set_facet(
                    key,
                    PROJECTION_DEFINITION_FACET,
                    serde_json::to_value(&changed).unwrap(),
                )
                .expect("another channel edits the facet")
            });
            let reverted = host
                .undo_now(PROJECTION_EDITOR_VIA)
                .expect("undo save")
                .expect("a save to undo");
            assert!(!reverted.kept.is_empty(), "the changed facet is kept");
            assert!(
                reverted.kept.iter().any(|kept| kept
                    .by
                    .iter()
                    .any(|author| format!("{author:?}").contains("turnstone"))),
                "and its changer is named"
            );
            assert_eq!(host.saved_projection(&id).unwrap(), Some(changed));
            let summary = kept_summary(&reverted.kept).expect("a summary");
            assert!(
                summary.starts_with("kept ") && summary.contains("via turnstone"),
                "{summary}"
            );
        });
    }

    #[test]
    fn product_profiles_preserve_held_surface_truth_and_refuse_legacy_before_mutation() {
        let original = crate::mere_host::placement_test_graph();
        let product = ProductCodicilV3 {
            schema: PRODUCT_CODICIL_SCHEMA.into(),
            scope: TransferScope::SelectedSubgraph,
            exported_at_ms: 17,
            graph: original.to_snapshot(),
            facets: original.facets().clone(),
            scene: None,
        };
        let mut host = MereHost::empty(
            MemoryBackend::new(),
            selected_persona(),
            fixture_handlers(),
            AccessContext {
                persona: FIXTURE_PERSONA_ADDRESS.into(),
                device: FIXTURE_DEVICE_TWO_ADDRESS.into(),
                at_ms: 17,
            },
        );
        let bytes = |placement| {
            serde_json::to_vec(&ProfiledProductCodicil {
                product: product.clone(),
                placement,
            })
            .unwrap()
        };
        let recorded = bytes(Some(PlacementProfile::RecordedStrataV1));
        host.replace_with_product_codicil(&recorded).unwrap();
        assert_eq!(
            host.snapshot_placement(),
            Some(PlacementProfile::RecordedStrataV1)
        );
        assert_eq!(host.graph().to_snapshot().edges, product.graph.edges);
        assert_eq!(
            host.graph().to_snapshot().resources,
            product.graph.resources
        );
        assert_eq!(
            host.graph().to_snapshot().resource_edges,
            product.graph.resource_edges
        );
        let first = Uuid::from_u128(1);
        let export = host
            .export_product_codicil(ExportRequest {
                focused: first,
                selected: vec![first, Uuid::from_u128(2)],
                scope: TransferScope::SelectedSubgraph,
                exported_at_ms: 18,
                include_local_file_locations: false,
                scene: None,
            })
            .unwrap();
        let exported = decode_profiled_codicil(&export).unwrap();
        assert_eq!(exported.placement, Some(PlacementProfile::RecordedStrataV1));
        assert_eq!(exported.graph.edges, product.graph.edges);
        let journal = host.graph_session().journal().entries().len();
        host.import_product_codicil(&recorded).unwrap();
        assert_eq!(host.graph_session().journal().entries().len(), journal);

        let mut before = host.graph().to_snapshot();
        before.timestamp_secs = 0;
        let session = host.graph_session().id();
        let changes = host.graph_session().changes().len();
        let legacy = bytes(Some(PlacementProfile::LegacySurfaceV1));
        assert!(host.import_product_codicil(&legacy).is_err());
        assert!(host.replace_with_product_codicil(&legacy).is_err());
        let mut after = host.graph().to_snapshot();
        after.timestamp_secs = 0;
        assert_eq!(
            serde_json::to_value(after).unwrap(),
            serde_json::to_value(before).unwrap()
        );
        assert_eq!(host.graph_session().id(), session);
        assert_eq!(host.graph_session().journal().entries().len(), journal);
        assert_eq!(host.graph_session().changes().len(), changes);
        host.replace_with_product_codicil(&bytes(None)).unwrap();
        assert_eq!(host.snapshot_placement(), None);
        assert_eq!(
            decode_profiled_codicil(&bytes(None)).unwrap().placement,
            None
        );
        assert!(host.graph().to_snapshot().edges.iter().any(|edge| {
            edge.semantic.as_ref().is_some_and(|semantic| {
                semantic
                    .statements
                    .iter()
                    .any(|claim| claim.statement_id == "held-profile-handle")
            })
        }));
    }

    fn resource_selection_graph() -> (Graph, Vec<Uuid>, mere::kernel::persistence::PersistedEdge) {
        use mere::kernel::persistence::{
            PersistedEdge, PersistedEdgeFamily, PersistedResourceRecord, PersistedSemanticEdgeData,
            PersistedSemanticStatement, PersistedShownResource,
        };
        let mut graph = Graph::new();
        let surfaces: Vec<_> = [
            "https://selected.test/page",
            "https://selected.test/page#part",
            "https://outside.test/page",
        ]
        .into_iter()
        .map(|iri| {
            let key = add_node(
                &mut graph,
                Some(Uuid::new_v4()),
                iri.into(),
                PortablePoint::zero(),
            );
            graph.get_node(key).unwrap().id
        })
        .collect();
        let iris = [
            "https://selected.test/page",
            "https://outside.test/page",
            "https://vocab.test/#Selected",
            "https://vocab.test/#Outside",
        ];
        let ids: Vec<_> = iris
            .iter()
            .map(|iri| chartulary::resource_id_from_canonical_iri(iri).to_string())
            .collect();
        let mut snapshot = graph.to_snapshot();
        snapshot.resources = iris
            .iter()
            .map(|iri| PersistedResourceRecord {
                canonical_iri: (*iri).into(),
                facets: vec![],
            })
            .collect();
        snapshot.shown_resources = surfaces
            .iter()
            .enumerate()
            .map(|(index, surface)| PersistedShownResource {
                surface_id: surface.to_string(),
                resource_id: ids[usize::from(index == 2)].clone(),
            })
            .collect();
        let relation = |from: usize, to: usize, predicate: &str, handle: &str| PersistedEdge {
            from_node_id: ids[from].clone(),
            to_node_id: ids[to].clone(),
            families: vec![PersistedEdgeFamily::Semantic],
            semantic: Some(PersistedSemanticEdgeData {
                statements: vec![PersistedSemanticStatement {
                    statement_id: handle.into(),
                    predicate: predicate.into(),
                    recognized_sub_kind: None,
                    label: Some("retained relation".into()),
                    graph_scope: mere::kernel::types::GraphScope::Default,
                    provenance_iri: Some("https://tagger.test/".into()),
                    asserted_at_ms: Some(23),
                }],
                ..Default::default()
            }),
            traversal: None,
            containment: None,
            arrangement: None,
            imported: None,
            provenance: None,
        };
        snapshot.resource_edges = vec![
            relation(0, 2, TAGGED_WITH_IRI, "selected-tag-handle"),
            relation(1, 3, TAGGED_WITH_IRI, "outside-tag-handle"),
            relation(
                0,
                1,
                "https://schema.org/citation",
                "outside-content-handle",
            ),
        ];
        let graph = Graph::try_from_snapshot(&snapshot).unwrap();
        let exact_tag = graph
            .to_snapshot()
            .resource_edges
            .into_iter()
            .find(|edge| edge.to_node_id == ids[2])
            .unwrap();
        (graph, surfaces, exact_tag)
    }

    #[test]
    fn direct_selection_and_family_readers_lift_content_without_losing_surface_relations() {
        use mere::kernel::persistence::{PersistedEdgeFamily, PersistedTraversalEdgeData};
        let (mut graph, surfaces, _) = resource_selection_graph();
        let mut extras = Vec::new();
        for name in ["traversal", "layout", "unrelated"] {
            let key = add_node(
                &mut graph,
                Some(Uuid::new_v4()),
                format!("https://{name}.test/"),
                PortablePoint::zero(),
            );
            extras.push(graph.get_node(key).unwrap().id);
        }
        let mut snapshot = graph.to_snapshot();
        snapshot.edges.clear();
        let mut traversal = crate::mere_host::placement_test_graph()
            .to_snapshot()
            .edges
            .remove(0);
        traversal.from_node_id = surfaces[0].to_string();
        traversal.to_node_id = extras[0].to_string();
        traversal.families = vec![PersistedEdgeFamily::Traversal];
        traversal.semantic = None;
        traversal.traversal = Some(PersistedTraversalEdgeData::default());
        snapshot.edges.push(traversal.clone());
        let mut graph = Graph::try_from_recorded_snapshot(&snapshot).unwrap();
        let first = graph.get_node_key_by_id(surfaces[0]).unwrap();
        let layout = graph.get_node_key_by_id(extras[1]).unwrap();
        assert_relation(
            &mut graph,
            first,
            layout,
            EdgeAssertion::Arrangement {
                sub_kind: ArrangementSubKind::FrameMember,
            },
        );
        let request = ExportRequest {
            focused: surfaces[0],
            selected: vec![],
            scope: TransferScope::DirectRelations,
            exported_at_ms: 17,
            include_local_file_locations: false,
            scene: None,
        };
        let members = transfer_members(&graph, &request).unwrap();
        assert_eq!(
            members,
            HashSet::from([surfaces[0], surfaces[2], extras[0], extras[1]])
        );
        assert!(
            !members.contains(&surfaces[1]),
            "sharing a resource alone is not a direct link"
        );
        assert!(!members.contains(&extras[2]));
        let incoming = transfer_members(
            &graph,
            &ExportRequest {
                focused: surfaces[2],
                ..request
            },
        )
        .unwrap();
        assert_eq!(
            incoming,
            HashSet::from([surfaces[0], surfaces[1], surfaces[2]]),
            "both source aliases lift the directed content claim"
        );
        let mut host = MereHost::empty(
            MemoryBackend::new(),
            selected_persona(),
            fixture_handlers(),
            AccessContext {
                persona: FIXTURE_PERSONA_ADDRESS.into(),
                device: FIXTURE_DEVICE_TWO_ADDRESS.into(),
                at_ms: 17,
            },
        );
        host.begin_profiled_session(graph, Some(PlacementProfile::RecordedStrataV1))
            .unwrap();
        assert_eq!(
            host.matching_members("", RelationFamilyFilter::Traversal)
                .into_iter()
                .collect::<HashSet<_>>(),
            HashSet::from([surfaces[0], extras[0]])
        );
        assert_eq!(
            host.matching_members("", RelationFamilyFilter::Arrangement)
                .into_iter()
                .collect::<HashSet<_>>(),
            HashSet::from([surfaces[0], extras[1]])
        );
        let semantic = host
            .matching_members("", RelationFamilyFilter::Semantic)
            .into_iter()
            .collect::<HashSet<_>>();
        assert!(surfaces.iter().all(|id| semantic.contains(id)));
        assert!(!semantic.contains(&extras[2]));
        let exported = filtered_snapshot(host.graph(), &members, 17);
        assert!(exported.edges.contains(&traversal));
        assert!(
            exported
                .resource_edges
                .iter()
                .any(|edge| edge.semantic.as_ref().is_some_and(|bucket| bucket
                    .statements
                    .iter()
                    .any(|claim| claim.statement_id == "outside-content-handle")))
        );
    }

    #[test]
    fn product_tag_edits_keep_peer_assertions_and_search_shown_resource_labels() {
        let (graph, surfaces, _) = resource_selection_graph();
        let mut host = MereHost::empty(
            MemoryBackend::new(),
            selected_persona(),
            fixture_handlers(),
            AccessContext {
                persona: FIXTURE_PERSONA_ADDRESS.into(),
                device: FIXTURE_DEVICE_TWO_ADDRESS.into(),
                at_ms: 17,
            },
        );
        host.begin_profiled_session(graph, Some(PlacementProfile::RecordedStrataV1))
            .unwrap();
        let first = host.graph().get_node_key_by_id(surfaces[0]).unwrap();
        host.mutate_product_graph(|graph| {
            graph.write_as(Author::person("urn:mere:peer-test"), |graph| {
                for label in ["Shared", "PeerOnly"] {
                    apply_graph_delta(
                        graph,
                        GraphDelta::InsertNodeTag {
                            key: first,
                            tag: label.into(),
                        },
                    );
                }
            })
        });
        host.edit_node(
            surfaces[0],
            "tagged page",
            ["Shared".into(), "OwnOnly".into()],
        )
        .unwrap();
        let source = host.graph().shown_resource_id(first).unwrap();
        let shared: Vec<_> = host
            .graph()
            .resource_relations()
            .filter(|(_, from, to, _)| {
                *from == source
                    && host
                        .graph()
                        .resource_tag_concept(*to)
                        .is_some_and(|tag| tag.label == "Shared")
            })
            .flat_map(|(_, _, _, bucket)| bucket.semantic_statements())
            .collect();
        assert_eq!(
            shared.len(),
            2,
            "the selected Author asserts even when a peer already uses the label"
        );
        assert_ne!(shared[0].provenance_iri, shared[1].provenance_iri);
        let peer_claims = host
            .graph()
            .to_snapshot()
            .resource_edges
            .into_iter()
            .filter_map(|edge| edge.semantic)
            .flat_map(|bucket| bucket.statements)
            .filter(|claim| claim.provenance_iri.as_deref() == Some("urn:mere:peer-test"))
            .collect::<Vec<_>>();
        assert_eq!(peer_claims.len(), 2);
        host.edit_node(surfaces[0], "tagged page", ["Shared".into()])
            .unwrap();
        let actual = host.graph().to_snapshot().resource_edges;
        assert!(peer_claims.iter().all(|claim| actual.iter().any(|edge| {
            edge.semantic
                .as_ref()
                .is_some_and(|bucket| bucket.statements.contains(claim))
        })));
        assert!(
            !host
                .graph()
                .node_content_tags(first)
                .unwrap()
                .contains("OwnOnly")
        );
        assert_eq!(
            host.matching_members("PeerOnly", RelationFamilyFilter::All)
                .into_iter()
                .collect::<HashSet<_>>(),
            HashSet::from([surfaces[0], surfaces[1]])
        );
        assert!(
            host.matching_members("absent-label", RelationFamilyFilter::All)
                .is_empty()
        );
        let journal = host.graph_session().journal().entries().len();
        host.edit_node(surfaces[0], "tagged page", ["Shared".into()])
            .unwrap();
        assert_eq!(host.graph_session().journal().entries().len(), journal);
    }

    #[test]
    fn selection_keeps_shown_resources_and_tag_concepts_without_unselected_content() {
        let (graph, surfaces, exact_tag) = resource_selection_graph();
        let members = [surfaces[0], surfaces[1]].into_iter().collect();
        let snapshot = filtered_snapshot(&graph, &members, 17_000);
        assert_eq!(snapshot.nodes.len(), 2);
        assert_eq!(
            snapshot.shown_resources.len(),
            2,
            "same-resource surfaces stay distinct"
        );
        let iris: HashSet<_> = snapshot
            .resources
            .iter()
            .map(|record| record.canonical_iri.as_str())
            .collect();
        assert_eq!(
            iris,
            HashSet::from(["https://selected.test/page", "https://vocab.test/#Selected"])
        );
        assert_eq!(
            snapshot.resource_edges,
            vec![exact_tag],
            "tag handle, endpoints, time and attribution survive exactly"
        );
        assert!(Graph::try_from_snapshot(&snapshot).is_ok());
        let all = surfaces.into_iter().collect();
        let whole = filtered_snapshot(&graph, &all, 17_000);
        assert_eq!(
            whole.resources.len(),
            4,
            "both tag concepts survive when both pages are selected"
        );
        assert_eq!(
            whole.resource_edges.len(),
            3,
            "the content edge survives when both endpoints are selected"
        );
    }

    #[test]
    fn selection_keeps_statement_and_literal_declarations_to_a_fixed_point() {
        use mere::kernel::graph::predicate_declarations::{
            PredicateDeclaration, PredicateDeclarations,
        };
        use mere::kernel::graph::{Author, GraphStratum};
        use mere::kernel::persistence::{PersistedResourceFacet, PersistedResourceRecord};

        const SURFACE: &str = "https://vocab.test/predicates#surface";
        const RESOURCE: &str = "https://vocab.test/predicates#resource";
        const CHAIN: &str = "https://vocab.test/predicates#chain";
        const SURFACE_LITERAL: &str = "https://vocab.test/predicates#surface-literal";
        const RESOURCE_LITERAL: &str = "https://vocab.test/predicates#resource-literal";
        const OUTSIDE: &str = "https://vocab.test/predicates#outside";
        const UNRELATED: &str = "https://vocab.test/predicates#Surface";
        let id = |iri: &str| chartulary::resource_id_from_canonical_iri(iri).to_string();
        let (graph, surfaces, exact_tag) = resource_selection_graph();
        let mut source = graph.to_snapshot();
        for predicate in [
            SURFACE,
            RESOURCE,
            CHAIN,
            SURFACE_LITERAL,
            RESOURCE_LITERAL,
            OUTSIDE,
            UNRELATED,
        ] {
            let mut declarations = PredicateDeclarations {
                variants: vec![PredicateDeclaration {
                    declaration_id: format!("declaration:{predicate}"),
                    stratum: GraphStratum::Surface,
                    author: Author::person("persona-a").via("export-control"),
                    declared_at_ms: Some(29),
                }],
                selected: None,
            };
            if predicate == SURFACE {
                declarations.variants.push(PredicateDeclaration {
                    declaration_id: "conflicting-resource-declaration".into(),
                    stratum: GraphStratum::Resource,
                    author: Author::person("persona-b").via("export-control"),
                    declared_at_ms: Some(31),
                });
            }
            source.resources.push(PersistedResourceRecord {
                canonical_iri: predicate.into(),
                facets: vec![PersistedResourceFacet {
                    facet: PREDICATE_DECLARATIONS_FACET.into(),
                    value_json: serde_json::to_string(&declarations).unwrap(),
                }],
            });
        }
        let relation = |from: String, to: String, predicate: &str, handle: &str| {
            let mut edge = exact_tag.clone();
            edge.from_node_id = from;
            edge.to_node_id = to;
            let semantic = edge.semantic.as_mut().unwrap();
            semantic.predicate = Some(predicate.into());
            semantic.statements[0].predicate = predicate.into();
            semantic.statements[0].statement_id = handle.into();
            edge
        };
        source.edges = vec![
            relation(
                surfaces[0].to_string(),
                surfaces[1].to_string(),
                SURFACE,
                "selected-surface-claim",
            ),
            relation(
                surfaces[0].to_string(),
                surfaces[2].to_string(),
                OUTSIDE,
                "outside-surface-claim",
            ),
        ];
        let selected_tag = source
            .resource_edges
            .iter_mut()
            .find(|edge| edge.to_node_id == id("https://vocab.test/#Selected"))
            .unwrap();
        let mut resource_claim = selected_tag.semantic.as_ref().unwrap().statements[0].clone();
        resource_claim.predicate = RESOURCE.into();
        resource_claim.statement_id = "selected-resource-claim".into();
        selected_tag
            .semantic
            .as_mut()
            .unwrap()
            .statements
            .push(resource_claim);
        source.resource_edges.push(relation(
            id(SURFACE),
            id("https://vocab.test/#Selected"),
            CHAIN,
            "induced-declaration-claim",
        ));
        let mut resource_literal =
            NodeProperty::new(RESOURCE_LITERAL.into(), "resource value".into());
        resource_literal.statement_id = "resource-literal-handle".into();
        resource_literal.provenance_iri = Some("https://author.test/".into());
        resource_literal.asserted_at_ms = Some(37);
        source
            .resources
            .iter_mut()
            .find(|record| record.canonical_iri == "https://selected.test/page")
            .unwrap()
            .facets
            .push(PersistedResourceFacet {
                facet: SEMANTIC_PROPERTIES.into(),
                value_json: serde_json::to_string(&vec![resource_literal]).unwrap(),
            });
        let mut graph = Graph::try_from_snapshot(&source).unwrap();
        let mut surface_literal = NodeProperty::new(SURFACE_LITERAL.into(), "surface value".into());
        surface_literal.statement_id = "surface-literal-handle".into();
        surface_literal.provenance_iri = Some("https://author.test/".into());
        surface_literal.asserted_at_ms = Some(41);
        assert!(graph.append_node_properties(
            graph.get_node_key_by_id(surfaces[0]).unwrap(),
            vec![surface_literal],
        ));
        assert!(graph.append_node_properties(
            graph.get_node_key_by_id(surfaces[2]).unwrap(),
            vec![NodeProperty::new(UNRELATED.into(), "outside value".into())],
        ));
        let baseline = graph.to_snapshot();
        let members = [surfaces[0], surfaces[1]].into_iter().collect();
        let exported = filtered_snapshot(&graph, &members, 17_000);
        let wanted = HashSet::from([
            "https://selected.test/page",
            "https://vocab.test/#Selected",
            SURFACE,
            RESOURCE,
            CHAIN,
            SURFACE_LITERAL,
            RESOURCE_LITERAL,
        ]);
        let actual: HashSet<_> = exported
            .resources
            .iter()
            .map(|record| record.canonical_iri.as_str())
            .collect();
        assert_eq!(
            actual, wanted,
            "unselected and case-distinct declarations stay out"
        );
        for record in &exported.resources {
            assert_eq!(
                Some(record),
                baseline
                    .resources
                    .iter()
                    .find(|original| original.canonical_iri == record.canonical_iri),
                "declaration variants, origins and literal metadata survive unchanged"
            );
        }
        let expected_surface_edges: Vec<_> = baseline
            .edges
            .iter()
            .filter(|edge| {
                let selected = |id: &str| Uuid::parse_str(id).is_ok_and(|id| members.contains(&id));
                selected(&edge.from_node_id) && selected(&edge.to_node_id)
            })
            .cloned()
            .collect();
        assert_eq!(exported.edges, expected_surface_edges);
        let surface_claims: HashSet<_> = exported
            .edges
            .iter()
            .filter_map(|edge| edge.semantic.as_ref())
            .flat_map(|semantic| &semantic.statements)
            .map(|claim| claim.statement_id.as_str())
            .collect();
        assert!(surface_claims.contains("selected-surface-claim"));
        assert!(!surface_claims.contains("outside-surface-claim"));
        assert!(
            exported.edges.iter().any(|edge| edge.containment.is_some()),
            "selected same-host surfaces retain their derived containment"
        );
        let retained: HashSet<_> = wanted.into_iter().map(id).collect();
        let expected_edges: Vec<_> = baseline
            .resource_edges
            .iter()
            .filter(|edge| {
                retained.contains(&edge.from_node_id) && retained.contains(&edge.to_node_id)
            })
            .cloned()
            .collect();
        assert_eq!(exported.resource_edges, expected_edges);
        assert_eq!(
            exported.resource_edges.len(),
            2,
            "induced claims carry their declarations"
        );
        assert!(Graph::try_from_snapshot(&exported).is_ok());
        let property_facet = chartulary::FacetId::new(SEMANTIC_PROPERTIES);
        let exported_facets = filtered_facets(graph.facets(), &members, false);
        assert_eq!(
            exported_facets.get(&surfaces[0], &property_facet),
            graph.facets().get(&surfaces[0], &property_facet),
            "literal sidecar travels with its declaration"
        );
        assert!(exported_facets.get(&surfaces[2], &property_facet).is_none());
        let all = surfaces.into_iter().collect();
        let whole = filtered_snapshot(&graph, &all, 17_000);
        assert!(
            whole
                .resources
                .iter()
                .any(|record| record.canonical_iri == OUTSIDE)
        );
        assert!(
            whole
                .resources
                .iter()
                .any(|record| record.canonical_iri == UNRELATED)
        );
        assert_eq!(whole.resource_edges, baseline.resource_edges);
    }

    #[test]
    fn product_surface_sidecars_reject_resource_handle_collisions_before_edits() {
        let (graph, surfaces, edge) = resource_selection_graph();
        let held = edge.semantic.as_ref().unwrap().statements[0]
            .statement_id
            .clone();
        let mut product = ProductCodicilV3 {
            schema: PRODUCT_CODICIL_SCHEMA.into(),
            scope: TransferScope::SelectedSubgraph,
            exported_at_ms: 17,
            graph: graph.to_snapshot(),
            facets: graph.facets().clone(),
            scene: None,
        };
        let mut host = MereHost::empty(
            MemoryBackend::new(),
            selected_persona(),
            fixture_handlers(),
            AccessContext {
                persona: FIXTURE_PERSONA_ADDRESS.into(),
                device: FIXTURE_DEVICE_TWO_ADDRESS.into(),
                at_ms: 17,
            },
        );
        host.begin_profiled_session(graph.clone(), Some(PlacementProfile::RecordedStrataV1))
            .unwrap();
        let before = host.graph().to_snapshot();
        let facets = host.graph().facets().clone();
        let journal = host.graph_session().journal().entries().len();
        for collision in [false, true] {
            let mut property =
                mere::kernel::types::NodeProperty::new("urn:test:predicate".into(), "exact".into())
                    .with_metadata(Some("urn:test:source".into()), Some(42));
            property.statement_id = if collision {
                held.clone()
            } else {
                "surface-literal-handle".into()
            };
            product
                .facets
                .set(
                    surfaces[0],
                    chartulary::FacetId::new(SEMANTIC_PROPERTIES),
                    serde_json::to_value(vec![property.clone(), property]).unwrap(),
                    &chartulary::AcceptAll,
                )
                .unwrap();
            product
                .facets
                .set(
                    surfaces[0],
                    chartulary::FacetId::new("extension.origin-note"),
                    serde_json::json!({"statement_id": held}),
                    &chartulary::AcceptAll,
                )
                .unwrap();
            let bytes = serde_json::to_vec(&ProfiledProductCodicil {
                product: product.clone(),
                placement: Some(PlacementProfile::RecordedStrataV1),
            })
            .unwrap();
            assert_eq!(decode_profiled_codicil(&bytes).is_err(), collision);
            assert_eq!(
                profiled_codicil_graph(product.graph.clone(), product.facets.clone(), None)
                    .is_err(),
                collision
            );
            if collision {
                assert!(host.import_product_codicil(&bytes).is_err());
                assert!(host.replace_with_product_codicil(&bytes).is_err());
                let mut after = host.graph().to_snapshot();
                after.timestamp_secs = before.timestamp_secs;
                assert_eq!(
                    serde_json::to_value(after).unwrap(),
                    serde_json::to_value(&before).unwrap()
                );
                assert_eq!(host.graph().facets(), &facets);
                assert_eq!(host.graph_session().journal().entries().len(), journal);
                // This input is valid alone; its sidecar collides only with the live Resource.
                let mut surface_only = product.clone();
                surface_only.graph.resources.clear();
                surface_only.graph.resource_edges.clear();
                surface_only.graph.shown_resources.clear();
                let bytes = serde_json::to_vec(&ProfiledProductCodicil {
                    product: surface_only,
                    placement: Some(PlacementProfile::RecordedStrataV1),
                })
                .unwrap();
                assert!(decode_profiled_codicil(&bytes).is_ok());
                assert!(host.import_product_codicil(&bytes).is_err());
                assert_eq!(host.graph().facets(), &facets);
                assert_eq!(host.graph_session().journal().entries().len(), journal);
            } else {
                let restored = profiled_codicil_graph(
                    product.graph.clone(),
                    product.facets.clone(),
                    Some(PlacementProfile::RecordedStrataV1),
                )
                .unwrap();
                assert_eq!(
                    restored.to_snapshot().resource_edges,
                    product.graph.resource_edges
                );
                assert_eq!(
                    restored.facets().get(
                        &surfaces[0],
                        &chartulary::FacetId::new("extension.origin-note")
                    ),
                    Some(&serde_json::json!({"statement_id": held}))
                );
            }
        }
    }

    #[test]
    fn invalid_resource_codicils_fail_before_import_with_valid_control() {
        let (graph, surfaces, _) = resource_selection_graph();
        let mut codicil = ProductCodicilV3 {
            schema: PRODUCT_CODICIL_SCHEMA.into(),
            scope: TransferScope::SelectedSubgraph,
            exported_at_ms: 17_000,
            graph: filtered_snapshot(&graph, &surfaces.into_iter().collect(), 17_000),
            facets: NodeFacetStore::new(),
            scene: None,
        };
        let valid = serde_json::to_vec(&codicil).unwrap();
        assert!(decode_codicil(&valid).is_ok());
        codicil.graph.shown_resources[0].resource_id = Uuid::nil().to_string();
        let invalid = serde_json::to_vec(&codicil).unwrap();
        assert!(matches!(
            decode_codicil(&invalid),
            Err(ProductError::InvalidCodicil(_))
        ));
        let mut host = MereHost::empty(
            MemoryBackend::new(),
            selected_persona(),
            fixture_handlers(),
            AccessContext {
                persona: FIXTURE_PERSONA_ADDRESS.into(),
                device: FIXTURE_DEVICE_TWO_ADDRESS.into(),
                at_ms: 100,
            },
        );
        assert!(matches!(
            host.import_product_codicil(&invalid),
            Err(ProductError::InvalidCodicil(_))
        ));
        assert_eq!(
            host.graph().node_count(),
            0,
            "failed import leaves live truth unchanged"
        );
        assert!(host.graph().resource_nodes().next().is_none());
        let decoded = decode_codicil(&valid).unwrap();
        assert!(codicil_graph(decoded.graph, decoded.facets).is_ok());
    }

    #[test]
    fn resource_codicil_import_is_one_exact_journaled_union_and_new_session_preserves_it() {
        let (graph, surfaces, _) = resource_selection_graph();
        let mut codicil = ProductCodicilV3 {
            schema: PRODUCT_CODICIL_SCHEMA.into(),
            scope: TransferScope::SelectedSubgraph,
            exported_at_ms: 17_000,
            graph: filtered_snapshot(&graph, &surfaces.into_iter().collect(), 17_000),
            facets: NodeFacetStore::new(),
            scene: None,
        };
        let bytes = serde_json::to_vec(&codicil).unwrap();
        let backend = MemoryBackend::new();
        let mut host = MereHost::empty(
            backend.clone(),
            selected_persona(),
            fixture_handlers(),
            AccessContext {
                persona: FIXTURE_PERSONA_ADDRESS.into(),
                device: FIXTURE_DEVICE_TWO_ADDRESS.into(),
                at_ms: 100,
            },
        );
        let existing = host
            .create_address("https://selected.test/page#existing", "Existing")
            .unwrap();
        let mut before = host.graph().to_snapshot();
        before.timestamp_secs = 0;
        let changes_before = host.graph_session().changes().len();
        let journal_before = host.graph_session().journal().entries().len();
        let receipt = host.import_product_codicil(&bytes).unwrap();
        assert_eq!(receipt.nodes, codicil.graph.nodes.len());
        assert_eq!(host.graph().node_count(), codicil.graph.nodes.len() + 1);
        assert!(host.graph().get_node_by_id(existing).is_some());
        let after = host.graph().to_snapshot();
        assert_eq!(after.resources.len(), codicil.graph.resources.len());
        for record in &codicil.graph.resources {
            assert!(after.resources.contains(record));
        }
        assert_eq!(after.resource_edges, codicil.graph.resource_edges);
        for shown in &codicil.graph.shown_resources {
            assert!(after.shown_resources.contains(shown));
        }
        assert_eq!(host.graph_session().changes().len(), changes_before + 1);
        let entries = &host.graph_session().journal().entries()[journal_before..];
        assert!(entries.iter().any(|entry| matches!(
            entry.delta,
            CapturedDelta::ReplaySetResourceRecordById { .. }
        )));
        assert!(entries.iter().any(|entry| matches!(
            entry.delta,
            CapturedDelta::ReplaySetResourceEdgesByIds { .. }
        )));
        assert!(entries.iter().any(|entry| matches!(
            entry.delta,
            CapturedDelta::ReplaySetShownResourceById { .. }
        )));
        assert!(
            entries.iter().all(
                |entry| entry.author == Author::person(FIXTURE_PERSONA_ADDRESS).via(GRAPHSHELL)
            )
        );
        pollster::block_on(async {
            host.persist(17).await.unwrap();
            let mut reopened = MereHost::open(
                backend,
                selected_persona(),
                fixture_handlers(),
                AccessContext {
                    persona: FIXTURE_PERSONA_ADDRESS.into(),
                    device: FIXTURE_DEVICE_TWO_ADDRESS.into(),
                    at_ms: 100,
                },
            )
            .await
            .unwrap();
            let loaded = reopened.graph().to_snapshot();
            assert_eq!(loaded.resources, after.resources);
            assert_eq!(loaded.resource_edges, after.resource_edges);
            assert_eq!(loaded.shown_resources, after.shown_resources);
            assert!(reopened.undo(GRAPHSHELL).await.unwrap().is_some());
            let mut undone = reopened.graph().to_snapshot();
            undone.timestamp_secs = 0;
            assert_eq!(
                serde_json::to_value(undone).unwrap(),
                serde_json::to_value(before).unwrap()
            );
        });

        host.replace_with_product_codicil(&bytes).unwrap();
        let opened = host.graph().to_snapshot();
        assert_eq!(opened.resources, codicil.graph.resources);
        assert_eq!(opened.resource_edges, codicil.graph.resource_edges);
        assert_eq!(opened.shown_resources, codicil.graph.shown_resources);

        codicil.graph.resources.clear();
        codicil.graph.resource_edges.clear();
        codicil.graph.shown_resources.clear();
        let legacy = serde_json::to_vec(&codicil).unwrap();
        let mut legacy_host = MereHost::empty(
            MemoryBackend::new(),
            selected_persona(),
            fixture_handlers(),
            AccessContext {
                persona: FIXTURE_PERSONA_ADDRESS.into(),
                device: FIXTURE_DEVICE_TWO_ADDRESS.into(),
                at_ms: 100,
            },
        );
        let receipt = legacy_host.import_product_codicil(&legacy).unwrap();
        assert_eq!(receipt.nodes, codicil.graph.nodes.len());
        assert_eq!(legacy_host.graph().node_count(), codicil.graph.nodes.len());
    }

    #[test]
    fn product_import_preserves_distinct_handles_and_rejects_collisions_atomically() {
        use mere::kernel::graph::GraphStratum;
        use mere::kernel::graph::predicate_declarations::{
            PredicateDeclaration, PredicateDeclarations,
        };
        use mere::kernel::persistence::{PersistedResourceFacet, PersistedResourceRecord};
        const IMPORT_PREDICATE: &str = "https://vocab.test/import#custom";
        let (graph, surfaces, tag) = resource_selection_graph();
        let mut codicil = ProductCodicilV3 {
            schema: PRODUCT_CODICIL_SCHEMA.into(),
            scope: TransferScope::SelectedSubgraph,
            exported_at_ms: 17_000,
            graph: filtered_snapshot(&graph, &surfaces[..2].iter().copied().collect(), 17_000),
            facets: NodeFacetStore::new(),
            scene: None,
        };
        let mut surface = tag;
        codicil.graph.edges.clear();
        surface.from_node_id = surfaces[0].to_string();
        surface.to_node_id = surfaces[1].to_string();
        surface.semantic.as_mut().unwrap().predicate = Some(IMPORT_PREDICATE.into());
        surface.semantic.as_mut().unwrap().statements[0].predicate = IMPORT_PREDICATE.into();
        surface.semantic.as_mut().unwrap().statements[0].statement_id = "surface-alice".into();
        codicil.graph.edges.push(surface.clone());
        let mut parallel = surface.clone();
        parallel.semantic.as_mut().unwrap().statements[0].statement_id = "surface-bob".into();
        codicil.graph.edges.push(parallel);
        let declarations = PredicateDeclarations {
            variants: vec![PredicateDeclaration {
                declaration_id: "import-declaration".into(),
                stratum: GraphStratum::Surface,
                author: Author::person("source-persona"),
                declared_at_ms: Some(31),
            }],
            selected: Some("import-declaration".into()),
        };
        codicil.graph.resources.push(PersistedResourceRecord {
            canonical_iri: IMPORT_PREDICATE.into(),
            facets: vec![PersistedResourceFacet {
                facet: PREDICATE_DECLARATIONS_FACET.into(),
                value_json: serde_json::to_string(&declarations).unwrap(),
            }],
        });
        let mut host = MereHost::empty(
            MemoryBackend::new(),
            selected_persona(),
            fixture_handlers(),
            AccessContext {
                persona: FIXTURE_PERSONA_ADDRESS.into(),
                device: FIXTURE_DEVICE_TWO_ADDRESS.into(),
                at_ms: 100,
            },
        );
        let bytes = serde_json::to_vec(&codicil).unwrap();
        host.import_product_codicil(&bytes).unwrap();
        let imported = host.graph().to_snapshot();
        let record = imported
            .resources
            .iter()
            .find(|record| record.canonical_iri == IMPORT_PREDICATE)
            .unwrap();
        let resource_id = chartulary::resource_id_from_canonical_iri(IMPORT_PREDICATE);
        let resource = host.graph().resource(resource_id).unwrap();
        assert_eq!(resource.id(), resource_id);
        assert_eq!(resource.canonical_iri(), IMPORT_PREDICATE);
        assert_eq!(
            mere::kernel::graph::predicate_declarations::predicate_declarations_from_record(record)
                .unwrap(),
            Some(declarations),
            "all declaration variants, authors, times and selection survive"
        );
        let facet_values = |record: &PersistedResourceRecord| {
            record
                .facets
                .iter()
                .map(|facet| {
                    (
                        facet.facet.clone(),
                        serde_json::from_str::<serde_json::Value>(&facet.value_json).unwrap(),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        };
        assert_eq!(
            facet_values(record),
            facet_values(codicil.graph.resources.last().unwrap()),
            "facet keys and every value survive JSON normalization"
        );
        let claims = host
            .graph()
            .to_snapshot()
            .edges
            .into_iter()
            .filter_map(|edge| edge.semantic)
            .flat_map(|semantic| semantic.statements)
            .map(|claim| claim.statement_id)
            .collect::<HashSet<_>>();
        assert!(claims.contains("surface-alice") && claims.contains("surface-bob"));
        let journal = host.graph_session().journal().entries().len();
        let changes = host.graph_session().changes().len();
        host.import_product_codicil(&bytes).unwrap();
        assert_eq!(
            host.graph_session().journal().entries().len(),
            journal,
            "identical carried records add no journal edits"
        );
        assert_eq!(host.graph_session().changes().len(), changes);

        let mut payload_collision = codicil.clone();
        payload_collision.graph.edges[0]
            .semantic
            .as_mut()
            .unwrap()
            .statements[0]
            .label = Some("divergent".into());
        let mut pair_collision = codicil.clone();
        pair_collision.graph.edges[0].to_node_id = surfaces[0].to_string();
        let mut store_collision = codicil.clone();
        store_collision.graph.edges[0]
            .semantic
            .as_mut()
            .unwrap()
            .statements[0]
            .statement_id = store_collision.graph.resource_edges[0]
            .semantic
            .as_ref()
            .unwrap()
            .statements[0]
            .statement_id
            .clone();
        store_collision.graph.resource_edges.clear();
        let mut source_collision = codicil.clone();
        let mut duplicate = surface;
        duplicate.semantic.as_mut().unwrap().statements[0].label =
            Some("raw source divergence".into());
        source_collision.graph.edges.push(duplicate);
        let mut invalid_declaration = codicil.clone();
        invalid_declaration
            .graph
            .resources
            .last_mut()
            .unwrap()
            .facets[0]
            .value_json = "false".into();
        let mut fixed_declaration = codicil.clone();
        fixed_declaration
            .graph
            .resources
            .last_mut()
            .unwrap()
            .canonical_iri = TAGGED_WITH_IRI.into();
        let mut before = host.graph().to_snapshot();
        before.timestamp_secs = 0;
        for invalid in [
            payload_collision,
            pair_collision,
            store_collision,
            source_collision,
            invalid_declaration,
            fixed_declaration,
        ] {
            assert!(matches!(
                host.import_product_codicil(&serde_json::to_vec(&invalid).unwrap()),
                Err(ProductError::InvalidCodicil(_))
            ));
            let mut after = host.graph().to_snapshot();
            after.timestamp_secs = 0;
            assert_eq!(
                serde_json::to_value(after).unwrap(),
                serde_json::to_value(&before).unwrap()
            );
            assert_eq!(host.graph_session().journal().entries().len(), journal);
            assert_eq!(host.graph_session().changes().len(), changes);
        }

        let mut legacy = codicil.clone();
        legacy.graph.edges[0].semantic.as_mut().unwrap().statements[0].provenance_iri = None;
        let mut legacy_host = MereHost::empty(
            MemoryBackend::new(),
            selected_persona(),
            fixture_handlers(),
            AccessContext {
                persona: FIXTURE_PERSONA_ADDRESS.into(),
                device: FIXTURE_DEVICE_TWO_ADDRESS.into(),
                at_ms: 100,
            },
        );
        let legacy_bytes = serde_json::to_vec(&legacy).unwrap();
        legacy_host.import_product_codicil(&legacy_bytes).unwrap();
        let normalized = legacy_host.graph().to_snapshot();
        let legacy_claim = normalized
            .edges
            .iter()
            .filter_map(|edge| edge.semantic.as_ref())
            .flat_map(|semantic| &semantic.statements)
            .find(|claim| claim.statement_id == "surface-alice")
            .unwrap();
        assert_eq!(
            legacy_claim.provenance_iri.as_deref(),
            Some(mere::kernel::graph::edge_data::UNKNOWN_LEGACY_ASSERTER_IRI)
        );
        assert_eq!(legacy_claim.asserted_at_ms, Some(23));
        let known_claim = normalized
            .edges
            .iter()
            .filter_map(|edge| edge.semantic.as_ref())
            .flat_map(|semantic| &semantic.statements)
            .find(|claim| claim.statement_id == "surface-bob")
            .unwrap();
        assert_eq!(
            known_claim.provenance_iri.as_deref(),
            Some("https://tagger.test/")
        );
        let legacy_journal = legacy_host.graph_session().journal().entries().len();
        legacy_host.import_product_codicil(&legacy_bytes).unwrap();
        assert_eq!(
            legacy_host.graph_session().journal().entries().len(),
            legacy_journal,
            "missing-source legacy reimport equals its normalized marker"
        );
        legacy.graph.edges[0].semantic.as_mut().unwrap().statements[0].label =
            Some("different legacy claim".into());
        assert!(
            legacy_host
                .import_product_codicil(&serde_json::to_vec(&legacy).unwrap())
                .is_err()
        );
        assert_eq!(
            legacy_host.graph_session().journal().entries().len(),
            legacy_journal
        );
    }

    /// A scene saved before the physics catalog carries no law, overlays or kind
    /// source; it must still open, as Springs with nothing composed on. And the
    /// three fields round-trip by id once present. (Physics catalog — P1.)
    #[test]
    fn saved_scene_physics_fields_default_and_round_trip() {
        let legacy = serde_json::json!({
            "name": "Before the catalog",
            "selected": [],
            "layout_strategy": "grid.default",
            "physics_paused": true,
            "physics_damping": 0.7,
            "arrangement_pull": 0.4,
            "camera_offset": [0.0, 0.0],
            "camera_zoom": 1.0,
            "default_handler": "system.default",
            "cartography": CartographyGeometry::default(),
        });
        let scene: SavedSceneV2 = serde_json::from_value(legacy).expect("a legacy scene opens");
        assert_eq!(scene.physics_law, mere::canvas::PhysicsLaw::Springs.id());
        assert!(scene.physics_overlays.is_empty());
        assert_eq!(
            scene.physics_kind_source,
            mere::canvas::PhysicsKindSource::Site.id()
        );
        assert_eq!(
            scene.physics_group_source,
            mere::canvas::PhysicsKindSource::Site.id(),
            "a scene saved before the groups channel pulls by site, as it did"
        );
        assert_eq!(
            scene.physics_mass_source,
            mere::canvas::PhysicsMassSource::Degree.id()
        );
        assert_eq!(
            scene.physics_depth_source,
            mere::canvas::PhysicsDepthSource::Roots.id()
        );

        let chosen = SavedSceneV2 {
            physics_law: mere::canvas::PhysicsLaw::Kinds.id().to_string(),
            physics_overlays: vec![
                mere::canvas::PhysicsOverlay::Tide.id().to_string(),
                mere::canvas::PhysicsOverlay::GridSnap.id().to_string(),
            ],
            physics_kind_source: mere::canvas::PhysicsKindSource::Cluster.id().to_string(),
            physics_group_source: mere::canvas::PhysicsKindSource::Meaning.id().to_string(),
            physics_mass_source: mere::canvas::PhysicsMassSource::PageRank.id().to_string(),
            physics_depth_source: mere::canvas::PhysicsDepthSource::Focus.id().to_string(),
            ..scene
        };
        let json = serde_json::to_string(&chosen).expect("encodes");
        let back: SavedSceneV2 = serde_json::from_str(&json).expect("decodes");
        assert_eq!(back, chosen);
        assert_eq!(
            mere::canvas::PhysicsLaw::parse(&back.physics_law),
            Some(mere::canvas::PhysicsLaw::Kinds)
        );
        assert_eq!(
            mere::canvas::PhysicsKindSource::parse(&back.physics_group_source),
            Some(mere::canvas::PhysicsKindSource::Meaning)
        );
    }

    /// G7: a scene saved before the roles has only its anchor pull, and opens
    /// as it behaved: a positive pull is every item anchored at that pull, zero
    /// is seeded. A new scene round-trips all three roles at all three scopes,
    /// and an unknown role id fails rather than falling back.
    #[test]
    fn saved_scene_roles_read_old_pulls_as_they_behaved_and_round_trip() {
        use mere::canvas::Role;
        let legacy = |pull: f32| {
            serde_json::json!({
                "name": "Before the roles",
                "selected": [],
                "layout_strategy": "phyllotaxis.default",
                "physics_paused": false,
                "physics_damping": 0.7,
                "arrangement_pull": pull,
                "camera_offset": [0.0, 0.0],
                "camera_zoom": 1.0,
                "default_handler": "system.default",
                "cartography": CartographyGeometry::default(),
            })
        };
        let old: SavedSceneV2 = serde_json::from_value(legacy(12.0)).expect("opens");
        assert_eq!(old.arrangement_roles, None);
        assert_eq!(
            old.roles().unwrap(),
            (SavedRoles::uniform(Role::Anchored), 12.0)
        );
        let fixture: SavedSceneV2 = serde_json::from_value(legacy(0.4)).unwrap();
        assert_eq!(
            fixture.roles().unwrap(),
            (SavedRoles::uniform(Role::Anchored), 0.4),
            "anchored at its own pull"
        );
        let unpulled: SavedSceneV2 = serde_json::from_value(legacy(0.0)).unwrap();
        assert_eq!(
            unpulled.roles().unwrap(),
            (
                SavedRoles::uniform(Role::Seeded),
                mere::canvas::DEFAULT_ANCHOR_STIFFNESS
            ),
            "no pull was the seed-only reading"
        );
        let resaved = serde_json::to_value(&old).unwrap();
        assert!(
            resaved.get("arrangement_roles").is_none(),
            "an old scene re-saved unchanged stays an old scene"
        );

        let member = Uuid::from_u128(7);
        let mut roles = SavedRoles::uniform(Role::Pinned);
        roles.groups.insert("example.test".into(), Role::Anchored);
        roles.items.insert(member, Role::Seeded);
        let new = SavedSceneV2 {
            arrangement_pull: 3.0,
            arrangement_roles: Some(roles.saved()),
            ..old.clone()
        };
        let json = serde_json::to_string(&new).unwrap();
        let back: SavedSceneV2 = serde_json::from_str(&json).unwrap();
        assert_eq!(back, new);
        assert_eq!(serde_json::to_string(&back).unwrap(), json, "byte for byte");
        assert_eq!(back.roles().unwrap(), (roles, 3.0));

        let unknown = SavedSceneV2 {
            arrangement_roles: Some(SavedRolesV1 {
                default: "tethered".into(),
                ..SavedRolesV1::default()
            }),
            ..old
        };
        assert_eq!(
            unknown.roles().unwrap_err(),
            "unknown arrangement role tethered"
        );
    }

    #[test]
    fn projection_clock_excludes_paused_host_time_and_replays_deterministically() {
        fn run() -> Vec<f32> {
            let mut clock = ProjectionClock::default();
            let mut receipt = vec![clock.observe(100.0), clock.observe(140.0)];
            clock.set_paused(true);
            receipt.push(clock.observe(240.0));
            receipt.push(clock.observe(300.0));
            clock.set_paused(false);
            receipt.push(clock.observe(340.0));
            receipt
        }

        assert_eq!(run(), vec![0.0, 40.0, 40.0, 40.0, 80.0]);
        assert_eq!(run(), run(), "identical host timestamps replay identically");
    }

    #[test]
    fn h3_mixed_graph_scene_and_selected_codicil_round_trip() {
        let mut host =
            MereHost::fixture(MemoryBackend::new(), selected_persona(), fixture_handlers())
                .expect("fixture");
        let file = host
            .create_file_metadata(LocalFileMetadata {
                content_hash: "ab".repeat(32),
                name: "radio-plan.unknown".to_string(),
                media_type: "application/x-unknown".to_string(),
                byte_len: 413,
                last_modified_ms: 42,
            })
            .expect("file");
        host.edit_node(
            file,
            "Radio plan",
            ["transport".to_string(), "unknown-file".to_string()],
        )
        .expect("edit");
        host.set_product_facet(
            file,
            "example.notes/v1",
            r#"{"status":"inspect-metadata-only"}"#,
        )
        .expect("facet");
        let web = host
            .graph()
            .get_node_by_url(FIXTURE_WEB_ADDRESS)
            .unwrap()
            .1
            .id;
        host.assert_product_relation(file, web, EditableRelation::Cites)
            .expect("relation");

        let receipt = host
            .graph()
            .get_node_by_url(FIXTURE_RECEIPT_ADDRESS)
            .unwrap()
            .1
            .id;
        let grant = host
            .graph()
            .get_node_by_url(FIXTURE_GRANT_ADDRESS)
            .unwrap()
            .1
            .id;
        let selected = vec![file, web, receipt, grant];
        let geometry = CartographyGeometry::from_positions([
            (file, (10.0, 20.0)),
            (web, (30.0, 40.0)),
            (receipt, (50.0, 60.0)),
            (grant, (70.0, 80.0)),
        ])
        .with_sprites([(file, "data:image/png;base64,AA==".to_string())])
        .with_faces([(file, "sprite".to_string()), (web, "bare".to_string())]);
        // Written flat, as a version-3 scene, and converted (F142).
        let flat = SavedSceneV2 {
            name: "Transport research".to_string(),
            selected: selected.clone(),
            layout_strategy: Some("grid.default".to_string()),
            physics_paused: true,
            physics_damping: 0.7,
            physics_law: "stress.kamada-kawai".to_string(),
            physics_overlays: vec!["grid-snap".to_string()],
            physics_kind_source: "site".to_string(),
            physics_group_source: "site".to_string(),
            physics_mass_source: "pagerank".to_string(),
            physics_depth_source: "layers".to_string(),
            arrangement_pull: 0.4,
            arrangement_roles: None,
            dynamics: None,
            camera_offset: (123.0, 234.0),
            camera_zoom: 1.2,
            default_handler: "system.default".to_string(),
            cartography: geometry,
        };
        let scene = flat.clone().into_v3().expect("converts");
        host.save_product_scene("mere://scene/h3-test", &scene)
            .expect("save scene");
        assert_eq!(
            host.product_scene("mere://scene/h3-test")
                .expect("open scene"),
            scene
        );
        // F44, F142: each version lives under its own facet, so a reader of
        // an older one does not see a newer scene; a scene saved before the
        // roles, under the old facet, opens and reads as it behaved.
        assert!(
            host.facet_value("mere://scene/h3-test", SAVED_SCENE_FACET_V2)
                .is_none()
                && host
                    .facet_value("mere://scene/h3-test", SAVED_SCENE_FACET_V3)
                    .is_none()
        );
        let old = host
            .create_address("mere://scene/before-roles", "Before the roles")
            .unwrap();
        let old_key = host.graph().get_node_key_by_id(old).unwrap();
        let mut v1 = serde_json::to_value(&flat).unwrap();
        v1.as_object_mut().unwrap().remove("arrangement_roles");
        v1["arrangement_pull"] = serde_json::json!(12.0);
        host.set_facet(old_key, SAVED_SCENE_FACET_V2, v1).unwrap();
        let reopened = host.product_scene("mere://scene/before-roles").unwrap();
        let target = reopened.dynamics_spec().unwrap().target.unwrap();
        assert_eq!(
            (target.default_role, target.anchored_pull, target.groups),
            (mere::canvas::Role::Anchored, 12.0, None),
            "anchored at its old pull"
        );
        assert_eq!(
            host.matching_members("unknown-file", RelationFamilyFilter::Semantic),
            vec![file]
        );

        let bytes = host
            .export_product_codicil(ExportRequest {
                focused: file,
                selected: selected.clone(),
                scope: TransferScope::SelectedSubgraph,
                exported_at_ms: 1_700_000_000_000,
                include_local_file_locations: false,
                scene: Some(scene.clone()),
            })
            .expect("export");
        let json = String::from_utf8(bytes.clone()).unwrap();
        assert!(!json.contains(LOCAL_FILE_FACET));
        assert!(json.contains(CONTENT_FACET));

        let mut legacy: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        legacy["schema"] = serde_json::Value::String(LEGACY_PRODUCT_ENGRAM_SCHEMA.to_string());
        // An engram never carried a version-4 scene; its own scenes convert
        // (`a_version_2_codicil_converts_and_an_export_narrows_the_target`).
        legacy.as_object_mut().unwrap().remove("scene");
        let legacy_bytes = serde_json::to_vec(&legacy).unwrap();
        assert_eq!(
            decode_codicil(&legacy_bytes).unwrap().schema,
            LEGACY_PRODUCT_ENGRAM_SCHEMA,
            "the v1 graph-engram tag remains readable"
        );

        let mut reopened = MereHost::empty(
            MemoryBackend::new(),
            selected_persona(),
            fixture_handlers(),
            AccessContext {
                persona: FIXTURE_PERSONA_ADDRESS.to_string(),
                device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                at_ms: 100,
            },
        );
        let imported = reopened.import_product_codicil(&bytes).expect("import");
        assert_eq!(imported.nodes, 4);
        assert_eq!(reopened.graph().node_count(), 4);
        let change = reopened
            .graph_session()
            .changes()
            .last()
            .expect("the import's change");
        assert_eq!(
            change.author,
            Author::person(FIXTURE_PERSONA_ADDRESS).via(GRAPHSHELL),
            "an import is Graphshell's edit, by the selected persona"
        );
        assert!(
            change.end > change.first,
            "the import is one journaled change"
        );
        for id in selected {
            assert!(
                reopened.graph().get_node_by_id(id).is_some(),
                "{id} survived"
            );
        }
        let portable_file = Sha256NamedInformation::from_hex(&"ab".repeat(32))
            .unwrap()
            .to_string();
        let content = reopened.facet_value(&portable_file, CONTENT_FACET).unwrap();
        assert_eq!(
            content["portable_id"].as_str(),
            Some(portable_file.as_str())
        );
        assert!(
            content.get("sha256").is_none(),
            "new content facets do not repeat the NI digest as private hex",
        );
        assert_eq!(content["byte_len"], 413);
        let file_key = reopened.graph().get_node_key_by_id(file).unwrap();
        let web_key = reopened.graph().get_node_key_by_id(web).unwrap();
        let (handle, payload) = reopened
            .graph()
            .projected_relations_between(file_key, web_key)
            .next()
            .unwrap();
        assert!(matches!(
            handle,
            mere::kernel::graph::RelationKey::Resource(_)
        ));
        assert!(
            payload.has_relation(mere::kernel::graph::RelationSelector::Semantic(
                SemanticSubKind::Cites
            ))
        );
        assert!(reopened.graph().find_edge_key(file_key, web_key).is_none());
        assert!(reopened.graph().projected_relations().any(|(_, relation)| {
            relation.kind == RelationKind::Provenance(ProvenanceSubKind::GeneratedFrom)
        }));
        let normalized_edges = |edges: &[mere::kernel::persistence::PersistedEdge]| {
            let mut edges: Vec<_> = edges
                .iter()
                .map(|edge| serde_json::to_string(edge).unwrap())
                .collect();
            edges.sort();
            edges
        };
        assert_eq!(
            normalized_edges(&reopened.graph().to_snapshot().resource_edges),
            normalized_edges(&decode_codicil(&bytes).unwrap().graph.resource_edges),
            "selected Resource claim handles, sources, scopes and metadata survive import"
        );

        let replaced = reopened.graph_session().id();
        let (_, imported_scene) = reopened
            .replace_with_product_codicil(&bytes)
            .expect("open as graph");
        assert_ne!(
            reopened.graph_session().id(),
            replaced,
            "opening a codicil begins a new session"
        );
        assert_eq!(reopened.graph().node_count(), 4);
        let imported_scene = imported_scene.expect("scene");
        assert_eq!(imported_scene.selected.len(), 4);
        assert_eq!(imported_scene.cartography.sprite_iter().count(), 1);
        assert_eq!(imported_scene.cartography.face_iter().count(), 2);
    }

    /// The dynamics spec on the scene: G4a's carrier (F97–F110) and G4b1's
    /// version 4, with the conversion of earlier scenes (F142, F143, F149,
    /// F151, F153, F154, F159).
    mod dynamics {
        use std::collections::BTreeMap;

        use mere::canvas::dynamics_spec::{
            Bar, DynamicsSpec, GroupRoles, Node, Observable, Realization, Stage, Stop, Target,
        };
        use mere::canvas::{PhysicsLaw, PhysicsOverlay, Role};

        use super::*;

        /// A scene carrying `spec`.
        fn scene(spec: &DynamicsSpec) -> SavedSceneV3 {
            SavedSceneV3 {
                name: "Spec scene".to_string(),
                selected: Vec::new(),
                physics_paused: true,
                dynamics: SavedDynamics::from_spec(spec),
                camera_offset: (0.0, 0.0),
                camera_zoom: 1.0,
                default_handler: "system.default".to_string(),
                cartography: CartographyGeometry::default(),
            }
        }

        /// A version-3 scene with no spec and every flat field at its default.
        fn legacy() -> serde_json::Value {
            serde_json::json!({
                "name": "Flat scene",
                "selected": [],
                "layout_strategy": "grid.default",
                "physics_paused": true,
                "physics_damping": 0.5,
                "camera_offset": [0.0, 0.0],
                "camera_zoom": 1.0,
                "default_handler": "system.default",
                "cartography": CartographyGeometry::default(),
            })
        }

        fn law(law: PhysicsLaw) -> Node {
            Node::preset(law.id())
        }

        fn at(law: PhysicsLaw, weight: f64) -> Node {
            Node::Preset {
                id: law.id().into(),
                weight,
                terms: BTreeMap::new(),
                rung: None,
                overlays: Vec::new(),
            }
        }

        /// A spec the canvas runs (R1–R4), using every root field.
        fn runnable(member: Uuid) -> DynamicsSpec {
            let mut first = law(PhysicsLaw::Springs);
            first
                .overlays_mut()
                .push(Node::preset(PhysicsOverlay::GravityLocus.id()));
            let mix = Node::Mix {
                parts: vec![at(PhysicsLaw::Springs, 0.5), at(PhysicsLaw::Energy, 2.0)],
                weight: 1.0,
                overlays: Vec::new(),
            };
            let between = Node::Grouped {
                partition: "groups.meaning".into(),
                outer: Box::new(at(PhysicsLaw::Charge, 16.0)),
                inner: Box::new(law(PhysicsLaw::Springs)),
                weight: 1.0,
                overlays: Vec::new(),
            };
            let stage = |node, stop, capture| Stage {
                node,
                stop,
                capture,
            };
            let mut spec = DynamicsSpec::new(Node::Schedule {
                stages: vec![
                    stage(first, Stop::Frames(240), Some(Role::Anchored)),
                    stage(mix, Stop::Rest, None),
                    stage(between, Stop::LawDone, None),
                    stage(law(PhysicsLaw::Still), Stop::Rest, None),
                ],
                weight: 1.0,
                overlays: Vec::new(),
            });
            spec.seed = 7;
            spec.channels.insert("mass".into(), "mass.pagerank".into());
            spec.realization = Realization::Integrate { damping: Some(0.7) };
            spec.target = Some(Target {
                arrangement: "grid.default".into(),
                anchored_pull: 12.0,
                default_role: Role::Seeded,
                groups: Some(GroupRoles {
                    channel: "groups.site".into(),
                    roles: BTreeMap::from([("example.test".to_string(), Role::Anchored)]),
                }),
                items: BTreeMap::from([(member.to_string(), Role::Pinned)]),
            });
            spec.bars = vec![Bar {
                observable: Observable::Overlaps,
                at_least: None,
                at_most: Some(4.0),
            }];
            spec
        }

        /// Nested schedules `depth` deep: the deepest wire per tree level.
        fn chain(depth: usize) -> DynamicsSpec {
            let mut node = law(PhysicsLaw::Springs);
            for _ in 1..depth {
                node = Node::Schedule {
                    stages: vec![Stage {
                        node,
                        stop: Stop::Rest,
                        capture: None,
                    }],
                    weight: 1.0,
                    overlays: Vec::new(),
                };
            }
            DynamicsSpec::new(node)
        }

        /// The deepest nesting of arrays and objects in JSON text.
        fn json_nesting(text: &[u8]) -> usize {
            let (mut depth, mut deepest, mut in_string, mut escaped) =
                (0usize, 0usize, false, false);
            for &b in text {
                if in_string {
                    match (escaped, b) {
                        (true, _) => escaped = false,
                        (false, b'\\') => escaped = true,
                        (false, b'"') => in_string = false,
                        _ => {},
                    }
                    continue;
                }
                match b {
                    b'"' => in_string = true,
                    b'{' | b'[' => {
                        depth += 1;
                        deepest = deepest.max(depth);
                    },
                    b'}' | b']' => depth -= 1,
                    _ => {},
                }
            }
            deepest
        }

        fn host() -> MereHost<MemoryBackend> {
            MereHost::fixture(MemoryBackend::new(), selected_persona(), fixture_handlers())
                .expect("fixture")
        }

        fn web(host: &MereHost<MemoryBackend>) -> Uuid {
            host.graph()
                .get_node_by_url(FIXTURE_WEB_ADDRESS)
                .unwrap()
                .1
                .id
        }

        fn codicil(host: &MereHost<MemoryBackend>, scene: SavedSceneV3) -> Vec<u8> {
            let web = web(host);
            host.export_product_codicil(ExportRequest {
                focused: web,
                selected: vec![web],
                scope: TransferScope::SelectedSubgraph,
                exported_at_ms: 1,
                include_local_file_locations: false,
                scene: Some(scene),
            })
            .unwrap()
        }

        /// Set `value` under `facet` on a fresh node at `address`.
        fn put(
            host: &mut MereHost<MemoryBackend>,
            address: &str,
            facet: &str,
            value: serde_json::Value,
        ) {
            let id = host.create_address(address, "scene").unwrap();
            let key = host.graph().get_node_key_by_id(id).unwrap();
            host.set_facet(key, facet, value).unwrap();
        }

        /// F142: a version-3 scene converts field by field. Its flat physics
        /// fields become the root and every slot, its damping the
        /// realization's, its layout, pull and roles the target; the view
        /// state stays beside the spec.
        #[test]
        fn a_flat_scene_converts_field_by_field() {
            let member = Uuid::from_u128(9);
            let mut value = legacy();
            value["physics_law"] = "kinds.particle-life".into();
            value["physics_overlays"] = serde_json::json!(["tide", "grid-snap", "tide"]);
            value["physics_kind_source"] = "cluster".into();
            value["physics_group_source"] = "meaning".into();
            value["physics_mass_source"] = "pagerank".into();
            value["physics_depth_source"] = "focus".into();
            value["arrangement_pull"] = serde_json::json!(3.0);
            value["arrangement_roles"] = serde_json::json!({
                "default": "pinned",
                "groups": { "example.test": "anchored" },
                "items": { member.to_string(): "seeded" },
            });
            value["selected"] = serde_json::json!([member]);
            let converted = serde_json::from_value::<SavedSceneV2>(value)
                .unwrap()
                .into_v3()
                .unwrap();
            assert_eq!(converted.selected, vec![member]);
            assert!(converted.physics_paused, "pause stays a scene field (F152)");
            let spec = converted.dynamics_spec().unwrap();
            let mut root = law(PhysicsLaw::Kinds);
            root.overlays_mut().extend([
                Node::preset(PhysicsOverlay::Tide.id()),
                Node::preset(PhysicsOverlay::GridSnap.id()),
            ]);
            assert_eq!(
                spec.root, root,
                "overlays in order, the duplicate collapsed"
            );
            assert_eq!(
                spec.channels,
                BTreeMap::from([
                    ("depth".to_string(), "depth.focus".to_string()),
                    ("groups".to_string(), "groups.meaning".to_string()),
                    ("kind".to_string(), "kind.cluster".to_string()),
                    ("mass".to_string(), "mass.pagerank".to_string()),
                ])
            );
            assert_eq!(
                spec.realization,
                Realization::Integrate { damping: Some(0.5) }
            );
            assert_eq!(
                spec.target,
                Some(Target {
                    arrangement: "grid.default".into(),
                    anchored_pull: 3.0,
                    default_role: Role::Pinned,
                    groups: Some(GroupRoles {
                        channel: "groups.site".into(),
                        roles: BTreeMap::from([("example.test".to_string(), Role::Anchored)]),
                    }),
                    items: BTreeMap::from([(member.to_string(), Role::Seeded)]),
                })
            );

            // A scene saved before the catalog opens as Springs at the
            // defaults; one saved before the roles reads its pull as the
            // roles it acted as; no layout opens the default arrangement.
            let mut before = legacy();
            before["arrangement_pull"] = serde_json::json!(12.0);
            before.as_object_mut().unwrap().remove("layout_strategy");
            let spec = serde_json::from_value::<SavedSceneV2>(before)
                .unwrap()
                .into_v3()
                .unwrap()
                .dynamics_spec()
                .unwrap();
            assert_eq!(spec.root, law(PhysicsLaw::Springs));
            assert_eq!(
                spec.channels,
                mere::canvas::PhysicsChoice::default().into_spec().channels
            );
            let target = spec.target.unwrap();
            assert_eq!(target.arrangement, DEFAULT_ARRANGEMENT);
            assert_eq!(
                (target.default_role, target.anchored_pull),
                (Role::Anchored, 12.0)
            );
            let mut unpulled = legacy();
            unpulled["arrangement_pull"] = serde_json::json!(0.0);
            let target = serde_json::from_value::<SavedSceneV2>(unpulled)
                .unwrap()
                .into_v3()
                .unwrap()
                .dynamics_spec()
                .unwrap()
                .target
                .unwrap();
            assert_eq!(
                (target.default_role, target.anchored_pull),
                (
                    Role::Seeded,
                    f64::from(mere::canvas::DEFAULT_ANCHOR_STIFFNESS)
                )
            );
        }

        /// F143, F154: converting, an unknown law, overlay or source id fails
        /// the open with the id; the same scene with known ids is the control.
        #[test]
        fn an_unknown_flat_id_fails_the_open_with_it() {
            let mut host = host();
            put(
                &mut host,
                "mere://scene/known",
                SAVED_SCENE_FACET_V3,
                legacy(),
            );
            assert!(
                host.product_scene("mere://scene/known").is_ok(),
                "the control"
            );
            let cases = [
                (
                    "physics_law",
                    serde_json::json!("spring.hooke"),
                    "unknown physics law spring.hooke",
                ),
                (
                    "physics_overlays",
                    serde_json::json!(["tide", "eddy"]),
                    "unknown physics overlay eddy",
                ),
                (
                    "physics_kind_source",
                    serde_json::json!("colour"),
                    "unknown kind source colour",
                ),
                (
                    "physics_group_source",
                    serde_json::json!("tribe"),
                    "unknown group source tribe",
                ),
                (
                    "physics_mass_source",
                    serde_json::json!("heft"),
                    "unknown mass source heft",
                ),
                (
                    "physics_depth_source",
                    serde_json::json!("sea"),
                    "unknown depth source sea",
                ),
            ];
            // One node per facet version, its scene rewritten per case: each
            // node added is a timestamp the session's replay check compares,
            // and under load a clock tick between the two fails it (main's
            // kernel flake, Findings 2026-10-04).
            put(
                &mut host,
                "mere://scene/unknown",
                SAVED_SCENE_FACET_V2,
                legacy(),
            );
            let key = host
                .graph()
                .get_node_by_url("mere://scene/unknown")
                .unwrap()
                .0;
            for (field, id, expected) in cases {
                let mut value = legacy();
                value[field] = id;
                host.set_facet(key, SAVED_SCENE_FACET_V2, value).unwrap();
                assert_eq!(
                    host.product_scene("mere://scene/unknown")
                        .unwrap_err()
                        .to_string(),
                    format!("the scene does not open: {expected}")
                );
            }
        }

        /// F153: a version-3 scene's spec wins; the flat fields fill only the
        /// target and the damping it lacks, and a flat id is not read where
        /// the spec speaks.
        #[test]
        fn a_spec_on_a_flat_scene_wins_and_the_flat_fields_fill_its_gaps() {
            let mut bare = DynamicsSpec::new(law(PhysicsLaw::Charge));
            bare.seed = 11;
            let mut value = legacy();
            value["physics_law"] = "spring.hooke".into();
            value["arrangement_pull"] = serde_json::json!(2.0);
            value["dynamics"] = serde_json::to_value(&bare).unwrap();
            let spec = serde_json::from_value::<SavedSceneV2>(value.clone())
                .unwrap()
                .into_v3()
                .expect("the spec speaks for the law")
                .dynamics_spec()
                .unwrap();
            assert_eq!((spec.root.clone(), spec.seed), (bare.root.clone(), 11));
            assert_eq!(
                spec.realization,
                Realization::Integrate { damping: Some(0.5) }
            );
            let target = spec.target.unwrap();
            assert_eq!(
                (
                    target.arrangement.as_str(),
                    target.default_role,
                    target.anchored_pull
                ),
                ("grid.default", Role::Anchored, 2.0)
            );

            let full = runnable(Uuid::from_u128(1));
            value["dynamics"] = serde_json::to_value(&full).unwrap();
            let kept = serde_json::from_value::<SavedSceneV2>(value)
                .unwrap()
                .into_v3()
                .unwrap()
                .dynamics_spec()
                .unwrap();
            assert_eq!(
                kept, full,
                "a spec with a target and a damping is taken whole"
            );
        }

        /// F142: a version-4 scene saves under its own facet and reopens
        /// byte for byte, through the facet and through a codicil opened as a
        /// session; no version-3 facet is written.
        #[test]
        fn a_spec_saves_and_reopens_byte_for_byte_with_its_scene() {
            let mut host = host();
            let web = web(&host);
            let spec = runnable(web);
            mere::canvas::dynamics_spec::bind(&spec).expect("a runnable spec binds");
            let scene = SavedSceneV3 {
                selected: vec![web],
                ..scene(&spec)
            };
            host.save_product_scene("mere://scene/spec", &scene)
                .unwrap();
            let reopened = host.product_scene("mere://scene/spec").unwrap();
            assert_eq!(reopened, scene);
            assert_eq!(
                serde_json::to_string(&reopened).unwrap(),
                serde_json::to_string(&scene).unwrap(),
                "byte for byte"
            );
            assert_eq!(
                host.facet_value("mere://scene/spec", SAVED_SCENE_FACET),
                Some(&serde_json::to_value(&scene).unwrap())
            );
            assert!(
                host.facet_value("mere://scene/spec", SAVED_SCENE_FACET_V3)
                    .is_none()
            );
            assert_eq!(SAVED_SCENE_FACET, "graphshell.saved-scene/v4");
            // Through the codicil, as JSON text, opened as a session.
            let bytes = codicil(&host, scene);
            let text: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(text["schema"], PRODUCT_CODICIL_SCHEMA);
            let (_, imported) = host.replace_with_product_codicil(&bytes).unwrap();
            assert_eq!(imported.unwrap().dynamics_spec(), Ok(spec));
        }

        #[test]
        fn a_refused_spec_fails_the_open_by_name_and_place() {
            let mut host = host();
            let mut spec = runnable(Uuid::from_u128(1));
            let Node::Schedule { stages, .. } = &mut spec.root else {
                unreachable!("runnable's root is a schedule")
            };
            stages[1].node = Node::Mix {
                parts: vec![law(PhysicsLaw::Springs), Node::preset("charge.coulomb")],
                weight: 1.0,
                overlays: Vec::new(),
            };
            let unknown = scene(&spec);
            host.save_product_scene("mere://scene/unknown", &unknown)
                .unwrap();
            let err = host
                .product_scene("mere://scene/unknown")
                .unwrap_err()
                .to_string();
            assert_eq!(
                err,
                "the scene does not open: dynamics spec: at root.stages[1].node.parts[1]: \
                 unknown law preset charge.coulomb"
            );
            // A newer spec is refused in the reader's words (F98).
            let good = scene(&runnable(Uuid::from_u128(1)));
            host.save_product_scene("mere://scene/newer", &good)
                .unwrap();
            assert!(
                host.product_scene("mere://scene/newer").is_ok(),
                "the control"
            );
            let key = host
                .graph()
                .get_node_by_url("mere://scene/newer")
                .unwrap()
                .0;
            let mut value = serde_json::to_value(&good).unwrap();
            value["dynamics"]["version"] = serde_json::json!(2);
            host.set_facet(key, SAVED_SCENE_FACET, value).unwrap();
            let err = host
                .product_scene("mere://scene/newer")
                .unwrap_err()
                .to_string();
            assert!(
                err.contains("at version: dynamics spec version 2 is newer than this reader's 1"),
                "{err}"
            );
            // A codicil carrying a refused spec does not open as a session.
            let session = host.graph_session().id();
            let bytes = codicil(&host, unknown);
            assert!(matches!(
                host.replace_with_product_codicil(&bytes),
                Err(ProductError::InvalidScene(_))
            ));
            assert_eq!(host.graph_session().id(), session, "no session begun");
        }

        /// F149: a channel that does not parse, or that its slot, a grouping
        /// or the role groups cannot read, fails the open by its place and id.
        #[test]
        fn an_unreadable_channel_fails_the_open_by_slot_and_id() {
            let mut host = host();
            let good = runnable(Uuid::from_u128(1));
            host.save_product_scene("mere://scene/channels", &scene(&good))
                .unwrap();
            assert!(
                host.product_scene("mere://scene/channels").is_ok(),
                "the control"
            );
            let edits: [(&dyn Fn(&mut DynamicsSpec), &str); 5] = [
                (
                    &|s| {
                        s.channels.insert("kind".into(), "kind.colour".into());
                    },
                    "at channels.kind: unknown channel kind.colour",
                ),
                (
                    &|s| {
                        s.channels.insert("mass".into(), "weight.degree".into());
                    },
                    "at channels.mass: the mass slot cannot read weight.degree",
                ),
                (
                    &|s| {
                        s.channels.insert("hue".into(), "kind.site".into());
                    },
                    "at channels.hue: no preset reads a slot named hue",
                ),
                (
                    &|s| {
                        let Node::Schedule { stages, .. } = &mut s.root else {
                            unreachable!()
                        };
                        let Node::Grouped { partition, .. } = &mut stages[2].node else {
                            unreachable!()
                        };
                        *partition = "groups.bridges".into();
                    },
                    "at root.stages[2].node.partition: groups.bridges is not a groups channel the \
                     canvas partitions by",
                ),
                (
                    &|s| {
                        s.target.as_mut().unwrap().groups.as_mut().unwrap().channel =
                            "groups.bridges".into()
                    },
                    "at target.groups.channel: groups.bridges is not a groups channel the canvas \
                     partitions by",
                ),
            ];
            for (i, (edit, expected)) in edits.into_iter().enumerate() {
                let mut spec = good.clone();
                edit(&mut spec);
                let address = format!("mere://scene/channel-{i}");
                host.save_product_scene(&address, &scene(&spec)).unwrap();
                assert_eq!(
                    host.product_scene(&address).unwrap_err().to_string(),
                    format!("the scene does not open: dynamics spec: {expected}")
                );
            }
        }

        /// F113: each kind of refusal serde makes while the spec is read
        /// names its path at the open: an unknown raw kind, an unknown field,
        /// an unknown role, a newer version.
        #[test]
        fn a_spec_refused_while_read_names_its_path_at_the_open() {
            let mut host = host();
            let good = scene(&runnable(Uuid::from_u128(1)));
            host.save_product_scene("mere://scene/read", &good).unwrap();
            assert!(
                host.product_scene("mere://scene/read").is_ok(),
                "the control"
            );
            let key = host.graph().get_node_by_url("mere://scene/read").unwrap().0;
            let base_value = serde_json::to_value(&good).unwrap();
            let cases: [(&dyn Fn(&mut serde_json::Value), &str); 4] = [
                (
                    &|v| {
                        v["dynamics"]["root"]["stages"][3]["node"] =
                            serde_json::json!({ "node": "raw", "term": { "densty": {} } })
                    },
                    "at root.stages[3].node.term: unknown variant `densty`",
                ),
                (
                    &|v| v["dynamics"]["root"]["stages"][1]["node"]["colour"] = "red".into(),
                    "at root.stages[1].node.colour: unknown field `colour`",
                ),
                (
                    &|v| v["dynamics"]["target"]["default_role"] = "tethered".into(),
                    "at target.default_role: unknown variant `tethered`",
                ),
                (
                    &|v| v["dynamics"]["version"] = 2.into(),
                    "at version: dynamics spec version 2 is newer than this reader's 1",
                ),
            ];
            for (edit, expected) in cases {
                let mut value = base_value.clone();
                edit(&mut value);
                host.set_facet(key, SAVED_SCENE_FACET, value).unwrap();
                let err = host
                    .product_scene("mere://scene/read")
                    .unwrap_err()
                    .to_string();
                assert!(
                    err.starts_with(&format!(
                        "the scene does not open: dynamics spec: {expected}"
                    )),
                    "{err}"
                );
            }
        }

        /// F110: a depth-32 spec round-trips through the JSON carrier, parsed
        /// from a codicil's text (serde_json's limit, 128), and derive refuses
        /// depth 33.
        #[test]
        fn a_depth_32_spec_round_trips_through_the_json_carrier() {
            let host = host();
            for depth in [32, 33] {
                let spec = chain(depth);
                assert_eq!(spec.depth(), depth);
                let bytes = codicil(&host, scene(&spec));
                let nesting = json_nesting(&bytes);
                println!("depth {depth}: codicil JSON nesting {nesting} (serde_json limit 128)");
                let codicil = decode_codicil(&bytes).expect("parsed within serde_json's limit");
                assert_eq!(codicil.scene.unwrap().dynamics.spec(), Ok(spec.clone()));
                let refusal = mere::canvas::dynamics_spec::derive(&spec)
                    .unwrap_err()
                    .to_string();
                if depth == 33 {
                    assert_eq!(refusal, "dynamics spec is 33 deep, deeper than 32");
                } else {
                    assert!(
                        refusal.contains("a schedule runs only at the root"),
                        "32 passes the cap and meets the runnability check: {refusal}"
                    );
                }
            }
        }

        /// F159: a version-2 codicil is read and its scene converted, the
        /// legacy engram too; any other schema is refused by name; and an
        /// export narrows the target's items to the exported members.
        #[test]
        fn a_version_2_codicil_converts_and_an_export_narrows_the_target() {
            let host = host();
            let web = web(&host);
            let outside = Uuid::from_u128(77);
            let mut spec = runnable(web);
            spec.target
                .as_mut()
                .unwrap()
                .items
                .insert(outside.to_string(), Role::Anchored);
            let bytes = codicil(&host, scene(&spec));
            let exported = decode_codicil(&bytes).unwrap().scene.unwrap();
            let items = exported.dynamics_spec().unwrap().target.unwrap().items;
            assert_eq!(
                items,
                BTreeMap::from([(web.to_string(), Role::Pinned)]),
                "only exported members keep their roles"
            );

            let mut old: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let mut flat = legacy();
            flat["physics_law"] = "stress.kamada-kawai".into();
            old["scene"] = flat;
            for schema in [PRODUCT_CODICIL_SCHEMA_V2, LEGACY_PRODUCT_ENGRAM_SCHEMA] {
                old["schema"] = schema.into();
                let read = decode_codicil(&serde_json::to_vec(&old).unwrap()).unwrap();
                assert_eq!(read.schema, schema);
                // Every field but the scene comes across as written.
                let carried = serde_json::to_value(&read).unwrap();
                for field in ["scope", "exported_at_ms", "graph", "facets"] {
                    assert_eq!(carried[field], old[field], "{schema}: {field}");
                }
                assert_eq!(
                    read.scene.unwrap().dynamics_spec().unwrap().root,
                    law(PhysicsLaw::Stress)
                );
            }
            old["scene"]["physics_law"] = "spring.hooke".into();
            assert!(matches!(
                decode_codicil(&serde_json::to_vec(&old).unwrap()),
                Err(ProductError::InvalidScene(error)) if error == "unknown physics law spring.hooke"
            ));
            old["schema"] = "graphshell.graph-codicil/v9".into();
            let err = decode_codicil(&serde_json::to_vec(&old).unwrap())
                .unwrap_err()
                .to_string();
            assert!(err.ends_with("found graphshell.graph-codicil/v9"), "{err}");
        }

        /// The codicil the `codicil_grouping` receipt opens (F161): eight
        /// objects on two sites in a ring, and a scene whose spec runs
        /// Charge between the sites' groups and Springs within.
        fn grouping_codicil() -> ProductCodicilV3 {
            use mere::kernel::graph::EdgeAssertion;
            let mut graph = Graph::new();
            let keys: Vec<_> = (0..8u128)
                .map(|i| {
                    add_node(
                        &mut graph,
                        Some(Uuid::from_u128(0x6b1_0000 + i)),
                        format!("https://s{}.example/g4b1/{i}", i % 2),
                        PortablePoint::new(0.0, 0.0),
                    )
                })
                .collect();
            for i in 0..8 {
                assert_relation(
                    &mut graph,
                    keys[i],
                    keys[(i + 1) % 8],
                    EdgeAssertion::Semantic {
                        sub_kind: SemanticSubKind::Hyperlink,
                        label: None,
                        decay_progress: None,
                    },
                );
            }
            let ids: Vec<Uuid> = keys
                .iter()
                .map(|k| graph.get_node(*k).unwrap().id)
                .collect();
            let mut spec = DynamicsSpec::new(Node::Grouped {
                partition: "groups.site".into(),
                outer: Box::new(at(PhysicsLaw::Charge, 16.0)),
                inner: Box::new(law(PhysicsLaw::Springs)),
                weight: 1.0,
                overlays: Vec::new(),
            });
            spec.channels = mere::canvas::PhysicsChoice::default().into_spec().channels;
            spec.realization = Realization::Integrate { damping: Some(0.7) };
            spec.target = Some(Target {
                arrangement: "grid.default".into(),
                anchored_pull: f64::from(mere::canvas::DEFAULT_ANCHOR_STIFFNESS),
                default_role: Role::Seeded,
                groups: None,
                items: BTreeMap::new(),
            });
            let mut snapshot = graph.to_snapshot();
            snapshot.timestamp_secs = 1_700_000_000;
            ProductCodicilV3 {
                schema: PRODUCT_CODICIL_SCHEMA.to_string(),
                scope: TransferScope::SelectedSubgraph,
                exported_at_ms: 1_700_000_000_000,
                graph: snapshot,
                facets: NodeFacetStore::new(),
                scene: Some(SavedSceneV3 {
                    name: "Charge between sites".into(),
                    selected: ids,
                    physics_paused: false,
                    ..scene(&spec)
                }),
            }
        }

        /// Prints the receipt's codicil as one line, for its `type` step.
        #[test]
        #[ignore = "writes the codicil_grouping receipt's text; run by hand"]
        fn print_the_grouping_receipts_codicil() {
            println!("{}", serde_json::to_string(&grouping_codicil()).unwrap());
        }

        /// F161: the codicil the receipt types in reads as version 3 and its
        /// scene's spec binds to a grouping on the site channel.
        #[test]
        fn the_grouping_receipts_codicil_reads_and_binds() {
            const RECEIPT: &str = include_str!("../web/scenarios/codicil_grouping.scn");
            let line = RECEIPT
                .lines()
                .find_map(|line| line.strip_prefix("type #gs-codicil-data "))
                .expect("the receipt types a codicil");
            let codicil = decode_codicil(line.as_bytes()).expect("reads");
            assert_eq!(codicil.schema, PRODUCT_CODICIL_SCHEMA);
            assert_eq!(codicil.graph.nodes.len(), 8);
            let spec = codicil.scene.unwrap().dynamics_spec().expect("binds");
            assert!(matches!(
                spec.root,
                Node::Grouped { ref partition, .. } if partition == "groups.site"
            ));
        }
    }
}
