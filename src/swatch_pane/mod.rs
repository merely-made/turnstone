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

mod presets;
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
    pub selected: Option<Uuid>,
}

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

fn swatch_view(state: &SwatchState) -> SwatchView {
    // The canvas owns hover emphasis; this pane keeps only what the graph
    // decides (selection, arriving through `state.swatch`) and the intents it
    // has to act on.
    let swatch = graph_canvas(
        &state.swatch,
        |state: &mut SwatchState, event: GraphCanvasEvent<Uuid>| match event {
            GraphCanvasEvent::Activate(id) => {
                if let Some(activate) = state.activate_of.get(&id) {
                    state.pending.push(SwatchIntent::Activate(activate.clone()));
                }
            }
            GraphCanvasEvent::Expand => state.pending.push(SwatchIntent::Expand),
            GraphCanvasEvent::Drag(_)
            | GraphCanvasEvent::RelationActivate(_)
            | GraphCanvasEvent::Pan { .. }
            | GraphCanvasEvent::Zoom { .. } => {}
        },
    );
    // The swatch fills the top (its own leaf is sized to `swatch_h` in sync).
    let mut children: Vec<SwatchView> = vec![Box::new(
        cambium::el::<_, SwatchState, ()>("div", swatch).attr(
            "style",
            format!("position: absolute; left: {SWATCH_PAD}px; top: {SWATCH_PAD}px;"),
        ),
    )];

    // Composed sections stack below the swatch in a scrollable column (the
    // gloss-composite: the minimap plus, say, the recycle bin's Removed rows).
    if !state.sections.is_empty() {
        let mut section_kids: Vec<SwatchView> = Vec::new();
        for (_id, title, rows) in &state.sections {
            section_kids.push(Box::new(
                cambium::el::<_, SwatchState, ()>("div", title.to_string()).attr(
                    "style",
                    "color: #7d8590; padding: 6px 12px 2px; font-size: 11px;",
                ),
            ));
            if rows.is_empty() {
                section_kids.push(Box::new(
                    cambium::el::<_, SwatchState, ()>("div", "nothing here".to_string()).attr(
                        "style",
                        "color: #484f58; padding: 2px 12px; font-size: 12px;",
                    ),
                ));
            } else {
                for row in rows {
                    // The row's activation rides it as DATA (the swatch node's
                    // rule): the click handler pushes the intent the provider
                    // declared, so a new provider needs no handler code here.
                    // `section-row` is the probe class a receipt addresses.
                    let row_view = cambium::el::<_, SwatchState, ()>("div", row.text.clone())
                        .attr("class", "section-row")
                        .attr("data-diagnostic-id", row.id.clone().unwrap_or_default())
                        .attr(
                            "style",
                            "color: #c9d1d9; padding: 2px 12px; font-size: 12px;",
                        );
                    let Some(activate) = row.activate.clone() else {
                        // Inspection and other read-only sections install no
                        // activation handler. Actions remain provider data.
                        section_kids.push(Box::new(row_view));
                        continue;
                    };
                    section_kids.push(Box::new(cambium::on_click(
                        row_view,
                        move |state: &mut SwatchState, _click: cambium::PointerClick| {
                            match &activate {
                                crate::sections::SectionActivate::Open(url) => {
                                    state.pending.push(SwatchIntent::Activate(
                                        SwatchActivate::Open(url.clone()),
                                    ));
                                },
                                crate::sections::SectionActivate::Recover(id) => {
                                    state
                                        .pending
                                        .push(SwatchIntent::Activate(SwatchActivate::Recover(*id)));
                                },
                            }
                        },
                    )));
                }
            }
        }
        children.push(Box::new(
            cambium::el::<_, SwatchState, ()>("div", section_kids)
                .attr("data-diagnostics-region", if state.sections.iter().any(|(id, _, _)| *id == "diagnostics") { "true" } else { "" })
                .attr(
                "style",
                format!(
                    "position: absolute; left: 0px; top: {}px; width: {}px; height: {}px; overflow-y: auto;",
                    state.swatch_h,
                    state.viewport_w,
                    (state.viewport_h - state.swatch_h).max(0.0),
                ),
            ),
        ));
    }

    Box::new(
        cambium::el::<_, SwatchState, ()>("div", children)
            .attr("class", "pane")
            .attr(
                "style",
                format!(
                    "position: relative; width: {}px; height: {}px;",
                    state.viewport_w, state.viewport_h
                ),
            ),
    )
}

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
}

