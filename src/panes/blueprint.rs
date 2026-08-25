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

impl Default for FloatSizeConstraints {
    fn default() -> Self {
        Self {
            min_width: 0.0,
            min_height: 0.0,
            max_width: None,
            max_height: None,
        }
    }
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

    /// All pane ids in the tiled topology, including inactive tab children.
    /// They still own one station and retained state even while hidden.
    pub fn tiled_panes(&self) -> Vec<PaneId> {
        let mut panes = Vec::new();
        if let Some(tree) = &self.tiled {
            tree.collect_panes(&mut panes);
        }
        panes
    }

    /// The tiled panes whose surfaces are currently live. Hosts use this for
    /// rendering, hit testing, accessibility focus, and pumping; inactive
    /// tab children are intentionally absent.
    pub fn active_tiled_panes(&self) -> Vec<PaneId> {
        let mut panes = Vec::new();
        if let Some(tree) = &self.tiled {
            tree.collect_active_panes(&mut panes);
        }
        panes
    }

    pub fn pane(&self, id: PaneId) -> Option<&PaneSpec> {
        self.panes.iter().find(|pane| pane.id == id)
    }

    /// Move one tiled pane beside another. This is the topology half of a
    /// pane drag; the host keeps renderer state keyed by the unchanged id.
    pub fn move_pane_beside(
        &mut self,
        pane: PaneId,
        target: PaneId,
        axis: SplitAxis,
        after: bool,
    ) -> bool {
        if pane == target
            || !self.tiled_panes().contains(&pane)
            || !self.tiled_panes().contains(&target)
        {
            return false;
        }
        let Some(mut tree) = self.tiled.take().and_then(|tree| tree.without_pane(pane)) else {
            return false;
        };
        if !tree.insert_beside(target, pane, axis, after) {
            return false;
        }
        self.tiled = Some(tree);
        self.normalize();
        true
    }

    /// Move one tiled pane into a tabbed subtree over `target` and select it.
    pub fn stack_pane_onto(&mut self, pane: PaneId, target: PaneId) -> bool {
        if pane == target
            || !self.tiled_panes().contains(&pane)
            || !self.tiled_panes().contains(&target)
        {
            return false;
        }
        let Some(mut tree) = self.tiled.take().and_then(|tree| tree.without_pane(pane)) else {
            return false;
        };
        if !tree.insert_tab(target, pane) {
            return false;
        }
        self.tiled = Some(tree);
        self.normalize();
        true
    }

    /// Select the tab containing `pane`. The pane remains in the same station;
    /// only its visibility and active surface lifecycle change.
    pub fn activate_tab(&mut self, pane: PaneId) -> bool {
        self.tiled
            .as_mut()
            .is_some_and(|tree| tree.activate_tab_containing(pane))
    }

    /// Apply a complete N-ary split weighting at `path`. Fractions are
    /// normalized as one topology edit, so a resize cannot leave a zero-width
    /// sibling behind.
    pub fn set_split_fractions(&mut self, path: &[LayoutPathStep], fractions: &[f32]) -> bool {
        self.tiled
            .as_mut()
            .is_some_and(|tree| tree.set_split_fractions(path, fractions))
    }

    /// Receive a pane that was torn out of another space as this space's tiled
    /// root. The pane id and its spec are transferred intact; A5 adds the
    /// floating station and window geometry around this same operation.
    pub fn insert_tiled_root(&mut self, spec: PaneSpec) -> Result<(), BlueprintViolation> {
        if self.panes.iter().any(|pane| pane.id == spec.id) {
            return Err(BlueprintViolation::DuplicatePaneSpec(spec.id));
        }
        if self.tiled.is_some() || !self.floating.is_empty() {
            return Err(BlueprintViolation::DuplicatePaneStation(spec.id));
        }
        let pane = spec.id;
        self.panes.push(spec);
        self.tiled = Some(LayoutNode::Pane(pane));
        Ok(())
    }

