// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    pub(super) fn apply_scope_visit(&mut self, visit: &Visit) -> Option<Vec<Effect>> {
        let place = &visit.place;
        if place.level == ScopeLevel::Overmap {
            if !self
                .sessions
                .iter()
                .any(|(_, m)| m.root_graph_id == place.graph)
                && self.graph_runtimes.canvas(place.graph).is_none()
            {
                return None;
            }
            let graph_pane = self.default_graph_pane();
            let mut effects = self.update(Action::SummonPane(crate::panes::PaneKindId::new(
                crate::panes::kind::OVERMAP,
            )));
            let pane = self
                .frisket
                .iter_leaves()
                .find_map(|(p, c, _)| matches!(c, PaneContent::Overmap(_)).then_some(p))?;
            if let Some(frame) = self.frisket.workspace_mut() {
                if let Some(path) = crate::panes::workspace_bridge::path_for(
                    frame.workspace.tiled(),
                    workbench::TileId(graph_pane.0),
                ) {
                    let event = workbench::TileEvent::Dragged {
                        tile: workbench::TileId(pane.0),
                        to: workbench::DropTarget::Stack {
                            stack: crate::panes::workspace_bridge::path_indices(&path),
                            index: usize::MAX,
                        },
                    };
                    let _ = frame.apply(&event.into());
                }
            }
            self.reveal_pane(pane);
            self.focus = crate::surface::FocusTarget::Pane(pane);
            effects.push(Effect::Redraw);
            return Some(effects);
        }
        if !self.graph_runtimes.canvas(place.graph).is_some_and(|c| {
            place
                .focus
                .is_none_or(|id| c.graph().get_node_by_id(id).is_some())
                && place
                    .root
                    .is_none_or(|id| c.graph().get_node_by_id(id).is_some())
        }) {
            return None;
        }
        let pane = visit
            .anchor
            .as_ref()
            .and_then(|b| b.map)
            .filter(|p| {
                self.graph_for_pane(*p) == Some(place.graph)
                    && self.pane_content(*p) == Some(&PaneContent::Orrery)
            })
            .or_else(|| self.graph_pane_for_navigation(place.graph))?;
        self.graph_views.bind_graph(pane, place.graph);
        if place.level == ScopeLevel::Document {
            let member = place.focus?;
            if place
                .parents()
                .iter()
                .any(|p| matches!(p.level, ScopeLevel::Forme(_)))
            {
                let reader = self.scope_forme_reader(place)?;
                if !self.workbench_for_pane(reader)?.has_tile(member) {
                    return None;
                }
            }
            let reader = visit
                .anchor
                .as_ref()
                .and_then(|b| b.reader)
                .filter(|reader| self.reader_matches_scope(*reader, place))
                .or_else(|| self.scope_forme_reader(place))
                .or(self.reading.scope.reader)
                .filter(|reader| self.reader_matches_scope(*reader, place));
            let mut effects = if let Some(reader) = reader {
                self.read_visit_in_pane(reader, member)
            } else {
                self.read_member_in_graph(place.graph, member)
            };
            let reader = self.active_pane?;
            self.reading.scope.reader = Some(reader);
            if let Some(anchor) = visit
                .anchor
                .as_ref()
                .and_then(|b| b.reading.clone())
                .or_else(|| {
                    self.reading
                        .anchors
                        .get(&ReadTarget {
                            pane: reader,
                            member,
                        })
                        .cloned()
                })
            {
                self.reading.anchors.insert(
                    ReadTarget {
                        pane: reader,
                        member,
                    },
                    anchor.clone(),
                );
                effects.push(Effect::RestoreReadingAnchor {
                    target: ReadTarget {
                        pane: reader,
                        member,
                    },
                    anchor,
                });
            }
            return Some(effects);
        }
        if matches!(place.level, ScopeLevel::Forme(_)) {
            let reader = visit
                .anchor
                .as_ref()
                .and_then(|b| b.reader)
                .filter(|reader| self.reader_matches_scope(*reader, place))
                .or_else(|| self.scope_forme_reader(place))?;
            self.reading.scope.reader = Some(reader);
            let members = self.workbench_for_pane(reader)?.open_members();
            let ScopeLevel::Forme(id) = place.level else {
                unreachable!()
            };
            self.graph_views.set_forme_scope(pane, id, members);
            let region = self.scope_forme_region(place)?;
            self.graph_views.set_forme_region(pane, Some(region));
        } else if place.level == ScopeLevel::Neighborhood {
            let root = place.root.or(place.focus)?;
            self.graph_views.set_neighborhood(pane, root);
        } else {
            self.graph_views.set_scope(pane, None);
        }
        self.reveal_pane(pane);
        if let Some(member) = place.focus {
            self.graph_pane_select_member(pane, member);
        }
        self.with_graph_pane(pane, |canvas| {
            if let Some(camera) = visit.camera {
                canvas.set_camera(mere::canvas::CameraView {
                    offset: (camera.x, camera.y),
                    zoom: camera.zoom,
                });
            } else {
                canvas.fit_visible();
            }
        });
        self.focus = crate::surface::FocusTarget::Graph(pane);
        Some(Vec::new())
    }
}