impl SwatchPane {
    pub fn new(preset: ProjectionPreset) -> Self {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = SwatchState {
            swatch: GraphCanvasSwatch::new(
                preset.leaf_key,
                GraphCanvasSubgraph {
                    nodes: Vec::new(),
                    edges: Vec::new(),
                },
            )
            .with_label(preset.label)
            .with_node_labels(preset.node_labels)
            .with_expand(preset.expand),
            activate_of: HashMap::new(),
            pending: Vec::new(),
            viewport_w: 0.0,
            viewport_h: 0.0,
            sections: Vec::new(),
            swatch_h: 0.0,
        };
        let runner = SwatchRunner::new(
            dom.clone(),
            swatch_view as fn(&SwatchState) -> SwatchView,
            state,
        );
        Self {
            preset,
            sections: Vec::new(),
            dom,
            runner,
            registry: LeafRegistry::new(),
            rendered: RenderedLeaves::new(),
            last_hover: None,
            layout: crate::ui::RetainedLayout::new(),
        }
    }

    /// Set the composed section providers for this pane (the host resolves
    /// them from the leaf's `PaneComposition` each frame). Cheap: providers are
    /// `Copy` descriptors, and the rows themselves are gathered in `sync`.
    pub fn set_sections(&mut self, sections: Vec<crate::sections::SectionProvider>) {
        self.sections = sections;
    }

    /// Refresh from app truth at the pane's size: run the preset's gather,
    /// project it into the swatch, re-register the paint leaf.
    pub fn sync(&mut self, app: &App, pane_w: f32, pane_h: f32) {
        let model = (self.preset.gather)(app);
        let mut activate_of = HashMap::new();
        let nodes: Vec<GraphCanvasNode<Uuid, NodeState>> = model
            .nodes
            .iter()
            .map(|node| {
                if let Some(activate) = &node.activate {
                    activate_of.insert(node.id, activate.clone());
                }
                GraphCanvasNode {
                    id: node.id,
                    kind: node.state,
                    position: node.position,
                    label: node.label.clone(),
                    key: node.key.clone(),
                }
            })
            .collect();
        let edges = model
            .edges
            .iter()
            .map(|&(from, to)| GraphCanvasEdge { from, to })
            .collect();

        // Composed sections shrink the swatch to the top fraction; without
        // them it fills the pane (the Overmap's shape, unchanged).
        let sections: Vec<(&'static str, &'static str, Vec<crate::sections::SectionRow>)> = self
            .sections
            .iter()
            .map(|p| (p.id, p.title, (p.gather)(app)))
            .collect();
        let swatch_h = if sections.is_empty() {
            pane_h
        } else {
            (pane_h * SWATCH_FRACTION).max(64.0)
        };
        let sw = ((pane_w - 2.0 * SWATCH_PAD).max(32.0)) as u32;
        let sh = ((swatch_h - 2.0 * SWATCH_PAD).max(32.0)) as u32;
        self.runner.update(|state| {
            state.swatch.graph = GraphCanvasSubgraph { nodes, edges };
            state.swatch.selected = model.selected;
            state.swatch.width = sw;
            state.swatch.height = sh;
            state.activate_of = activate_of;
            state.viewport_w = pane_w;
            state.viewport_h = pane_h;
            state.sections = sections;
            state.swatch_h = swatch_h;
        });
        self.registry.insert(
            self.preset.leaf_key,
            Box::new(self.runner.state().swatch.paint_leaf(state_color)),
        );
    }