    /// Insert a new tiled pane beside an existing station, preserving the
    /// caller's pane id, source, and config as one blueprint operation. The
    /// topology is changed only after all identity checks pass.
    pub fn insert_tiled_beside(
        &mut self,
        spec: PaneSpec,
        target: PaneId,
        axis: SplitAxis,
        after: bool,
    ) -> Result<(), BlueprintViolation> {
        if self.panes.iter().any(|pane| pane.id == spec.id) {
            return Err(BlueprintViolation::DuplicatePaneSpec(spec.id));
        }
        if self.station_ids().contains(&spec.id) {
            return Err(BlueprintViolation::DuplicatePaneStation(spec.id));
        }
        if !self.tiled_panes().contains(&target) {
            return Err(BlueprintViolation::MissingPaneSpec {
                space: self.id.clone(),
                pane: target,
            });
        }
        let pane = spec.id;
        let Some(mut tree) = self.tiled.take() else {
            return Err(BlueprintViolation::MissingPaneSpec {
                space: self.id.clone(),
                pane: target,
            });
        };
        if !tree.insert_beside(target, pane, axis, after) {
            self.tiled = Some(tree);
            return Err(BlueprintViolation::MissingPaneSpec {
                space: self.id.clone(),
                pane: target,
            });
        }
        self.tiled = Some(tree);
        self.panes.push(spec);
        self.normalize();
        Ok(())
    }

    /// Floats that are live in the current window. Hiding the float layer
    /// suppresses ordinary floats, but a pinned float remains visible by
    /// definition. The host owns the toggle because it is transient window
    /// presentation state, not a second layout tree.
    pub fn visible_floating_panes(&self, float_layer_visible: bool) -> Vec<&FloatingPane> {
        let mut floats: Vec<_> = self
            .floating
            .iter()
            .filter(|item| item.visible && (float_layer_visible || item.pinned))
            .collect();
        floats.sort_by_key(|item| (item.z, item.pane.0));
        floats
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
        let z = self
            .floating
            .iter()
            .map(|item| item.z)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.floating.push(FloatingPane {
            pane,
            rect,
            constraints: old_float
                .as_ref()
                .map(|item| item.constraints)
                .unwrap_or_default(),
            z,
            pinned: old_float.as_ref().is_some_and(|item| item.pinned),
            visible: old_float.as_ref().is_none_or(|item| item.visible),
        });
        self.normalize();
        true
    }

    /// Update the proportional placement without changing the pane's station
    /// or z-order. A shell calls this while a float drag is in flight and
    /// persists once the gesture settles.
    pub fn set_float_rect(&mut self, pane: PaneId, rect: RelativeRect) -> bool {
        let Some(float) = self.floating.iter_mut().find(|item| item.pane == pane) else {
            return false;
        };
        float.rect = rect;
        true
    }

    pub fn set_float_constraints(
        &mut self,
        pane: PaneId,
        constraints: FloatSizeConstraints,
    ) -> bool {
        let Some(float) = self.floating.iter_mut().find(|item| item.pane == pane) else {
            return false;
        };
        float.constraints = constraints;
        true
    }

    pub fn set_float_pinned(&mut self, pane: PaneId, pinned: bool) -> bool {
        let Some(float) = self.floating.iter_mut().find(|item| item.pane == pane) else {
            return false;
        };
        float.pinned = pinned;
        true
    }

    pub fn set_float_visible(&mut self, pane: PaneId, visible: bool) -> bool {
        let Some(float) = self.floating.iter_mut().find(|item| item.pane == pane) else {
            return false;
        };
        float.visible = visible;
        true
    }

    /// Raise the clicked float above every other float in this space. Equal or
    /// stale persisted z-values are normalized first so order is deterministic
    /// and the counter cannot drift indefinitely across saves.
    pub fn raise_float(&mut self, pane: PaneId) -> bool {
        if !self.floating.iter().any(|item| item.pane == pane) {
            return false;
        }
        self.normalize_float_z();
        let index = self
            .floating
            .iter()
            .position(|item| item.pane == pane)
            .expect("float was checked before z normalization");
        let z = self.floating.len() as u32 + 1;
        self.floating[index].z = z;
        self.normalize_float_z();
        true
    }

