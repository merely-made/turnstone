// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl OuterFrame {
    /// Resolve a producer to its actual retained aperture. Inline Workbench
    /// bodies supply their source identity; graph/inset sources use the existing
    /// native hole in the shared Frisket tree.
    pub(crate) fn native_source_node(
        &self,
        app: &crate::app::App,
        surface: &crate::surface::Surface,
        slot: usize,
    ) -> Option<NodeId> {
        use crate::surface::{SurfaceId, SurfaceKind};
        let dom = self.controls.dom.borrow();
        let slots = body_slots(&dom, self.controls.root());
        let matches_role = |node, role| {
            SurfaceId::content_appearance(
                node,
                role ^ (slot as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
            ) == surface.id
        };
        let candidates = match surface.kind {
            SurfaceKind::Graph(pane) | SurfaceKind::Pane(pane) => slots
                .iter()
                .filter(|(id, _, _)| id.0 == pane.0)
                .map(|(_, _, node)| *node)
                .collect::<Vec<_>>(),
            SurfaceKind::Content(node) => {
                let mut candidates = Vec::new();
                for (id, _, aperture) in &slots {
                    let role = match app.pane_content(PaneId(id.0)) {
                        Some(crate::panes::PaneContent::Tile(source)) if *source == node => {
                            Some(0x1000_0000_0000_0000 | id.0)
                        },
                        Some(crate::panes::PaneContent::Orrery) => {
                            Some(0x3000_0000_0000_0000 | id.0)
                        },
                        _ => None,
                    };
                    if role.is_some_and(|role| matches_role(node, role)) {
                        candidates.push(*aperture);
                    }
                }
                let name = node.to_string();
                candidates.extend(nodes_at(&dom, self.controls.root()).into_iter().filter(
                    |candidate| {
                        attr(&dom, *candidate, "data-native-node").as_deref() == Some(name.as_str())
                            && attr(&dom, *candidate, "data-native-role")
                                .and_then(|value| value.parse().ok())
                                .is_some_and(|role| matches_role(node, role))
                    },
                ));
                candidates
            },
            _ => Vec::new(),
        };
        // A source UUID and rectangle are never appearance authority. Equal
        // overlapping appearances retain distinct roles; ambiguity is refused.
        (candidates.len() == 1).then(|| candidates[0])
    }

    pub(crate) fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.controls.dom.borrow()
    }

    pub(crate) fn header_height(&self) -> f32 {
        let dom = self.controls.dom.borrow();
        nodes_at(&dom, self.controls.root())
            .into_iter()
            .filter(|n| {
                attr(&dom, *n, "class")
                    .is_some_and(|class| class.split_whitespace().any(|c| c == "frisket-tabbar"))
            })
            .filter_map(|n| {
                self.layout
                    .borrow()
                    .painted_rect(&dom, n)
                    .map(|(_, _, _, h)| h)
            })
            .fold(0.0, f32::max)
    }

    pub fn hit(&mut self, x: f32, y: f32) -> Option<HeaderHit> {
        let hit = self.layout.borrow_mut().hit_test(
            &mut self.controls.dom.borrow_mut(),
            &self.sheet,
            self.size.0,
            self.size.1,
            x,
            y,
        )?;
        let dom = self.controls.dom.borrow();
        // Body widgets share the tab-bar vocabulary (including numeric
        // data-tabkey values). Their controls belong to the retained runner,
        // even when an ancestor or descendant resembles frame furniture.
        if cambium::slot_kind(&*dom, hit).is_none() {
            if let Some(id) = cambium::close_target(&*dom, hit) {
                return Some(HeaderHit::Close(id));
            }
            if let Some(id) = cambium::tab_target(&*dom, hit) {
                return Some(HeaderHit::Tab(id));
            }
        }
        // Keep the layout seam narrow while admitting a comfortable pointer
        // band on either side. Only a divider in the hit's own split may win:
        // a floating pane covering a seam must keep its input. Reverse walk
        // order gives nested dividers precedence at intersections.
        let hit = self
            .controls
            .order()
            .into_iter()
            .rev()
            .find_map(|control| {
                let Control::Divider { .. } = control else {
                    return None;
                };
                let node = self.controls.node(&control)?;
                let (rx, ry, rw, rh) = self.layout.borrow().painted_rect(&dom, node)?;
                let vertical = attr(&dom, node, "aria-orientation").as_deref() == Some("vertical");
                if !crate::pane::divider_grab_rect(Rect::new(rx, ry, rw, rh), vertical)
                    .contains(x, y)
                {
                    return None;
                }
                let parent = dom.parent(node)?;
                let mut ancestor = Some(hit);
                while let Some(at) = ancestor {
                    if at == parent {
                        return Some(node);
                    }
                    ancestor = dom.parent(at);
                }
                None
            })
            .unwrap_or(hit);
        let target = cambium::divider_target(&*dom, hit)?;
        let tree = tree_at(self.workspace.tiled(), &target.path.0)?;
        let TileTree::Split { axis, children } = tree else {
            return None;
        };
        let mut parent = dom.parent(hit)?;
        // Divider targets can be descendants. Find the containing split box.
        loop {
            if attr(&dom, parent, "class")
                .is_some_and(|s| s.split_whitespace().any(|c| c == "frisket-split"))
            {
                break;
            }
            parent = dom.parent(parent)?;
        }
        let (x, y, w, h) = self.layout.borrow().painted_rect(&dom, parent)?;
        let branches = dom
            .dom_children(parent)
            .into_iter()
            .filter(|n| {
                attr(&dom, *n, "class")
                    .is_some_and(|class| class.split_whitespace().any(|c| c == "frisket-branch"))
            })
            .map(|n| self.layout.borrow().painted_rect(&dom, n))
            .collect::<Option<Vec<_>>>()?;
        if branches.len() != children.len() {
            return None;
        }
        let row = *axis == workbench::SplitAxis::Row;
        let first = branches.first()?;
        let last = branches.last()?;
        // Branch-to-branch gaps include authored divider width and margins;
        // using their span also excludes split padding from distributable space.
        let gaps = branches
            .windows(2)
            .map(|pair| {
                if row {
                    pair[1].0 - pair[0].0 - pair[0].2
                } else {
                    pair[1].1 - pair[0].1 - pair[0].3
                }
            })
            .collect();
        let area = if row {
            Rect::new(first.0, y, last.0 + last.2 - first.0, h)
        } else {
            Rect::new(x, first.1, w, last.1 + last.3 - first.1)
        };
        Some(HeaderHit::Divider {
            target,
            area,
            axis: *axis,
            shares: children.iter().map(|c| c.fraction).collect(),
            gaps,
        })
    }

    pub fn drop_target(&mut self, x: f32, y: f32) -> Option<DropTarget> {
        if x < 0.0 || y < 0.0 || x >= self.size.0 as f32 || y >= self.size.1 as f32 {
            return Some(DropTarget::Outside);
        }
        let hit = self.layout.borrow_mut().hit_test(
            &mut self.controls.dom.borrow_mut(),
            &self.sheet,
            self.size.0,
            self.size.1,
            x,
            y,
        )?;
        let dom = self.controls.dom.borrow();
        if let Some((stack, index)) = cambium::tab_drop_index(&*dom, hit, x, |node| {
            self.layout.borrow().painted_rect(&dom, node)
        }) {
            return Some(DropTarget::Stack { stack, index });
        }
        let mut node = hit;
        loop {
            if let Some(id) =
                attr(&dom, node, cambium::FRISKET_TILE_ATTR).and_then(|s| s.parse::<u64>().ok())
            {
                let (rx, ry, w, h) = self.layout.borrow().painted_rect(&dom, node)?;
                let dx = (x - rx) / w.max(1.0);
                let dy = (y - ry) / h.max(1.0);
                let edge = if dx < 0.25 {
                    Edge::Left
                } else if dx > 0.75 {
                    Edge::Right
                } else if dy < 0.5 {
                    Edge::Top
                } else {
                    Edge::Bottom
                };
                return Some(DropTarget::Edge {
                    tile: TileId(id),
                    edge,
                });
            }
            node = dom.parent(node)?;
        }
    }

    /// Explicit Ctrl-docking inspects tiled apertures beneath the dragged float.
    pub(crate) fn float_dock_target(&self, x: f32, y: f32) -> Option<workbench::FloatDockTarget> {
        if self.workspace.tiled().is_empty()
            && self
                .workspace_area
                .unwrap_or(Rect::full(self.size.0, self.size.1))
                .contains(x, y)
        {
            return Some(workbench::FloatDockTarget::TiledRoot);
        }
        let dom = self.controls.dom.borrow();
        for (id, _, node) in body_slots(&dom, self.controls.root()) {
            if self.workspace.tiled().find(id).is_none() {
                continue;
            }
            let (rx, ry, w, h) = self.layout.borrow().visible_rect(&dom, node)?;
            if !Rect::new(rx, ry, w, h).contains(x, y) {
                continue;
            }
            let dx = (x - rx) / w.max(1.0);
            let dy = (y - ry) / h.max(1.0);
            let edge = if dx < 0.25 {
                Some(Edge::Left)
            } else if dx > 0.75 {
                Some(Edge::Right)
            } else if dy < 0.25 {
                Some(Edge::Top)
            } else if dy > 0.75 {
                Some(Edge::Bottom)
            } else {
                None
            };
            return Some(match edge {
                Some(edge) => workbench::FloatDockTarget::Beside { target: id, edge },
                None => workbench::FloatDockTarget::Tab { target: id },
            });
        }
        None
    }
}