    /// The pane's scene at its size (the shared cambium leaf pipeline).
    pub fn scene(&mut self, w: u32, h: u32) -> netrender::Scene {
        self.layout.scene_with_leaves(
            &mut self.dom.borrow_mut(),
            crate::ui::CAMBIUM_SHEET,
            w,
            h,
            &mut self.registry,
            &mut self.rendered,
        )
    }

    /// Route a pointer MOVE at pane-local `(x, y)`: Enter/Leave transitions;
    /// returns whether the hover target changed (the host redraws on true).
    pub fn hover(&mut self, x: f32, y: f32, w: u32, h: u32) -> bool {
        let hit = self.layout.hit_test(
            &mut self.dom.borrow_mut(),
            crate::ui::CAMBIUM_SHEET,
            w,
            h,
            x,
            y,
        );
        if hit == self.last_hover {
            return false;
        }
        if let Some(prev) = self.last_hover {
            let _ = self.runner.dispatch_hover(
                prev,
                cambium::HoverEvent::new(cambium::HoverPhase::Leave, (x, y), (x, y)),
            );
        }
        if let Some(node) = hit {
            let _ = self.runner.dispatch_hover(
                node,
                cambium::HoverEvent::new(cambium::HoverPhase::Enter, (x, y), (x, y)),
            );
        }
        self.last_hover = hit;
        true
    }

    /// The pointer left this pane: deliver the pending Leave so emphasis clears.
    pub fn hover_leave(&mut self) -> bool {
        let Some(prev) = self.last_hover.take() else {
            return false;
        };
        let _ = self.runner.dispatch_hover(
            prev,
            cambium::HoverEvent::new(cambium::HoverPhase::Leave, (0.0, 0.0), (0.0, 0.0)),
        );
        true
    }

    /// Route a click at pane-local `(x, y)`; drain the recorded intents.
    pub fn click(&mut self, x: f32, y: f32, w: u32, h: u32) -> Vec<SwatchIntent> {
        let hit = self.layout.hit_test(
            &mut self.dom.borrow_mut(),
            crate::ui::CAMBIUM_SHEET,
            w,
            h,
            x,
            y,
        );
        if let Some(node) = hit {
            let _ = self
                .runner
                .dispatch_click(node, cambium::PointerClick::at((x, y)));
        }
        let mut drained = Vec::new();
        self.runner
            .update(|state| drained = std::mem::take(&mut state.pending));
        drained
    }

    /// Resolve a probe selector within this pane's DOM (nodes carry their
    /// stable identity as `data-key`).
    pub fn resolve(&self, sel: &taproot::Selector, rect: [f32; 4]) -> Option<(f32, f32)> {
        let dom = self.dom.borrow();
        let surfaces = [taproot::ProbeSurface {
            name: self.preset.id,
            dom: &dom,
            rect,
            sheet: crate::ui::CAMBIUM_SHEET,
        }];
        taproot::resolve(&surfaces, sel).map(|h| h.point)
    }