    /// Dock a floating pane back into the serializable tiled topology. The
    /// pane spec remains in this space and keeps the same identity; only its
    /// station changes.
    pub fn dock_floating_pane(&mut self, pane: PaneId, target: FloatDockTarget) -> bool {
        let Some(float_index) = self.floating.iter().position(|item| item.pane == pane) else {
            return false;
        };
        let docked = match target {
            FloatDockTarget::TiledRoot => {
                if self.tiled.is_some() {
                    return false;
                }
                self.tiled = Some(LayoutNode::Pane(pane));
                true
            }
            FloatDockTarget::Beside {
                target,
                axis,
                after,
            } => self
                .tiled
                .as_mut()
                .is_some_and(|tree| tree.insert_beside(target, pane, axis, after)),
            FloatDockTarget::Tab { target } => self
                .tiled
                .as_mut()
                .is_some_and(|tree| tree.insert_tab(target, pane)),
        };
        if !docked {
            return false;
        }
        self.floating.remove(float_index);
        self.normalize();
        true
    }

    /// Transfer one floating station to another OS-window space, preserving
    /// its relative rectangle, constraints, pin, visibility, spec, and
    /// therefore its `PaneId`-keyed retained runner. The destination gives it
    /// its own topmost z-order because float order is scoped to a window.
    pub fn tear_out_floating_pane(
        &mut self,
        pane: PaneId,
        destination: &mut SpaceBlueprint,
    ) -> Result<(), BlueprintViolation> {
        // Check the destination before taking anything from the source. A
        // rejected tear-out must leave the originating pane and its runner
        // lookup reachable where they started.
        if destination.panes.iter().any(|spec| spec.id == pane)
            || destination.station_ids().contains(&pane)
        {
            return Err(BlueprintViolation::DuplicatePaneSpec(pane));
        }
        let Some((spec, floating)) = self.take_floating_pane(pane) else {
            return Err(BlueprintViolation::MissingPaneSpec {
                space: self.id.clone(),
                pane,
            });
        };
        destination.insert_transferred_floating(spec, floating)
    }

    /// Remove a floating pane's spec and station together for a window move.
    /// Unlike [`Self::take_pane`], callers also receive the persisted float
    /// geometry rather than discarding it.
    pub fn take_floating_pane(&mut self, pane: PaneId) -> Option<(PaneSpec, FloatingPane)> {
        let float_index = self.floating.iter().position(|item| item.pane == pane)?;
        let spec_index = self.panes.iter().position(|item| item.id == pane)?;
        let floating = self.floating.remove(float_index);
        let spec = self.panes.remove(spec_index);
        self.normalize();
        Some((spec, floating))
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
            constraints: FloatSizeConstraints::default(),
            z: self
                .floating
                .iter()
                .map(|item| item.z)
                .max()
                .unwrap_or(0)
                .saturating_add(1),
            pinned: false,
            visible: true,
        });
        Ok(())
    }

    fn insert_transferred_floating(
        &mut self,
        spec: PaneSpec,
        mut floating: FloatingPane,
    ) -> Result<(), BlueprintViolation> {
        if spec.id != floating.pane || self.panes.iter().any(|pane| pane.id == spec.id) {
            return Err(BlueprintViolation::DuplicatePaneSpec(spec.id));
        }
        if self.station_ids().contains(&spec.id) {
            return Err(BlueprintViolation::DuplicatePaneStation(spec.id));
        }
        floating.z = self
            .floating
            .iter()
            .map(|item| item.z)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.panes.push(spec);
        self.floating.push(floating);
        Ok(())
    }

    fn normalize_float_z(&mut self) {
        self.floating.sort_by_key(|item| (item.z, item.pane.0));
        for (index, float) in self.floating.iter_mut().enumerate() {
            float.z = index as u32 + 1;
        }
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
