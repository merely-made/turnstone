// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SwatchPane {
    pub fn new(preset: ProjectionPreset) -> Self {
        Self::new_in(None, preset)
    }

    pub(crate) fn new_in(
        mount: Option<cambium::RetainedMount>,
        mut preset: ProjectionPreset,
    ) -> Self {
        let dom: DomHandle = mount
            .as_ref()
            .map(cambium::RetainedMount::dom)
            .unwrap_or_else(|| Rc::new(RefCell::new(ScriptedDom::new())));
        if let Some(mount) = &mount {
            preset.leaf_key = dom.borrow().opaque_id(mount.node());
        }
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
        let runner = SwatchRunner::new_at(
            dom.clone(),
            mount
                .as_ref()
                .map_or_else(|| dom.borrow().document(), cambium::RetainedMount::node),
            swatch_view as fn(&SwatchState) -> SwatchView,
            state,
        );
        Self {
            mount: mount.clone(),
            preset,
            sections: Vec::new(),
            dom,
            runner,
            registry: LeafRegistry::new(),
            rendered: RenderedLeaves::new(),
            last_hover: None,
            layout: crate::ui::RetainedLayout::new_at(
                mount.as_ref().map(cambium::RetainedMount::node),
            ),
        }
    }

    pub(crate) fn inline_focus(&self) -> Option<genet_scripted_dom::NodeId> {
        self.runner.focus()
    }

    pub(crate) fn focus_inline(&mut self, node: genet_scripted_dom::NodeId) -> bool {
        let Some(mount) = &self.mount else {
            return false;
        };
        if !cambium_rootstock::WindowDom::new(&self.dom.borrow(), mount.node()).contains(node) {
            return false;
        }
        self.runner.set_focus(Some(node));
        true
    }

    pub(crate) fn inline_selector(
        &self,
        selector: &taproot::Selector,
    ) -> Result<Option<(f32, f32)>, &'static str> {
        self.layout.selector_point(&self.dom.borrow(), selector)
    }

    pub(crate) fn key_inline(&mut self, event: cambium::KeyEvent) -> Vec<SwatchIntent> {
        let _: Vec<()> = self.runner.dispatch_key(event);
        let mut out = Vec::new();
        self.runner
            .update(|state| out = std::mem::take(&mut state.pending));
        out
    }

    pub(crate) fn blur_inline(&mut self) {
        self.runner.set_focus(None);
    }

    pub(crate) fn inline_accepts_text(&self) -> bool {
        self.layout
            .editable_focus(&self.dom.borrow(), self.runner.focus())
    }

    pub(crate) fn inline_cursor_area(&self) -> Option<(f32, f32, f32, f32)> {
        self.layout
            .cursor_area(&self.dom.borrow(), self.runner.focus())
    }

    pub(crate) fn inline_mount(&self) -> Option<cambium::RetainedMount> {
        self.mount.clone()
    }

    pub(crate) fn unbind_inline(&mut self) {
        self.layout.unbind_inline();
    }

    pub(crate) fn bind_inline(&mut self, layout: &crate::ui::InlineLayoutHandle) {
        if let Some(mount) = &self.mount {
            self.layout.bind_inline(layout, mount);
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
        let relations: Vec<cambium::GraphCanvasRelation<Uuid>> = model
            .relations
            .iter()
            .map(|relation| cambium::GraphCanvasRelation {
                id: format!("{}:{}:{}", relation.kind, relation.from, relation.to),
                from: relation.from,
                to: relation.to,
                kind: relation.kind.to_string(),
                label: relation.label.clone(),
                route: Vec::new(),
                visible: true,
                emphasized: false,
                link_key: None,
                marks: Default::default(),
                strength: 1.0,
            })
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
            state.swatch.relations = relations;
            state.swatch.relation_lines = RELATION_LINES
                .iter()
                .map(|(kind, line)| (kind.to_string(), *line))
                .collect();
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
    pub fn scene(&mut self, w: u32, h: u32, scale: f32) -> netrender::Scene {
        self.layout.set_scale(scale);
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
        if !sel.matches_surface(self.preset.id) {
            return None;
        }
        let dom = self.dom.borrow();
        self.layout
            .first_selector_point(&dom, sel)
            .map(|(x, y)| (rect[0] + x, rect[1] + y))
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
            lines.push(apparatus_core::InspectionLine {
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
        let mut tree = project_inspection(&apparatus_core::Inspection { lines });
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