    /// The promoted read-only Diagnostics section, from its producer identities
    /// and the actual retained visible DOM geometry. Active rows and swatch
    /// leaves remain outside this subtree and gain no invented actions/focus.
    pub(crate) fn diagnostic_tree(
        &self,
        pane: crate::panes::PaneId,
        placement: crate::surface::Rect,
    ) -> Option<uxtree::UxTree> {
        let rows = self
            .runner
            .state()
            .sections
            .iter()
            .find(|(id, _, _)| *id == "diagnostics")?
            .2
            .as_slice();
        let trusted: HashMap<_, _> = rows
            .iter()
            .filter_map(|row| Some((row.id.as_deref()?, row.text.as_str())))
            .collect();
        let dom = self.dom.borrow();
        let projection = self.layout.document_projection(&dom, None)?;
        let namespace = layout_dom_api::Namespace::default();
        let mut pending = vec![dom.document()];
        let mut region = None;
        let mut lines = Vec::new();
        let mut bounds = HashMap::new();
        while let Some(node) = pending.pop() {
            pending.extend(dom.dom_children(node));
            let Some(visible) = self.layout.visible_rect(&dom, node) else {
                continue;
            };
            let shown = projection
                .nodes()
                .iter()
                .any(|semantic| semantic.id.get() == dom.opaque_id(node) && !semantic.state.hidden);
            if !shown {
                continue;
            }
            let window_rect = accesskit::Rect::new(
                f64::from(placement.x + visible.0),
                f64::from(placement.y + visible.1),
                f64::from(placement.x + visible.0 + visible.2),
                f64::from(placement.y + visible.1 + visible.3),
            );
            if dom.attribute(node, &namespace, &"data-diagnostics-region".into()) == Some("true") {
                region = Some(window_rect);
            }
            let Some(identity) = dom.attribute(node, &namespace, &"data-diagnostic-id".into())
            else {
                continue;
            };
            let Some(text) = trusted.get(identity) else {
                continue;
            };
            let id = format!("turnstone/gloss/{}/{}", pane.0, identity);
            bounds.insert(uxtree::node_id_for_path(&id), window_rect);
            lines.push(apparatus::InspectionLine {
                id,
                text: (*text).to_owned(),
            });
        }
        let region = region?;
        // DOM traversal uses a stack; restore the producer's stable display order.
        lines.sort_by_key(|line| {
            rows.iter().position(|row| {
                row.id
                    .as_ref()
                    .is_some_and(|id| line.id == format!("turnstone/gloss/{}/{}", pane.0, id))
            })
        });
        let mut tree = apparatus::project_inspection(&apparatus::Inspection { lines });
        let previous_root = tree.root;
        tree.root = uxtree::node_id_for_path(&format!("turnstone/gloss/{}/diagnostics", pane.0));
        for (id, semantic) in &mut tree.nodes {
            if *id == previous_root {
                *id = tree.root;
                semantic.set_bounds(region);
            } else if let Some(rect) = bounds.get(id) {
                semantic.set_bounds(*rect);
            }
        }
        Some(tree)
    }

    /// Borrow this pane's DOM for the shared driver's `with_surfaces`.
    pub fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.dom.borrow()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Update;

    fn gloss_pane_on_sample_graph() -> (SwatchPane, App) {
        let mut app = App::test_stub();
        *app.graph_runtimes.active_canvas_mut() = mere::canvas::Canvas::with_sample_graph();
        let mut pane = SwatchPane::new(GLOSS_MINIMAP);
        pane.sync(&app, 480.0, 400.0);
        (pane, app)
    }

    fn overmap_pane_on_fork_pair() -> (SwatchPane, App, crate::panes::SessionId) {
        let mut app = App::test_stub();
        let donor = app.session_id;
        let mut donor_m = pandect::GraphSessionManifest::new(
            donor,
            crate::panes::GraphId::from_uuid(uuid::Uuid::from_u128(0xd0)),
        );
        donor_m.display_name = Some("home".to_string());
        app.sessions.insert(donor_m);
        let fork = crate::panes::SessionId::new();
        let mut fork_m = pandect::GraphSessionManifest::new(
            fork,
            crate::panes::GraphId::from_uuid(uuid::Uuid::from_u128(0xf0)),
        );
        fork_m.parent_session = Some(donor);
        app.sessions.insert(fork_m);
        app.session_id = fork;
        let mut pane = SwatchPane::new(OVERMAP_LINEAGE);
        pane.sync(&app, 480.0, 400.0);
        (pane, app, donor)
    }

