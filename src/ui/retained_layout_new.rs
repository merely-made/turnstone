// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl RetainedLayout {
    pub fn new() -> Self {
        Self {
            inline_leaves: HashMap::new(),
            native_sources: HashMap::new(),
            inline_scroll_root: None,
            inline_bar: None,
            root: None,
            mutation_cursor: 0,
            inline: None,
            snapshot: None,
            size: (0, 0),
            scale: 1.0,
            laid_at: 1.0,
            sheet: String::new(),
            rebuilds: 0,
            viewport_scroll: (0.0, 0.0),
            element_scroll: HashMap::new(),
            projections: std::cell::RefCell::new(HashMap::new()),
        }
    }

    pub(crate) fn new_at(root: Option<DomNodeId>) -> Self {
        Self {
            root,
            ..Self::new()
        }
    }

    /// Compose this layout at `scale` physical pixels per layout pixel, the
    /// convention Graphshell's rootstock host keeps: the sheet and DOM are
    /// authored and laid out at `size / scale`, the resulting scene is
    /// rasterized at `scale`, and every point, size and rect this type takes
    /// or returns stays in physical pixels, so no caller divides pointers.
    pub(crate) fn set_scale(&mut self, scale: f32) {
        self.scale = clean_scale(scale);
    }

    pub(crate) fn scale(&self) -> f32 {
        self.scale
    }

    /// The laid-out viewport, in layout pixels.
    pub(super) fn logical(&self) -> (u32, u32) {
        logical_size(self.size, self.scale)
    }

    pub(super) fn physical(&self, (x, y, w, h): NodeRect) -> NodeRect {
        let scale = self.scale;
        (x * scale, y * scale, w * scale, h * scale)
    }

    pub(crate) fn bind_inline(
        &mut self,
        layout: &InlineLayoutHandle,
        mount: &cambium::RetainedMount,
    ) {
        let owner = mount.node();
        let handle = mount.dom();
        let mut dom = handle.borrow_mut();
        let root = dom
            .dom_children(owner)
            .find(|node| !dom.has_class(*node, "turnstone-pane-scrollbars"));
        let bar = dom
            .dom_children(owner)
            .find(|node| dom.has_class(*node, "turnstone-pane-scrollbars"));
        let bar = bar.unwrap_or_else(|| {
            let node = dom.create_element(cambium::html_qual("custom-leaf"));
            dom.set_attribute(
                node,
                cambium::attr_qual("class"),
                "turnstone-pane-scrollbars",
            );
            let key = dom.opaque_id(node).to_string();
            dom.set_attribute(node, cambium::attr_qual("key"), &key);
            dom.set_attribute(
                node,
                cambium::attr_qual("style"),
                "position:absolute;left:0;top:0;width:100%;height:100%;pointer-events:none;",
            );
            dom.append_child(owner, node);
            node
        });
        self.inline_scroll_root = root;
        self.inline_bar = Some(bar);
        self.inline = Some((std::rc::Rc::downgrade(layout), owner));
        self.replace_snapshot(None);
    }

    pub(crate) fn unbind_inline(&mut self) {
        self.inline = None;
        self.replace_snapshot(None);
    }

    /// Every snapshot change retires the projection derived from the old one.
    pub(super) fn replace_snapshot(&mut self, snapshot: Option<LiverySnapshot<DomNodeId>>) {
        self.snapshot = snapshot;
        self.projections.get_mut().clear();
    }

    pub(crate) fn set_native_sources(&mut self, sources: HashMap<u64, Vec<PaintCmd>>) {
        self.native_sources = sources;
    }

    pub(crate) fn native_source_command(
        &self,
        dom: &ScriptedDom,
        owner: DomNodeId,
        key: u64,
        generation: u64,
        source: crate::surface::Rect,
    ) -> Option<PaintCmd> {
        let fragment = self.snapshot.as_ref()?.fragments.get(owner)?;
        let (painted_x, painted_y, _, _) = self.painted_rect(dom, owner)?;
        // The source is a physical window rect; the scene it joins is in
        // layout pixels.
        let scale = self.scale;
        let x = fragment.x + (source.x - painted_x) / scale;
        let y = fragment.y + (source.y - painted_y) / scale;
        Some(PaintCmd::DrawExternalTexture(
            paint_list_api::ExternalTextureItem {
                placement: paint_list_api::CommonPlacement::new(paint_list_api::LayoutRect::new(
                    paint_list_api::LayoutPoint::new(x, y),
                    paint_list_api::LayoutPoint::new(x + source.w / scale, y + source.h / scale),
                )),
                texture_key: key,
                opacity: 1.0,
                content_generation: Some(generation),
            },
        ))
    }

    pub(crate) fn scroll_stamp(&self) -> Vec<(u64, u32, u32)> {
        let mut rows = self
            .element_scroll
            .iter()
            .map(|(node, (x, y))| (node.raw(), x.to_bits(), y.to_bits()))
            .collect::<Vec<_>>();
        rows.sort_unstable();
        rows
    }

    pub(crate) fn selector_point(
        &self,
        dom: &ScriptedDom,
        selector: &taproot::Selector,
    ) -> Result<Option<(f32, f32)>, &'static str> {
        if !self.has_current_snapshot(dom) {
            return Err("retained selector snapshot is unavailable or stale");
        }
        let mut point = None;
        for node in self.matching_nodes(dom, selector) {
            if let Some((x, y, width, height)) = self.visible_rect(dom, node) {
                if point.is_some() {
                    return Err("multiple visible body targets match");
                }
                point = Some((x + width / 2.0, y + height / 2.0));
            }
        }
        Ok(point)
    }

    /// The pane convenience resolvers historically select the first painted
    /// match. Keep that ordering while using the retained semantic and layout
    /// owners for the target and its coordinates.
    pub(crate) fn first_selector_point(
        &self,
        dom: &ScriptedDom,
        selector: &taproot::Selector,
    ) -> Option<(f32, f32)> {
        self.matching_nodes(dom, selector)
            .into_iter()
            .filter_map(|node| self.visible_rect(dom, node))
            .find(|(_, _, width, height)| *width > 0.0 && *height > 0.0)
            .map(|(x, y, width, height)| (x + width / 2.0, y + height / 2.0))
    }

    pub(crate) fn editable_focus(&self, dom: &ScriptedDom, focus: Option<DomNodeId>) -> bool {
        let Some(focus) = focus else {
            return false;
        };
        let Some(projection) = self.document_projection(dom, Some(focus)) else {
            return false;
        };
        projection.nodes().iter().any(|node| {
            node.id.get() == dom.opaque_id(focus)
                && node.state.editable
                && !node.state.disabled
                && !node.state.hidden
                && !node.state.read_only
        })
    }

    pub(crate) fn cursor_area(
        &self,
        dom: &ScriptedDom,
        focus: Option<DomNodeId>,
    ) -> Option<NodeRect> {
        let focus = focus?;
        if !self.editable_focus(dom, Some(focus)) {
            return None;
        }
        let mut caret = None;
        walk_dom(dom, focus, &mut |node| {
            if dom.has_class(node, cambium::FIELD_CARET_CLASS) {
                caret = Some(node);
            }
        });
        self.visible_rect(dom, caret.unwrap_or(focus))
    }

    pub(super) fn inline_target(&self) -> Option<(InlineLayoutHandle, DomNodeId)> {
        let (layout, owner) = self.inline.as_ref()?;
        Some((layout.upgrade()?, *owner))
    }

    pub(super) fn inline_hit(&self, dom: &mut ScriptedDom, x: f32, y: f32) -> Option<DomNodeId> {
        let (layout, owner) = self.inline_target()?;
        let mut layout = layout.borrow_mut();
        let sheet = layout.sheet.clone();
        let (w, h) = layout.size;
        layout.ensure(dom, &sheet, w, h);
        let (left, top, width, height) = layout.painted_rect(dom, owner)?;
        let (visible_x, visible_y, visible_w, visible_h) = layout.visible_rect(dom, owner)?;
        let global = (left + x, top + y);
        if x < 0.0
            || y < 0.0
            || x >= width
            || y >= height
            || global.0 < visible_x
            || global.1 < visible_y
            || global.0 >= visible_x + visible_w
            || global.1 >= visible_y + visible_h
        {
            return None;
        }
        let hit = layout.hit_test(dom, &sheet, w, h, global.0, global.1)?;
        cambium_rootstock::WindowDom::new(dom, owner)
            .contains(hit)
            .then_some(hit)
    }

    /// How many full rebuilds this pane has paid for.
    ///
    /// Expected to reach 1 and stay there for a pane that is merely being
    /// repainted, rising only when its content changes structurally, it is
    /// resized, or its sheet is replaced.
    pub fn rebuilds(&self) -> u32 {
        self.rebuilds
    }

    /// Owner-computed semantics from the retained fragments used by paint.
    /// Revision zero deliberately makes no immutable presented-frame claim.
    pub fn document_projection(
        &self,
        dom: &ScriptedDom,
        focus: Option<DomNodeId>,
    ) -> Option<document_session_api::DocumentA11yProjection> {
        if let Some((layout, owner)) = self.inline_target() {
            return layout.borrow().document_projection_at(dom, owner, focus);
        }
        self.document_projection_at(dom, self.root.unwrap_or_else(|| dom.document()), focus)
    }

    /// A selector may only use the fragments from the current DOM revision.
    /// In inline mode the outer frame owns those fragments, so its cursor is
    /// the one that matters even though the pane itself has no snapshot.
    pub(crate) fn has_current_snapshot(&self, dom: &ScriptedDom) -> bool {
        if let Some((layout, _)) = self.inline_target() {
            return layout.borrow().has_current_snapshot(dom);
        }
        let (base, pending) = dom.pending_mutations();
        self.snapshot.is_some()
            && self.mutation_cursor >= base
            && self.mutation_cursor == base + pending.len() as u64
    }

    /// Match semantic selectors against this layout's current accessibility
    /// projection. Class selectors retain taproot's DOM matching contract.
    pub(crate) fn matching_nodes(
        &self,
        dom: &ScriptedDom,
        selector: &taproot::Selector,
    ) -> Vec<DomNodeId> {
        if !self.has_current_snapshot(dom) {
            return Vec::new();
        }
        if let taproot::Match::Class(_) = &selector.matcher {
            return taproot::matching_classes(dom, selector);
        }
        self.document_projection(dom, None)
            .map(|projection| {
                taproot::matching_prefer_exact_with_projection(dom, selector, &projection)
            })
            .unwrap_or_default()
    }

    /// Every recorded DOM change, snapshot refresh, focus, scroll or scale
    /// change yields a fresh projection; otherwise the last one is reused.
    pub(super) fn document_projection_at(
        &self,
        dom: &ScriptedDom,
        root: DomNodeId,
        focus: Option<DomNodeId>,
    ) -> Option<document_session_api::DocumentA11yProjection> {
        self.snapshot.as_ref()?;
        let (base, pending) = dom.pending_mutations();
        let key = ProjectionKey {
            dom: base + pending.len() as u64,
            focus,
            scale: self.scale.to_bits(),
            viewport_scroll: (
                self.viewport_scroll.0.to_bits(),
                self.viewport_scroll.1.to_bits(),
            ),
            element_scroll: self.scroll_stamp(),
        };
        if let Some(memo) = self.projections.borrow().get(&root)
            && memo.key == key
        {
            return Some(memo.projection.clone());
        }
        let projection = self.fresh_projection_at(dom, root, focus)?;
        self.projections.borrow_mut().insert(
            root,
            ProjectionMemo {
                key,
                projection: projection.clone(),
            },
        );
        Some(projection)
    }

    pub(super) fn fresh_projection_at(
        &self,
        dom: &ScriptedDom,
        root: DomNodeId,
        focus: Option<DomNodeId>,
    ) -> Option<document_session_api::DocumentA11yProjection> {
        use document_session_api::{DocumentA11yBounds, DocumentA11yProjection};
        let snapshot = self.snapshot.as_ref()?;
        let scoped = cambium_rootstock::WindowDom::new(dom, root);
        let projected =
            genet_render::document_a11y_projection(&scoped, &snapshot.fragments, focus, 0);
        let mut nodes = projected.nodes().to_vec();
        let indices: HashMap<_, _> = nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.get(), index))
            .collect();
        // An inline semantic root can sit inside a wider retained snapshot.
        // Carry that owner's transforms and scroll through one descent, then
        // select this projection's nodes by identity.
        let owner =
            cambium_rootstock::WindowDom::new(dom, self.root.unwrap_or_else(|| dom.document()));
        genet_livery::walk_translated_geometry(
            &owner,
            &snapshot.styles,
            &snapshot.fragments,
            &self.element_scroll,
            |node, geometry| {
                #[cfg(test)]
                super::projection_work_tests::geometry_visit();
                #[cfg(test)]
                super::projection_work_tests::semantic_probe();
                let Some(&index) = indices.get(&dom.opaque_id(node)) else {
                    return;
                };
                let semantic = &mut nodes[index];
                // Cambium's controlled field owns editing in its runner, rather
                // than native HTML contenteditable. Its explicit committed-value
                // property supplies producer semantics, not a browser editing flag.
                if semantic.role == document_session_api::DocumentA11yRole::TextField {
                    if let Some(value) = dom.attribute(
                        node,
                        &Namespace::default(),
                        &LocalName::from("data-cambium-text-value"),
                    ) {
                        semantic.value = Some(value.to_owned());
                        semantic.state.editable =
                            !semantic.state.disabled && !semantic.state.read_only;
                        semantic.state.multiline |= dom.attribute(
                            node,
                            &Namespace::default(),
                            &LocalName::from("aria-multiline"),
                        ) == Some("true");
                    }
                }
                semantic.state.hidden |= snapshot
                    .styles
                    .computed_style(node, "visibility")
                    .is_some_and(|value| matches!(value.as_str(), "hidden" | "collapse"));
                semantic.bounds = geometry
                    .border_rect
                    .map(|(x, y, width, height)| {
                        self.physical((
                            x - self.viewport_scroll.0,
                            y - self.viewport_scroll.1,
                            width,
                            height,
                        ))
                    })
                    .map(|(x, y, width, height)| DocumentA11yBounds {
                        x,
                        y,
                        width,
                        height,
                    });
            },
        );
        Some(DocumentA11yProjection::new(
            projected.revision(),
            projected.support().clone(),
            projected.root(),
            nodes,
        ))
    }

    /// AccessKit lowers this same neutral observation, including corrected
    /// retained bounds. Local DOM identity remains outside the platform tree.
    pub fn accessibility_tree(
        &self,
        dom: &ScriptedDom,
        focus: Option<DomNodeId>,
    ) -> Option<(accesskit::TreeUpdate, HashMap<accesskit::NodeId, DomNodeId>)> {
        let projected = self.document_projection(dom, focus)?;
        let semantic_ids: std::collections::HashSet<_> =
            projected.nodes().iter().map(|node| node.id.get()).collect();
        let mut action_map = HashMap::new();
        walk_dom(dom, dom.document(), &mut |node| {
            let id = dom.opaque_id(node);
            #[cfg(test)]
            super::projection_work_tests::semantic_probe();
            if semantic_ids.contains(&id) {
                action_map.insert(accesskit::NodeId(id), node);
            }
        });
        Some((
            genet_render::accesskit_tree_from_projection(projected),
            action_map,
        ))
    }

    pub fn painted_rect(&self, dom: &ScriptedDom, node: DomNodeId) -> Option<NodeRect> {
        if let Some((layout, owner)) = self.inline_target() {
            if !cambium_rootstock::WindowDom::new(dom, owner).contains(node) {
                return None;
            }
            let layout = layout.borrow();
            let (ox, oy, _, _) = layout.painted_rect(dom, owner)?;
            let (x, y, w, h) = layout.painted_rect(dom, node)?;
            return Some((x - ox, y - oy, w, h));
        }
        let scoped =
            cambium_rootstock::WindowDom::new(dom, self.root.unwrap_or_else(|| dom.document()));
        snapshot_node_rect(
            &scoped,
            self.snapshot.as_ref()?,
            node,
            &self.element_scroll,
            self.viewport_scroll,
        )
        .map(|rect| self.physical(rect))
    }
}
