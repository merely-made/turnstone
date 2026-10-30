// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Pure pane, context, and space-blueprint authority.
//!
//! This is the migration target for the legacy [`super::FrisketLayout`]. It is
//! deliberately rendering-free: hosts may project it onto Turnstone's mixed-
//! surface compositor, Genet's `TileTree`, or a receipt without changing truth.

use std::collections::{HashMap, HashSet};

use mere::forme::{FormeRef, GraphMemberId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{GraphId, PaneId, SessionId, SplitAxis};

#[path = "blueprint/context.rs"]
mod context;
#[path = "blueprint/layout.rs"]
mod layout;
#[path = "blueprint/presentation.rs"]
mod presentation;

pub use context::ContextIndex;
pub use presentation::{
    BlueprintDividerPlacement, BlueprintFloatPlacement, BlueprintPanePlacement, BlueprintTiling,
    place_space, place_space_with_float_layer, surface_plan_for_space,
    surface_plan_for_space_with_float_layer,
};

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

string_id!(SpaceId);
string_id!(PaneKindId);
string_id!(PaneConfigSchemaId);
string_id!(PaneViewSchemaId);
string_id!(SourceSchemaId);

/// A schema-owned encoded pane configuration. The pane definition, not the
/// space model, owns defaults, validation, and decoding for `payload`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaneConfig {
    pub schema: PaneConfigSchemaId,
    pub version: u32,
    pub payload: Value,
}

/// Pane-local mutable state. Each pane definition decides which fields, if
/// any, are copied into a saved blueprint.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaneViewState {
    pub schema: PaneViewSchemaId,
    pub version: u32,
    pub payload: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SerializedSource {
    pub version: u32,
    pub payload: Value,
}

/// A fixed content authority. Pane kind remains independent: Graph and
/// Workbench panes may both refer to a Forme while projecting it differently.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SourceRef {
    Graph(GraphId),
    Forme {
        graph: GraphId,
        forme: FormeRef,
    },
    Member {
        graph: GraphId,
        member: GraphMemberId,
    },
    Settings(String),
    Session(SessionId),
    SessionSet,
    Application,
    External {
        schema: SourceSchemaId,
        payload: SerializedSource,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceSelector {
    Graph,
    Forme,
    Member,
    Session,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PaneSource {
    Fixed(SourceRef),
    FromContext(SourceSelector),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextBinding {
    Own,
    Follow(PaneId),
    FocusedInOwnSpace,
    Application,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaneContext {
    pub graph: Option<GraphId>,
    pub forme: Option<FormeRef>,
    pub member: Option<GraphMemberId>,
    pub session: Option<SessionId>,
}

/// Durable pane instance description. It carries intent, never a retained
/// widget, surface, graph runtime, or Forme runtime.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaneSpec {
    pub id: PaneId,
    pub kind: PaneKindId,
    pub source: PaneSource,
    pub context: ContextBinding,
    pub config: PaneConfig,
}

/// Live pane state keyed by `PaneId`. The retained renderer joins this record
/// at A1; it does not become part of the serializable [`PaneSpec`].
#[derive(Clone, Debug, PartialEq)]
pub struct PaneRecord {
    pub spec: PaneSpec,
    pub view: PaneViewState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneMultiplicity {
    Many,
    PerSpace,
    PerSpaceAndSource,
    PerSpaceAndContext,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutBranch {
    pub fraction: f32,
    pub tree: LayoutNode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GridShares {
    pub columns: Vec<f32>,
    pub rows: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LayoutNode {
    Pane(PaneId),
    Split {
        axis: SplitAxis,
        children: Vec<LayoutBranch>,
    },
    Tabs {
        children: Vec<LayoutNode>,
        active: usize,
    },
    Grid {
        children: Vec<LayoutNode>,
        columns: usize,
        shares: GridShares,
    },
}

/// A stable path through a [`LayoutNode`] tree. Unlike the retired binary
/// `SplitPath`, each step names both the container kind and child ordinal, so
/// a divider drag can address an N-ary split without inventing fake panes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutPathStep {
    Split(usize),
    Tab(usize),
    Grid(usize),
}

pub type LayoutPath = Vec<LayoutPathStep>;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelativeRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Physical limits applied after a floating pane's proportional rectangle is
/// resolved against its current window. Fractions preserve intent on resize;
/// these pixel bounds prevent a useful pane from becoming too small or too
/// large for its controls.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FloatSizeConstraints {
    pub min_width: f32,
    pub min_height: f32,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FloatingPane {
    pub pane: PaneId,
    pub rect: RelativeRect,
    /// Pixel constraints alongside the proportional `rect`. `default` keeps
    /// A0/A4 layouts readable while the format remains pre-alpha.
    #[serde(default)]
    pub constraints: FloatSizeConstraints,
    pub z: u32,
    pub pinned: bool,
    pub visible: bool,
}

/// A float's destination inside its own space. It is a station relocation:
/// the moved pane keeps its spec and `PaneId`, so retained runner state never
/// has to be copied or reconstructed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FloatDockTarget {
    /// Use an empty tiled root. Refused when the space already has tiled
    /// content, because replacing a tree would silently orphan panes.
    TiledRoot,
    Beside {
        target: PaneId,
        axis: SplitAxis,
        after: bool,
    },
    Tab {
        target: PaneId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChromeEdge {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChromePlacement {
    Overlay,
    Docked(ChromeEdge),
    Floating,
    Pane(PaneId),
    Hidden,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChromeBlueprint {
    pub omnibar: ChromePlacement,
    pub shellbar: ChromePlacement,
    pub transcript: ChromePlacement,
    pub status: ChromePlacement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizationPolicy {
    pub collapse_single_child: bool,
    pub join_same_axis_splits: bool,
    pub prune_unknown_panes: bool,
}

/// One OS-window composition. Pane specs are stored beside their one tiled or
/// floating station; graph/Forme/Workbench truth remains elsewhere.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpaceBlueprint {
    pub id: SpaceId,
    pub label: String,
    pub panes: Vec<PaneSpec>,
    pub tiled: Option<LayoutNode>,
    pub floating: Vec<FloatingPane>,
    pub chrome: ChromeBlueprint,
    pub normalization: NormalizationPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlueprintViolation {
    DuplicateSpaceId(SpaceId),
    DuplicatePaneSpec(PaneId),
    MissingPaneSpec { space: SpaceId, pane: PaneId },
    UnplacedPane { space: SpaceId, pane: PaneId },
    DuplicatePaneStation(PaneId),
    FormeGraphMismatch(PaneId),
}

#[path = "blueprint/impl_default_for_chrome_blueprint.rs"]
mod impl_default_for_chrome_blueprint;
#[path = "blueprint/impl_default_for_float_size_constraints.rs"]
mod impl_default_for_float_size_constraints;
#[path = "blueprint/impl_default_for_normalization_policy.rs"]
mod impl_default_for_normalization_policy;
#[path = "blueprint/pane_config_empty.rs"]
mod pane_config_empty;
#[path = "blueprint/pane_context_graph.rs"]
mod pane_context_graph;
#[path = "blueprint/pane_source_fixed_context.rs"]
mod pane_source_fixed_context;
#[path = "blueprint/pane_spec_pin.rs"]
mod pane_spec_pin;
#[path = "blueprint/pane_view_state_empty.rs"]
mod pane_view_state_empty;
#[path = "blueprint/source_ref_context.rs"]
mod source_ref_context;
#[path = "blueprint/space_blueprint_normalize.rs"]
mod space_blueprint_normalize;
#[path = "blueprint/validate_spaces.rs"]
mod validate_spaces;
pub use validate_spaces::validate_spaces;