    /// The leaf pipeline end to end, headless, for the minimap preset — the
    /// pipeline every leaf consumer reuses (the old GlossPane's charter).
    #[test]
    fn minimap_preset_paints_through_the_pipeline() {
        let (mut pane, _app) = gloss_pane_on_sample_graph();
        assert!(!pane.runner.state().swatch.graph.nodes.is_empty());
        assert!(
            !pane.runner.state().swatch.graph.edges.is_empty(),
            "edge endpoints must match back to nodes bit-exactly"
        );
        assert!(
            !pane.runner.state().swatch.show_labels,
            "minimap stays bare"
        );
        let _scene = pane.scene(480, 400);
        assert!(
            pane.rendered
                .get(GLOSS_MINIMAP.leaf_key)
                .is_some_and(|c| !c.is_empty()),
            "the minimap leaf must render paint commands at its laid-out box"
        );
    }

    /// A minimap node click drains the Open activation for that node's url —
    /// the intent riding the node as preset data.
    #[test]
    fn clicking_a_minimap_node_activates_open() {
        let (mut pane, _app) = gloss_pane_on_sample_graph();
        let key = pane.runner.state().swatch.graph.nodes[0]
            .key
            .clone()
            .expect("every minimap node carries its url as a key");
        let (x, y) = pane
            .resolve(
                &taproot::Selector::class("graph-canvas-swatch-node")
                    .with_attr("data-key", &key),
                [0.0, 0.0, 480.0, 400.0],
            )
            .expect("the node must resolve by its data-key");
        let intents = pane.click(x, y, 480, 400);
        assert!(
            matches!(&intents[..], [SwatchIntent::Activate(SwatchActivate::Open(url))] if *url == key),
            "a node click drains Open for that node's url, got {intents:?}"
        );
    }

    fn diagnostic_fixture() -> (SwatchPane, App) {
        let mut app = App::test_stub();
        app.diagnostic_inspection = Some(Ok(apparatus::Inspection {
            lines: (0..30)
                .map(|index| apparatus::InspectionLine {
                    id: format!("apparatus/fixture/record/{index}"),
                    text: format!("Diagnostic record {index}"),
                })
                .collect(),
        }));
        let mut pane = SwatchPane::new(GLOSS_MINIMAP);
        pane.set_sections(vec![crate::sections::DIAGNOSTICS_SECTION]);
        pane.sync(&app, 480.0, 400.0);
        pane.scene(480, 400);
        (pane, app)
    }

    fn diagnostic_dom_node(pane: &SwatchPane, identity: &str) -> NodeId {
        let dom = pane.dom.borrow();
        let mut pending = vec![dom.document()];
        while let Some(node) = pending.pop() {
            pending.extend(dom.dom_children(node));
            if dom.attribute(node, &Default::default(), &"data-diagnostic-id".into())
                == Some(identity)
            {
                return node;
            }
        }
        panic!("missing diagnostic DOM row {identity}");
    }

