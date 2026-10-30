// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl RetainedSubtreeLayout {
    pub(crate) fn selector_point(
        &self,
        dom: &ScriptedDom,
        root: DomNodeId,
        selector: &taproot::Selector,
    ) -> Option<(f32, f32)> {
        let (base, pending) = dom.pending_mutations();
        if self.dirty || self.mutation_cursor != base + pending.len() as u64 {
            return None;
        }
        let snapshot = self.snapshot.as_ref()?;
        let scoped = cambium_rootstock::WindowDom::new(dom, root);
        let nodes = if matches!(selector.matcher, taproot::Match::Class(_)) {
            taproot::matching_classes(dom, selector)
        } else {
            let projection =
                genet_render::document_a11y_projection(&scoped, &snapshot.fragments, None, 0);
            taproot::matching_prefer_exact_with_projection(dom, selector, &projection)
        };
        let view = SubtreeView::new(dom, root);
        let mut point = None;
        for node in nodes.into_iter().filter(|node| scoped.contains(*node)) {
            let Some((x, y, w, h)) = snapshot_visible_rect(
                &view,
                snapshot,
                node,
                &HashMap::new(),
                (0.0, 0.0),
                logical_size(self.size, self.scale),
            ) else {
                continue;
            };
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            if point.is_some() {
                return None;
            }
            point = Some(((x + w / 2.0) * self.scale, (y + h / 2.0) * self.scale));
        }
        point
    }

    pub fn new() -> Self {
        Self {
            snapshot: None,
            size: (0, 0),
            scale: 1.0,
            laid_at: 1.0,
            sheet: String::new(),
            dirty: true,
            rebuilds: 0,
            mutation_cursor: 0,
        }
    }

    /// Compose this root at `scale`, as [`RetainedLayout::set_scale`].
    pub(crate) fn set_scale(&mut self, scale: f32) {
        self.scale = clean_scale(scale);
    }

    /// How many full cascades this root has paid for.
    ///
    /// Expected to reach 1 and stay there while the chrome is merely being
    /// repainted, rising when its DOM changes, its window resizes, or the
    /// appearance sheet is replaced.
    pub fn rebuilds(&self) -> u32 {
        self.rebuilds
    }

    /// Mark this root's layout stale. Called by the owner after it has
    /// drained the shared DOM and found the batch non-empty.
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    pub(super) fn ensure(
        &mut self,
        dom: &ScriptedDom,
        root: DomNodeId,
        sheet: &str,
        w: u32,
        h: u32,
    ) {
        let (base, pending) = dom.pending_mutations();
        let revision = base + pending.len() as u64;
        let resized = self.size != (w, h) || self.laid_at != self.scale;
        let resheeted = self.sheet != sheet;
        if self.snapshot.is_some()
            && !self.dirty
            && !resized
            && !resheeted
            && self.mutation_cursor == revision
        {
            return;
        }
        self.rebuilds = self.rebuilds.saturating_add(1);
        tracing::debug!(
            dirty = self.dirty,
            resized,
            resheeted,
            rebuilds = self.rebuilds,
            "chrome subtree layout rebuilt"
        );
        // The text system survives the rebuild; throwing it away would
        // discard the shaping caches that make a rebuild affordable.
        let text = self
            .snapshot
            .take()
            .map_or_else(TextSystem::new, |snapshot| snapshot.text);
        let view = SubtreeView::new(dom, root);
        let (lw, lh) = logical_size((w, h), self.scale);
        self.snapshot = Some(build_livery_snapshot(&view, sheet, lw, lh, text));
        self.size = (w, h);
        self.laid_at = self.scale;
        self.sheet = sheet.to_string();
        self.dirty = false;
        self.mutation_cursor = revision;
    }

    /// This root's scene, cascading only when something actually changed.
    pub fn scene(
        &mut self,
        dom: &ScriptedDom,
        root: DomNodeId,
        sheet: &str,
        w: u32,
        h: u32,
    ) -> netrender::Scene {
        self.ensure(dom, root, sheet, w, h);
        let (lw, lh) = logical_size(self.size, self.scale);
        let view = SubtreeView::new(dom, root);
        let plist = paint_snapshot(
            &view,
            self.snapshot.as_mut().expect("ensure leaves a snapshot"),
            lw,
            lh,
            &HashMap::new(),
            (0.0, 0.0),
        );
        composite(DeviceIntSize::new(lw as i32, lh as i32), &plist)
    }

    /// The same retained forest-root layout, with shared Cambium paint leaves.
    pub fn scene_with_leaves(
        &mut self,
        dom: &ScriptedDom,
        root: DomNodeId,
        sheet: &str,
        w: u32,
        h: u32,
        registry: &mut sprigging::LeafRegistry<u64>,
        cache: &mut sprigging::RenderedLeaves,
    ) -> netrender::Scene {
        self.ensure(dom, root, sheet, w, h);
        let (lw, lh) = logical_size(self.size, self.scale);
        let view = SubtreeView::new(dom, root);
        let snapshot = self.snapshot.as_mut().expect("ensure leaves a snapshot");
        let mut nodes = Vec::new();
        let mut sizes = HashMap::new();
        walk_dom(&view, root, &mut |node| {
            if let (Some(key), Some(fragment)) =
                (custom_leaf_key(&view, node), snapshot.fragments.get(node))
            {
                nodes.push((node, key));
                sizes.insert(
                    key,
                    sprigging::Size {
                        width: fragment.width,
                        height: fragment.height,
                    },
                );
            }
        });
        registry.render_into(|key| sizes.get(&key).copied(), cache);
        let mut plist = paint_snapshot(&view, snapshot, lw, lh, &HashMap::new(), (0., 0.));
        for (node, key) in nodes {
            let Some((x, y, _, _)) =
                snapshot_node_rect(&view, snapshot, node, &HashMap::new(), (0., 0.))
            else {
                continue;
            };
            let Some((vx, vy, vw, vh)) =
                snapshot_visible_rect(&view, snapshot, node, &HashMap::new(), (0., 0.), (lw, lh))
            else {
                continue;
            };
            if let Some(commands) = cache.get(key) {
                let mut clipped = vec![paint_list_api::PaintCmd::PushClip(
                    paint_list_api::ClipSpec {
                        kind: paint_list_api::ClipKind::Rect(paint_list_api::LayoutRect::new(
                            paint_list_api::LayoutPoint::new(vx - x, vy - y),
                            paint_list_api::LayoutPoint::new(vx - x + vw, vy - y + vh),
                        )),
                    },
                )];
                clipped.extend_from_slice(commands);
                clipped.push(paint_list_api::PaintCmd::PopClip);
                plist.push_host_commands_at(paint_list_api::LayoutPoint::new(x, y), &clipped);
            }
        }
        composite(DeviceIntSize::new(lw as i32, lh as i32), &plist)
    }

    /// Hit-test this root against the same retained layout the frame painted.
    ///
    /// Sharing the layout with [`scene`](Self::scene) is the point: a hit test
    /// that built its own cascade could disagree with what is on screen, and
    /// it paid the full cost on every pointer press besides.
    pub fn hit_test(
        &mut self,
        dom: &ScriptedDom,
        root: DomNodeId,
        sheet: &str,
        w: u32,
        h: u32,
        x: f32,
        y: f32,
    ) -> Option<DomNodeId> {
        self.ensure(dom, root, sheet, w, h);
        let view = SubtreeView::new(dom, root);
        snapshot_hit_test(
            &view,
            self.snapshot.as_ref().expect("ensure leaves a snapshot"),
            x / self.scale,
            y / self.scale,
            &HashMap::new(),
            (0.0, 0.0),
            logical_size(self.size, self.scale),
        )
    }

    pub(crate) fn app_region_at(
        &mut self,
        dom: &ScriptedDom,
        root: DomNodeId,
        sheet: &str,
        w: u32,
        h: u32,
        x: f32,
        y: f32,
    ) -> cambium_rootstock::AppRegion {
        let Some(node) = self.hit_test(dom, root, sheet, w, h, x, y) else {
            return Default::default();
        };
        let Some(snapshot) = self.snapshot.as_ref() else {
            return Default::default();
        };
        let value = snapshot
            .styles
            .computed_style(node, "app-region")
            .or_else(|| {
                snapshot
                    .styles
                    .custom_properties(node)?
                    .get("--app-region")
                    .cloned()
            });
        value
            .as_deref()
            .map(cambium_rootstock::AppRegion::parse)
            .unwrap_or_default()
    }
}
