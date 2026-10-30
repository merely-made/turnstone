// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The swatch as a customizable projection (design_docs/
//! 2026-07-20_gloss_composite_pane.md).
//!
//! [`SwatchPane`] is the ONE retained pane over any [`ProjectionPreset`]; the
//! Gloss minimap and the Overmap are two presets of it. A preset carries what
//! is pane-specific as DATA plus one gather fn, including what a node click
//! MEANS ([`SwatchActivate`] per node), so a new swatch consumer needs no
//! handler code. Composed sections come from the pane's own leaf, not the
//! preset: the swatch is the preset, the sections are per-pane config.

mod inspection;
mod presets;
use inspection::project_inspection;
use presets::state_color;
pub use presets::{GLOSS_MINIMAP, OVERMAP_LINEAGE, ProjectionPreset};

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, GraphCanvasEdge, GraphCanvasEvent,
    GraphCanvasNode, GraphCanvasSubgraph, GraphCanvasSwatch, graph_canvas,
};
use genet_scripted_dom::{NodeId, ScriptedDom};
use layout_dom_api::LayoutDom;
use mere::canvas::NodeState;
use mere::canvas::palette;
use sprigging::{ColorF, LeafRegistry, RenderedLeaves};
use uuid::Uuid;

use crate::app::App;
use crate::content::NodeContent;
use crate::overmap;

/// Inset (px) of the swatch from the pane edges.
const SWATCH_PAD: f32 = 12.0;
/// When a swatch preset composes sections, the swatch takes this fraction of
/// the pane height and the sections stack below (the gloss-composite split).
const SWATCH_FRACTION: f32 = 0.55;

/// What activating (clicking) a swatch node means — data on the node, so the
/// one pane needs no per-preset handler code and the shell lowers each variant
/// through the ordinary spine.
#[derive(Clone, Debug, PartialEq)]
pub enum SwatchActivate {
    /// Land on this address (`Action::OpenAddress`).
    Open(String),
    /// Adopt this session (`Action::SwitchSession`).
    Switch(crate::panes::SessionId),
    /// Recover a removed node by its ORIGINAL id
    /// (`Action::RecoverDeletedNode`) — a composed Removed row's click.
    Recover(uuid::Uuid),
}

/// What a swatch-pane interaction asks of the shell.
#[derive(Clone, Debug, PartialEq)]
pub enum SwatchIntent {
    /// A node was activated; its meaning rode the node as data.
    Activate(SwatchActivate),
    /// The Expand chip: jump to the app's full view (the canvas).
    Expand,
}

/// One gathered node: identity, normalized position, palette state, label,
/// probe key, and what activating it means.
#[derive(Clone, Debug, PartialEq)]
pub struct SwatchNode {
    pub id: Uuid,
    /// Normalized `0..1` scene position.
    pub position: (f32, f32),
    /// The palette state (node color carries node identity everywhere).
    pub state: NodeState,
    pub label: String,
    /// The stable `data-key` a probe / `click-node` targets by.
    pub key: Option<String>,
    pub activate: Option<SwatchActivate>,
}

/// The gathered projection: what the swatch renders this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SwatchModel {
    pub nodes: Vec<SwatchNode>,
    pub edges: Vec<(Uuid, Uuid)>,
    /// Kinded links, drawn in place of `edges` when present, each kind with
    /// its own line (the overmap's fork and pick lineage).
    pub relations: Vec<SwatchRelation>,
    pub selected: Option<Uuid>,
}

/// One kinded link between two gathered nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct SwatchRelation {
    pub from: Uuid,
    pub to: Uuid,
    /// The relation cell's `data-kind`, such as `picked-from`.
    pub kind: &'static str,
    /// Its accessible words, such as "Draft picked from home".
    pub label: String,
}

/// The line each kinded link is painted with: a fork solid, a pick dashed,
/// containment dotted.
const RELATION_LINES: [(&str, sprigging::RelationLine); 3] = [
    ("forked-from", sprigging::RelationLine::Solid),
    ("picked-from", sprigging::RelationLine::Dashed),
    ("contains", sprigging::RelationLine::Dotted),
];

struct SwatchState {
    swatch: GraphCanvasSwatch<Uuid, NodeState>,
    /// Each node's activation meaning, by id (data off the gather).
    activate_of: HashMap<Uuid, SwatchActivate>,
    pending: Vec<SwatchIntent>,
    viewport_w: f32,
    viewport_h: f32,
    /// The composed sections (title + rows) rendered below the swatch, gathered
    /// from the preset's providers. Empty for a fill-the-pane swatch.
    sections: Vec<(&'static str, &'static str, Vec<crate::sections::SectionRow>)>,
    /// The swatch area's height (px): the whole pane when there are no
    /// sections, the top fraction when there are. The sections stack below it.
    swatch_h: f32,
}

type SwatchView = Box<dyn AnyView<SwatchState, (), GenetCtx, GenetElement>>;
type SwatchRunner = GenetAppRunner<SwatchState, fn(&SwatchState) -> SwatchView, SwatchView, ()>;

/// The one retained swatch pane, parameterized by its [`ProjectionPreset`]:
/// a cambium runner + the custom-paint leaf pipeline, `!Send`, persistent
/// between the frame that draws it and the click that hits it. Gloss and the
/// Overmap are two instances of this.
pub struct SwatchPane {
    preset: ProjectionPreset,
    /// The composed section providers, set by the host from THIS pane's leaf
    /// config each frame. Empty = the swatch fills the pane.
    sections: Vec<crate::sections::SectionProvider>,
    dom: DomHandle,
    runner: SwatchRunner,
    registry: LeafRegistry<u64>,
    rendered: RenderedLeaves,
    /// The dom node the pointer last hovered, for Enter/Leave transitions
    /// (the hover contract is edge-triggered, like the browser's).
    last_hover: Option<NodeId>,
    /// Kept across frames. Rebuilding a layout per paint re-cascaded and
    /// re-shaped the whole document to draw an unchanged screen; see
    /// [`crate::ui::RetainedLayout`] for the measurement.
    layout: crate::ui::RetainedLayout,
    // Drop the runner and its widget resources before final mount custody.
    mount: Option<cambium::RetainedMount>,
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[path = "swatch_view.rs"]
mod swatch_view;
use swatch_view::swatch_view;
#[path = "swatch_pane_new.rs"]
mod swatch_pane_new;