    #[test]
    fn diagnostic_rows_keep_producer_identity_and_retained_bounds_without_activation() {
        let (mut pane, mut app) = diagnostic_fixture();
        let placement = crate::surface::Rect::new(100.0, 40.0, 480.0, 400.0);
        let id = uxtree::node_id_for_path("turnstone/gloss/7/apparatus/fixture/record/0");
        let tree = pane
            .diagnostic_tree(crate::panes::PaneId(7), placement)
            .unwrap();
        let (_, row) = tree
            .nodes
            .iter()
            .find(|(candidate, _)| *candidate == id)
            .unwrap();
        assert_eq!(row.label(), Some("Diagnostic record 0"));
        assert!(tree.nodes.iter().all(|(_, node)| {
            !node.supports_action(accesskit::Action::Click)
                && !node.supports_action(accesskit::Action::Focus)
        }));
        let dom_node = diagnostic_dom_node(&pane, "apparatus/fixture/record/0");
        let visible = pane
            .layout
            .visible_rect(&pane.dom.borrow(), dom_node)
            .unwrap();
        assert_eq!(
            row.bounds(),
            Some(accesskit::Rect::new(
                f64::from(placement.x + visible.0),
                f64::from(placement.y + visible.1),
                f64::from(placement.x + visible.0 + visible.2),
                f64::from(placement.y + visible.1 + visible.3),
            ))
        );
        assert!(
            pane.click(
                visible.0 + visible.2 / 2.0,
                visible.1 + visible.3 / 2.0,
                480,
                400
            )
            .is_empty()
        );
        let Some(Ok(inspection)) = app.diagnostic_inspection.as_mut() else {
            unreachable!()
        };
        inspection.lines[0].text = "Diagnostic record updated".into();
        inspection.lines.insert(
            0,
            apparatus::InspectionLine {
                id: "apparatus/fixture/new".into(),
                text: "New earlier row".into(),
            },
        );
        pane.sync(&app, 480.0, 400.0);
        pane.scene(480, 400);
        let updated = pane
            .diagnostic_tree(crate::panes::PaneId(7), placement)
            .unwrap();
        assert_eq!(
            updated
                .nodes
                .iter()
                .find(|(candidate, _)| *candidate == id)
                .unwrap()
                .1
                .label(),
            Some("Diagnostic record updated")
        );
        let another = pane
            .diagnostic_tree(crate::panes::PaneId(8), placement)
            .unwrap();
        assert!(
            !another.nodes.iter().any(|(candidate, _)| *candidate == id),
            "pane identity scopes shared inspection rows"
        );
    }

    #[test]
    fn diagnostic_subtree_omits_clipped_hidden_and_removed_rows() {
        use layout_dom_api::{LayoutDomMut, QualName};
        let (mut pane, _app) = diagnostic_fixture();
        let placement = crate::surface::Rect::new(0.0, 0.0, 480.0, 400.0);
        let id = uxtree::node_id_for_path("turnstone/gloss/7/apparatus/fixture/record/0");
        let tree = pane
            .diagnostic_tree(crate::panes::PaneId(7), placement)
            .unwrap();
        let clipped = uxtree::node_id_for_path("turnstone/gloss/7/apparatus/fixture/record/29");
        assert!(
            !tree
                .nodes
                .iter()
                .any(|(candidate, _)| *candidate == clipped),
            "the below-container row is absent, not assigned invented bounds"
        );
        let root_bounds = tree
            .nodes
            .iter()
            .find(|(candidate, _)| *candidate == tree.root)
            .unwrap()
            .1
            .bounds()
            .unwrap();
        let dom_node = diagnostic_dom_node(&pane, "apparatus/fixture/record/0");
        pane.dom.borrow_mut().set_attribute(
            dom_node,
            QualName::new(None, "".into(), "style".into()),
            &format!(
                "position:absolute;left:0;top:{}px;width:150px;height:20px;padding:0",
                root_bounds.height() - 5.0
            ),
        );
        pane.scene(480, 400);
        let partial = pane
            .diagnostic_tree(crate::panes::PaneId(7), placement)
            .unwrap();
        let bounds = partial
            .nodes
            .iter()
            .find(|(candidate, _)| *candidate == id)
            .unwrap()
            .1
            .bounds()
            .unwrap();
        assert!(
            bounds.height() > 0.0 && bounds.height() < 20.0,
            "ancestor clipping supplies the actual visible part: {bounds:?}"
        );
        pane.dom.borrow_mut().set_attribute(
            dom_node,
            QualName::new(None, "".into(), "aria-hidden".into()),
            "true",
        );
        pane.scene(480, 400);
        assert!(
            !pane
                .diagnostic_tree(crate::panes::PaneId(7), placement)
                .unwrap()
                .nodes
                .iter()
                .any(|(candidate, _)| *candidate == id)
        );
        pane.set_sections(Vec::new());
        let app = App::test_stub();
        pane.sync(&app, 480.0, 400.0);
        pane.scene(480, 400);
        assert!(
            pane.diagnostic_tree(crate::panes::PaneId(7), placement)
                .is_none(),
            "removing the section retires the whole subtree"
        );
    }

