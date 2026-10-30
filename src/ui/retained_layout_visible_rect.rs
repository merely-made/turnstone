// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl RetainedLayout {
    pub fn visible_rect(&self, dom: &ScriptedDom, node: DomNodeId) -> Option<NodeRect> {
        if let Some((layout, owner)) = self.inline_target() {
            if !cambium_rootstock::WindowDom::new(dom, owner).contains(node) {
                return None;
            }
            let layout = layout.borrow();
            let (ox, oy, _, _) = layout.painted_rect(dom, owner)?;
            let (x, y, w, h) = layout.visible_rect(dom, node)?;
            return Some((x - ox, y - oy, w, h));
        }
        let scoped =
            cambium_rootstock::WindowDom::new(dom, self.root.unwrap_or_else(|| dom.document()));
        snapshot_visible_rect(
            &scoped,
            self.snapshot.as_ref()?,
            node,
            &self.element_scroll,
            self.viewport_scroll,
            self.logical(),
        )
        .map(|rect| self.physical(rect))
    }

    /// Resolve a painted text point to a byte in this control's descendants.
    /// Keep the retained shaping, transforms, and scroll offsets used by paint.
    pub(crate) fn text_caret_at(
        &mut self,
        dom: &mut ScriptedDom,
        node: DomNodeId,
        x: f32,
        y: f32,
    ) -> Option<cambium::CaretPosition> {
        if let Some((layout, owner)) = self.inline_target() {
            let mut layout = layout.borrow_mut();
            let (ox, oy, _, _) = layout.painted_rect(dom, owner)?;
            return layout.text_caret_at(dom, node, x + ox, y + oy);
        }
        self.snapshot.as_ref()?;
        // Selection and caret projection can replace styled text nodes between
        // moves in one native event batch. Resolve against their current shape.
        let sheet = self.sheet.clone();
        self.ensure(dom, &sheet, self.size.0, self.size.1);
        let (x, y) = (x / self.scale, y / self.scale);
        let snapshot = self.snapshot.as_ref()?;
        let translated = accumulated_translation(dom, snapshot, node);
        let scroll = ancestor_scroll(dom, node, &self.element_scroll);
        let own_scroll = self.element_scroll.get(&node).copied().unwrap_or_default();
        let (text_node, offset) = snapshot.fragments.text_position_at_point(
            x - translated.0 + scroll.0 + own_scroll.0 + self.viewport_scroll.0,
            y - translated.1 + scroll.1 + own_scroll.1 + self.viewport_scroll.1,
        )?;
        let (mut before, mut found) = (0, false);
        walk_dom(dom, node, &mut |descendant| {
            if found {
                return;
            }
            if descendant == text_node {
                found = true;
            } else if let Some(text) = dom.text(descendant) {
                before += text.len();
            }
        });
        found.then_some(cambium::CaretPosition {
            byte: before + offset,
            affinity: cambium::CaretAffinity::Downstream,
        })
    }

    /// Request the nearest viewport movement; the next paint clamps it and
    /// supplies a new visible rectangle. Ancestor clips remain authoritative:
    /// moving the viewport cannot make a hidden nested target clickable.
    pub fn reveal(&self, dom: &ScriptedDom, node: DomNodeId, scroll: &mut PaneScroll) -> bool {
        let Some((x, y, width, height)) = self.painted_rect(dom, node) else {
            return false;
        };
        let nearest = |start: f32, length: f32, extent: f32| {
            if start < 0.0 {
                start
            } else if start + length > extent {
                (start + length - extent).min(start)
            } else {
                0.0
            }
        };
        let viewport = self
            .inline_target()
            .and_then(|(layout, owner)| {
                layout
                    .borrow()
                    .painted_rect(dom, owner)
                    .map(|(_, _, w, h)| (w, h))
            })
            .unwrap_or((self.size.0 as f32, self.size.1 as f32));
        let dx = nearest(x, width, viewport.0);
        let dy = nearest(y, height, viewport.1);
        scroll.nudge(dx, dy);
        true
    }

    /// Bring the retained layout up to date for this frame.
    ///
    /// Each observer reads its own mutation cursor. The shell drains the
    /// shared forest after recording current owner revisions; an observer
    /// behind that drain rebuilds conservatively rather than losing a sibling's
    /// changes. Standalone owners use the same retained path.
    pub(super) fn ensure(&mut self, dom: &mut ScriptedDom, sheet: &str, w: u32, h: u32) {
        // Every window and body observes the same forest. Reading a sibling
        // cannot consume mutations that another window still needs to see.
        let (base, pending) = dom.pending_mutations();
        let end = base + pending.len() as u64;
        let missed = self.mutation_cursor < base;
        let start = self
            .mutation_cursor
            .saturating_sub(base)
            .min(pending.len() as u64) as usize;
        let mutations = &pending[start..];
        let structural = mutations
            .iter()
            .any(|m| !matches!(m, layout_dom_api::DomMutation::AttributeChanged { .. }))
            || missed;
        let resized = self.size != (w, h) || self.laid_at != self.scale;
        let resheeted = self.sheet != sheet;
        if self.snapshot.is_some() && mutations.is_empty() && !missed && !resized && !resheeted {
            return;
        }
        let full_rebuild = self.snapshot.is_none() || structural || resized || resheeted;
        tracing::debug!(
            structural,
            mutations = mutations.len(),
            resized,
            resheeted,
            full_rebuild,
            "pane Livery layout refreshed"
        );
        if full_rebuild {
            self.rebuilds = self.rebuilds.saturating_add(1);
        }
        let text = self
            .snapshot
            .take()
            .map_or_else(TextSystem::new, |snapshot| snapshot.text);
        let scoped =
            cambium_rootstock::WindowDom::new(dom, self.root.unwrap_or_else(|| dom.document()));
        let (lw, lh) = logical_size((w, h), self.scale);
        self.snapshot = Some(build_livery_snapshot(&scoped, sheet, lw, lh, text));
        self.mutation_cursor = end;
        self.size = (w, h);
        self.laid_at = self.scale;
        self.sheet = sheet.to_string();
        let extent = self
            .snapshot
            .as_ref()
            .expect("a refreshed snapshot is live")
            .content_extent;
        self.viewport_scroll = clamp_scroll(self.viewport_scroll, extent, (lw, lh));
        if self.root.is_none() {
            let mut observed = Vec::new();
            dom.drain_mutations(&mut observed);
        }
    }

    /// [`scene_from_dom`] against the retained layout.
    pub fn scene(
        &mut self,
        dom: &mut ScriptedDom,
        sheet: &str,
        w: u32,
        h: u32,
    ) -> netrender::Scene {
        match self.scene_resources(dom, sheet, w, h) {
            Ok(translated) => translated.scene,
            Err(error) => {
                let (lw, lh) = logical_size((w, h), self.scale);
                pane_scene(&format!("Surface cannot render: {error}"), lw, lh)
            },
        }
    }

    /// Keep the GPU mask requests together with the exact emitted scene.
    /// The rendering owner builds these resources before rasterization.
    pub(crate) fn scene_resources(
        &mut self,
        dom: &mut ScriptedDom,
        sheet: &str,
        w: u32,
        h: u32,
    ) -> Result<paint_list_render::TranslatedDisplayList, paint_list_render::TranslationError> {
        self.ensure(dom, sheet, w, h);
        let scoped =
            cambium_rootstock::WindowDom::new(dom, self.root.unwrap_or_else(|| dom.document()));
        let mut live_keys = std::collections::HashSet::new();
        walk_dom(&scoped, scoped.document(), &mut |node| {
            if let Some(key) = custom_leaf_key(&scoped, node) {
                live_keys.insert(key);
            }
        });
        self.inline_leaves.retain(|key, _| live_keys.contains(key));
        let (lw, lh) = self.logical();
        let mut plist = paint_snapshot(
            &scoped,
            self.snapshot.as_mut().expect("ensure leaves a snapshot"),
            lw,
            lh,
            &self.element_scroll,
            self.viewport_scroll,
        );
        plist.splice_host_leaf_slots(|key| self.inline_leaves.get(&key).cloned(), |_| None);
        plist.splice_text_paint_slots(|node, phase| {
            (phase == genet_livery::TextPaintPhase::AfterContent)
                .then(|| self.native_sources.get(&node).cloned())
                .flatten()
        });
        composite_resources(DeviceIntSize::new(lw as i32, lh as i32), &plist)
    }

    /// [`scene_from_dom_scrolled`] against the retained layout.
    pub fn scene_scrolled(
        &mut self,
        dom: &mut ScriptedDom,
        sheet: &str,
        w: u32,
        h: u32,
        scroll: &mut PaneScroll,
    ) -> netrender::Scene {
        if let Some((layout, owner)) = self.inline_target() {
            let mut layout = layout.borrow_mut();
            let scale = layout.scale;
            let root = self.inline_scroll_root.unwrap_or(owner);
            // The pane's scroll is physical; the frame's fragments are in
            // layout pixels.
            let mut offset = (scroll.offset.0 / scale, scroll.offset.1 / scale);
            if let Some(snapshot) = &layout.snapshot {
                if let Some(fragment) = snapshot.fragments.get(owner) {
                    let scoped = cambium_rootstock::WindowDom::new(dom, owner);
                    let extent = content_extent(&scoped, &snapshot.fragments);
                    let viewport = (
                        fragment.width.round().max(1.0) as u32,
                        fragment.height.round().max(1.0) as u32,
                    );
                    let extent = (extent.0 - fragment.x, extent.1 - fragment.y);
                    offset = clamp_scroll(offset, extent, viewport);
                    scroll.offset = (offset.0 * scale, offset.1 * scale);
                    if let Some(bar) = self.inline_bar {
                        let mut commands = LiveryPaintList::new(
                            DeviceIntSize::new(viewport.0 as i32, viewport.1 as i32),
                            0,
                        );
                        append_scrollbars(&mut commands, extent, viewport, offset, scroll.alpha());
                        layout
                            .inline_leaves
                            .insert(dom.opaque_id(bar), commands.commands().to_vec());
                    }
                }
            }
            layout.element_scroll.insert(root, offset);
            return netrender::Scene::default();
        }

        self.ensure(dom, sheet, w, h);
        let (lw, lh) = self.logical();
        let scale = self.scale;
        let scoped =
            cambium_rootstock::WindowDom::new(dom, self.root.unwrap_or_else(|| dom.document()));
        let snapshot = self.snapshot.as_mut().expect("ensure leaves a snapshot");
        self.viewport_scroll = clamp_scroll(
            (scroll.offset.0 / scale, scroll.offset.1 / scale),
            snapshot.content_extent,
            (lw, lh),
        );
        scroll.offset = (
            self.viewport_scroll.0 * scale,
            self.viewport_scroll.1 * scale,
        );
        let viewport = DeviceIntSize::new(lw as i32, lh as i32);
        let mut plist = paint_snapshot(
            &scoped,
            snapshot,
            lw,
            lh,
            &self.element_scroll,
            self.viewport_scroll,
        );
        append_scrollbars(
            &mut plist,
            snapshot.content_extent,
            (lw, lh),
            self.viewport_scroll,
            scroll.alpha(),
        );
        composite(viewport, &plist)
    }

    /// [`scene_from_dom_with_leaves`] against the retained layout.
    pub fn scene_with_leaves(
        &mut self,
        dom: &mut ScriptedDom,
        sheet: &str,
        w: u32,
        h: u32,
        registry: &mut sprigging::LeafRegistry<u64>,
        cache: &mut sprigging::RenderedLeaves,
    ) -> netrender::Scene {
        if let Some((layout, owner)) = self.inline_target() {
            let mut layout = layout.borrow_mut();
            let sheet = layout.sheet.clone();
            let (width, height) = layout.size;
            layout.ensure(dom, &sheet, width, height);
            let scoped = cambium_rootstock::WindowDom::new(dom, owner);
            let mut sizes = HashMap::new();
            walk_dom(&scoped, owner, &mut |node| {
                if let (Some(key), Some(fragment)) = (
                    custom_leaf_key(&scoped, node),
                    layout.snapshot.as_ref().and_then(|s| s.fragments.get(node)),
                ) {
                    sizes.insert(key, (fragment.width, fragment.height));
                }
            });
            registry.render_into(
                |key| {
                    sizes
                        .get(&key)
                        .map(|&(width, height)| sprigging::Size { width, height })
                },
                cache,
            );
            for key in sizes.keys() {
                if let Some(commands) = cache.get(*key) {
                    layout.inline_leaves.insert(*key, commands.to_vec());
                }
            }
            return netrender::Scene::default();
        }
        self.ensure(dom, sheet, w, h);
        let scoped =
            cambium_rootstock::WindowDom::new(dom, self.root.unwrap_or_else(|| dom.document()));
        let snapshot = self.snapshot.as_mut().expect("ensure leaves a snapshot");
        let mut sizes = HashMap::new();
        walk_dom(&scoped, scoped.document(), &mut |node| {
            if let (Some(key), Some(fragment)) =
                (custom_leaf_key(&*dom, node), snapshot.fragments.get(node))
            {
                sizes.insert(key, (fragment.width, fragment.height));
            }
        });
        registry.render_into(
            |key| {
                sizes.get(&key).map(|&(w, h)| sprigging::Size {
                    width: w,
                    height: h,
                })
            },
            cache,
        );
        let (lw, lh) = logical_size(self.size, self.scale);
        let viewport = DeviceIntSize::new(lw as i32, lh as i32);
        let mut plist = paint_snapshot(
            &scoped,
            snapshot,
            lw,
            lh,
            &self.element_scroll,
            self.viewport_scroll,
        );
        walk_dom(&scoped, scoped.document(), &mut |node| {
            let Some(key) = custom_leaf_key(&*dom, node) else {
                return;
            };
            let Some((x, y, _, _)) = snapshot_node_rect(
                &scoped,
                snapshot,
                node,
                &self.element_scroll,
                self.viewport_scroll,
            ) else {
                return;
            };
            if let Some(commands) = cache.get(key) {
                plist.push_host_commands_at(paint_list_api::LayoutPoint::new(x, y), commands);
            }
        });
        composite(viewport, &plist)
    }

    /// Hit-test against the layout the last frame painted.
    ///
    /// Sharing the retained layout with paint is the point, not only a saving:
    /// a click must resolve against the fragments on screen, and a
    /// freshly-built layout matches them only by coincidence.
    pub fn hit_test(
        &mut self,
        dom: &mut ScriptedDom,
        sheet: &str,
        w: u32,
        h: u32,
        x: f32,
        y: f32,
    ) -> Option<DomNodeId> {
        if self.inline_target().is_some() {
            return self.inline_hit(dom, x, y);
        }

        self.ensure(dom, sheet, w, h);
        let scoped =
            cambium_rootstock::WindowDom::new(dom, self.root.unwrap_or_else(|| dom.document()));
        snapshot_hit_test(
            &scoped,
            self.snapshot.as_ref().expect("ensure leaves a snapshot"),
            x / self.scale,
            y / self.scale,
            &self.element_scroll,
            self.viewport_scroll,
            self.logical(),
        )
    }
}
