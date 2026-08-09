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

pub use context::ContextIndex;

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

impl PaneConfig {
    pub fn empty(schema: impl Into<String>) -> Self {
        Self {
            schema: PaneConfigSchemaId::new(schema),
            version: 1,
            payload: Value::Null,
        }
    }
}

/// Pane-local mutable state. Each pane definition decides which fields, if
/// any, are copied into a saved blueprint.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaneViewState {
    pub schema: PaneViewSchemaId,
    pub version: u32,
    pub payload: Value,
}

impl PaneViewState {
    pub fn empty(schema: impl Into<String>) -> Self {
        Self {
            schema: PaneViewSchemaId::new(schema),
            version: 1,
            payload: Value::Null,
        }
    }
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

impl SourceRef {
    pub fn context(&self) -> PaneContext {
        match self {
            Self::Graph(graph) => PaneContext::graph(*graph),
            Self::Forme { graph, forme } => PaneContext {
                graph: Some(*graph),
                forme: Some(*forme),
                ..PaneContext::default()
            },
            Self::Member { graph, member } => PaneContext {
                graph: Some(*graph),
                member: Some(*member),
                ..PaneContext::default()
            },
            Self::Session(session) => PaneContext {
                session: Some(*session),
                ..PaneContext::default()
            },
            Self::Settings(_) | Self::SessionSet | Self::Application | Self::External { .. } => {
                PaneContext::default()
            }
        }
    }
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

impl PaneSource {
    pub fn fixed_context(&self) -> Option<PaneContext> {
        match self {
            Self::Fixed(source) => Some(source.context()),
            Self::FromContext(_) => None,
        }
    }
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

impl PaneContext {
    pub fn graph(graph: GraphId) -> Self {
        Self {
            graph: Some(graph),
            ..Self::default()
        }
    }

    pub fn source(self, selector: SourceSelector) -> Option<SourceRef> {
        match selector {
            SourceSelector::Graph => self.graph.map(SourceRef::Graph),
            SourceSelector::Forme => Some(SourceRef::Forme {
                graph: self.graph?,
                forme: self.forme?,
            }),
            SourceSelector::Member => Some(SourceRef::Member {
                graph: self.graph?,
                member: self.member?,
            }),
            SourceSelector::Session => self.session.map(SourceRef::Session),
        }
    }

    fn supplies(self, selector: SourceSelector) -> bool {
        self.source(selector).is_some()
    }
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

impl PaneSpec {
    pub fn pin(&mut self, source: SourceRef) {
        self.source = PaneSource::Fixed(source);
        self.context = ContextBinding::Own;
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelativeRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FloatingPane {
    pub pane: PaneId,
    pub rect: RelativeRect,
    pub z: u32,
    pub pinned: bool,
    pub visible: bool,
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

impl Default for ChromeBlueprint {
    fn default() -> Self {
        Self {
            omnibar: ChromePlacement::Overlay,
            shellbar: ChromePlacement::Docked(ChromeEdge::Right),
            transcript: ChromePlacement::Hidden,
            status: ChromePlacement::Docked(ChromeEdge::Bottom),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizationPolicy {
    pub collapse_single_child: bool,
    pub join_same_axis_splits: bool,
    pub prune_unknown_panes: bool,
}

impl Default for NormalizationPolicy {
    fn default() -> Self {
        Self {
            collapse_single_child: true,
            join_same_axis_splits: true,
            prune_unknown_panes: true,
        }
    }
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

impl SpaceBlueprint {
    pub fn normalize(&mut self) {
        let known: HashSet<_> = self.panes.iter().map(|pane| pane.id).collect();
        self.tiled = self
            .tiled
            .take()
            .and_then(|tree| tree.normalized(&known, self.normalization));
    }

    pub fn float_pane(&mut self, pane: PaneId, rect: RelativeRect) -> bool {
        if !self.panes.iter().any(|spec| spec.id == pane) {
            return false;
        }
        let old_float = self.floating.iter().find(|item| item.pane == pane).cloned();
        self.floating.retain(|item| item.pane != pane);
        if let Some(tree) = self.tiled.take() {
            self.tiled = tree.without_pane(pane);
        }
        let z = self.floating.iter().map(|item| item.z).max().unwrap_or(0) + 1;
        self.floating.push(FloatingPane {
            pane,
            rect,
            z,
            pinned: old_float.as_ref().is_some_and(|item| item.pinned),
            visible: old_float.as_ref().is_none_or(|item| item.visible),
        });
        self.normalize();
        true
    }

    pub fn take_pane(&mut self, pane: PaneId) -> Option<PaneSpec> {
        let index = self.panes.iter().position(|spec| spec.id == pane)?;
        if let Some(tree) = self.tiled.take() {
            self.tiled = tree.without_pane(pane);
        }
        self.floating.retain(|item| item.pane != pane);
        let spec = self.panes.remove(index);
        self.normalize();
        Some(spec)
    }

    pub fn insert_floating(
        &mut self,
        spec: PaneSpec,
        rect: RelativeRect,
    ) -> Result<(), BlueprintViolation> {
        if self.panes.iter().any(|pane| pane.id == spec.id) {
            return Err(BlueprintViolation::DuplicatePaneSpec(spec.id));
        }
        let pane = spec.id;
        self.panes.push(spec);
        self.floating.push(FloatingPane {
            pane,
            rect,
            z: self.floating.iter().map(|item| item.z).max().unwrap_or(0) + 1,
            pinned: false,
            visible: true,
        });
        Ok(())
    }

    fn station_ids(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        if let Some(tree) = &self.tiled {
            tree.collect_panes(&mut out);
        }
        out.extend(self.floating.iter().map(|item| item.pane));
        out
    }
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

/// Validate the global identity law: each pane spec and station appears once
/// across all live spaces, and fixed identity Formes name their actual graph.
pub fn validate_spaces(spaces: &[SpaceBlueprint]) -> Vec<BlueprintViolation> {
    let mut violations = Vec::new();
    let mut space_ids = HashSet::new();
    let mut specs: HashMap<PaneId, SpaceId> = HashMap::new();
    let mut station_counts: HashMap<PaneId, usize> = HashMap::new();

    for space in spaces {
        if !space_ids.insert(space.id.clone()) {
            violations.push(BlueprintViolation::DuplicateSpaceId(space.id.clone()));
        }
        let local_specs: HashSet<_> = space.panes.iter().map(|pane| pane.id).collect();
        for spec in &space.panes {
            if specs.insert(spec.id, space.id.clone()).is_some() {
                violations.push(BlueprintViolation::DuplicatePaneSpec(spec.id));
            }
            if let PaneSource::Fixed(SourceRef::Forme {
                graph,
                forme: FormeRef::Identity(identity_graph),
            }) = spec.source
                && graph.0 != identity_graph
            {
                violations.push(BlueprintViolation::FormeGraphMismatch(spec.id));
            }
        }
        for pane in space.station_ids() {
            if !local_specs.contains(&pane) {
                violations.push(BlueprintViolation::MissingPaneSpec {
                    space: space.id.clone(),
                    pane,
                });
            }
            *station_counts.entry(pane).or_default() += 1;
        }
    }

    for (pane, space) in specs {
        match station_counts.get(&pane).copied().unwrap_or(0) {
            0 => violations.push(BlueprintViolation::UnplacedPane { space, pane }),
            1 => {}
            _ => violations.push(BlueprintViolation::DuplicatePaneStation(pane)),
        }
    }
    violations
}