    /// The overmap preset renders both sessions + the lineage edge, labels
    /// on, and a session-node click drains the Switch activation.
    #[test]
    fn overmap_preset_renders_lineage_and_switches() {
        let (mut pane, _app, donor) = overmap_pane_on_fork_pair();
        assert_eq!(pane.runner.state().swatch.graph.nodes.len(), 2);
        assert_eq!(pane.runner.state().swatch.graph.edges.len(), 1);
        assert!(pane.runner.state().swatch.show_labels, "identity labels on");
        assert_eq!(
            pane.runner.state().swatch.selected,
            Some(uuid::Uuid::from_u128(0xf0)),
            "the current session's container is selected"
        );
        let _scene = pane.scene(480, 400);
        assert!(
            pane.rendered
                .get(OVERMAP_LINEAGE.leaf_key)
                .is_some_and(|c| !c.is_empty())
        );
        let (x, y) = pane
            .resolve(
                &taproot::Selector::class("graph-canvas-swatch-node")
                    .with_attr("data-key", &donor.0.to_string()),
                [0.0, 0.0, 480.0, 400.0],
            )
            .expect("the donor session node resolves by its data-key");
        let intents = pane.click(x, y, 480, 400);
        assert!(
            matches!(&intents[..], [SwatchIntent::Activate(SwatchActivate::Switch(id))] if *id == donor),
            "a session-node click drains Switch for that session, got {intents:?}"
        );
    }

    /// Hover transitions set and clear the emphasis, preset-agnostically.
    ///
    /// The emphasis is the canvas component's own state, so this reads it
    /// where it is now observable: the rendered node's class. The pane no
    /// longer holds a hover field to assert against, which is the point.
    #[test]
    fn hovering_sets_and_clears_emphasis() {
        let (mut pane, _app, donor) = overmap_pane_on_fork_pair();
        let selector = taproot::Selector::class("graph-canvas-swatch-node")
            .with_attr("data-key", &donor.0.to_string());
        let (x, y) = pane
            .resolve(&selector, [0.0, 0.0, 480.0, 400.0])
            .expect("the donor session node resolves");
        let key = donor.0.to_string();
        let hovered_class = |pane: &SwatchPane| {
            use layout_dom_api::LayoutDom;
            let dom = pane.dom.borrow();
            dom.all_with_class(dom.document(), "graph-canvas-swatch-node")
                .into_iter()
                .filter(|node| {
                    dom.attribute(
                        *node,
                        &layout_dom_api::Namespace::from(""),
                        &layout_dom_api::LocalName::from("data-key"),
                    ) == Some(key.as_str())
                })
                .any(|node| {
                    dom.attribute(
                        node,
                        &layout_dom_api::Namespace::from(""),
                        &layout_dom_api::LocalName::from("class"),
                    )
                    .is_some_and(|class| class.contains("hovered"))
                })
        };

        assert!(pane.hover(x, y, 480, 400), "entering the node is a change");
        assert!(hovered_class(&pane), "the canvas renders its own emphasis");
        assert!(!pane.hover(x, y, 480, 400), "same target, no re-dispatch");
        assert!(pane.hover_leave(), "leaving the pane is a change");
        assert!(!hovered_class(&pane), "emphasis cleared");
    }

    /// A recovered content state colors the minimap node open — the preset
    /// gather reads the same lifecycle the canvas colors by.
    #[test]
    fn minimap_nodes_color_by_content_state() {
        let (mut pane, mut app) = gloss_pane_on_sample_graph();
        let id = app
            .graph_runtimes
            .graph()
            .nodes()
            .next()
            .map(|(_, n)| n.id)
            .unwrap();
        app.apply_update(Update::ContentSpawned {
            node: id,
            facts: None,
        });
        pane.sync(&app, 480.0, 400.0);
        let node = pane
            .runner
            .state()
            .swatch
            .graph
            .nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap();
        assert_eq!(node.kind, NodeState::Open, "live content reads open");
    }
}
