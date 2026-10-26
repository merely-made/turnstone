// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Scope routes are view intent. Reading, graph truth and pane arrangement keep
//! their owners; history records accepted projections, never synthetic parents.
use crate::{
    action::{Action, Effect},
    app::App,
    panes::{GraphId, PaneContent, PaneId},
    reading_view::ReadTarget,
};
pub use mere::navigation::browser::{ScopeContext, ScopeDestination, ScopeLevel, ScopePlace};
use cambium::scope_navigation::{NodeActivation, ScopeCamera, ScopeControls, ScopeDirection, ScopeVisit};
pub type Destination = ScopeDestination<GraphId, Uuid>;
pub type ScopeCommand = cambium::scope_navigation::ScopeCommand<Destination>;
pub type ScopeCrumb = cambium::scope_navigation::ScopeCrumb<Destination>;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ScopeBookmark {
    pub reader: Option<PaneId>,
    pub map: Option<PaneId>,
    pub reading: Option<tabard::ReadingAnchor>,
}
impl<'de> Deserialize<'de> for ScopeBookmark {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Current {
            reader: Option<PaneId>,
            map: Option<PaneId>,
            reading: Option<tabard::ReadingAnchor>,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Saved {
            Current(Current),
            Legacy(tabard::ReadingAnchor),
        }
        Ok(match Saved::deserialize(deserializer)? {
            Saved::Current(c) => Self {
                reader: c.reader,
                map: c.map,
                reading: c.reading,
            },
            Saved::Legacy(reading) => Self {
                reading: Some(reading),
                ..Default::default()
            },
        })
    }
}
impl ScopeBookmark {
    pub fn validate(&self) -> bool {
        self.reading.as_ref().is_none_or(|a| a.validate().is_ok())
    }
}
pub type Navigation = cambium::scope_navigation::ScopeNavigation<ScopePlace<GraphId, Uuid>, ScopeBookmark>;
type Visit = cambium::scope_navigation::ScopeVisit<ScopePlace<GraphId, Uuid>, ScopeBookmark>;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScopeState {
    pub navigation: Navigation,
    pub reader: Option<PaneId>,
    #[serde(skip)]
    pub hint: Option<ScopeDirection>,
    #[serde(skip)]
    pub restoring: bool,
    #[serde(skip)]
    pub pending: Option<(Visit, Option<bool>)>,
    #[serde(skip)]
    pub activation: std::collections::BTreeMap<PaneId, NodeActivation<(Uuid,Uuid)>>,
    #[serde(skip)]
    pub read_request: Option<(GraphId, Uuid, Uuid)>,
    #[serde(skip)]
    pub editing_location: bool,
}

impl App {
    pub(crate) fn scope_place(&self) -> ScopePlace<GraphId, Uuid> {
        let held = self
            .reading
            .scope
            .navigation
            .current()
            .map(|v| v.place.clone());
        if self.active_pane.is_some_and(|p| {
            !matches!(
                self.pane_content(p),
                Some(PaneContent::Workbench | PaneContent::Orrery | PaneContent::Overmap(_))
            )
        }) {
            if let Some(place) = held {
                return place;
            }
        }
        if self
            .active_pane
            .is_some_and(|p| matches!(self.pane_content(p), Some(PaneContent::Overmap(_))))
        {
            if let Some(place) = held {
                return place.project(ScopeLevel::Overmap,None,place.focus);
            }
        }
        let reader = self
            .active_pane
            .filter(|p| self.pane_content(*p) == Some(&PaneContent::Workbench));
        let graph = self
            .active_pane
            .and_then(|p| self.graph_for_pane(p))
            .or_else(|| held.as_ref().map(|p| p.graph))
            .unwrap_or_else(|| self.graph_runtimes.active_graph());
        let member = if reader.is_some() {
            self.browser_command_target().map(|(m, _)| m)
        } else {
            self.graph_pane_for_navigation(graph)
                .and_then(|p| self.graph_pane_focused_member(p))
        };
        let mut place = held.filter(|p| p.graph == graph).unwrap_or(ScopePlace {
            level: ScopeLevel::Graph,
            graph,
            root: member,
            focus: member,
            contexts: None,
        });
        if reader.is_some() {
            if let Some(member)=member {
                if place.level!=ScopeLevel::Document || place.focus!=Some(member) {place=place.reading(graph,member);}
            } else {place=place.project(ScopeLevel::Graph,None,None);}
        } else {
            if let Some(pane)=self.active_pane.filter(|p|self.pane_content(*p)==Some(&PaneContent::Orrery)) {
                if let Some(root)=self.graph_views.neighborhood_root(pane) {
                    place=place.project(ScopeLevel::Neighborhood,Some(root),member);
                } else if !matches!(place.level,ScopeLevel::Forme(_)) {place=place.project(ScopeLevel::Graph,None,member);}
            } else if matches!(place.level,ScopeLevel::Document|ScopeLevel::Overmap) {
                place=place.project(ScopeLevel::Graph,None,member);
            }
            if let Some(member)=member {place.select(member);}
        }
        place
    }
    fn scope_visit(&self) -> Visit {
        let place = self.scope_place();
        let camera = self
            .graph_pane_for_navigation(place.graph)
            .and_then(|p| self.graph_views.viewport(p))
            .map(|v| ScopeCamera {
                x: v.offset.0,
                y: v.offset.1,
                zoom: v.zoom,
            });
        let reader = self
            .active_pane
            .filter(|p| self.pane_content(*p) == Some(&PaneContent::Workbench))
            .or(self.reading.scope.reader);
        let anchor = reader.zip(place.focus).and_then(|(pane, member)| {
            self.reading
                .anchors
                .get(&ReadTarget { pane, member })
                .cloned()
        });
        let map = self.graph_pane_for_navigation(place.graph);
        cambium::scope_navigation::ScopeVisit {
            place,
            camera,
            anchor: Some(ScopeBookmark {
                reader,
                map,
                reading: anchor,
            }),
        }
    }
    /// Capture external reading/tab navigation as an ordinary history visit.
    pub(crate) fn record_scope_reading(&mut self, pane: PaneId, member: Uuid) {
        if self.reading.scope.restoring {
            return;
        }
        let Some(graph) = self.graph_for_pane(pane) else {
            return;
        };
        self.reading.scope.reader = Some(pane);
        let current = self
            .reading
            .scope
            .navigation
            .current()
            .map(|v| v.place.clone());
        let place=current.map(|p|p.reading(graph,member)).unwrap_or(ScopePlace {
            level:ScopeLevel::Document,graph,root:Some(member),focus:Some(member),contexts:None,
        });
        let anchor = self
            .reading
            .anchors
            .get(&ReadTarget { pane, member })
            .cloned();
        self.reading.scope.navigation.visit(cambium::scope_navigation::ScopeVisit {
            place,
            camera: None,
            anchor: Some(ScopeBookmark {
                reader: Some(pane),
                map: self
                    .reading
                    .scope
                    .navigation
                    .current()
                    .and_then(|v| v.anchor.as_ref())
                    .and_then(|b| b.map)
                    .filter(|map| {
                        self.graph_for_pane(*map) == Some(graph)
                            && self.forme_for_pane(*map) == self.forme_for_pane(pane)
                    })
                    .or_else(|| self.graph_pane_for_navigation(graph)),
                reading: anchor,
            }),
        });
    }
    pub(crate) fn scope_command(&mut self, command: ScopeCommand) -> Vec<Effect> {
        self.scope_command_for_reader(command,None)
    }
    pub(crate) fn scope_command_for_reader(&mut self, command: ScopeCommand, requested_reader:Option<PaneId>) -> Vec<Effect> {
        if command == ScopeCommand::EditLocation {
            return self.edit_scope_location();
        }
        if let ScopeCommand::Hint(hint) = command {
            self.reading.scope.hint = hint;
            return vec![Effect::Redraw];
        }
        if self.reading.scope.pending.is_some() {
            return vec![Effect::Redraw];
        }
        let origin = self.scope_visit();
        if origin.place.level == ScopeLevel::Document
            && origin
                .place
                .focus
                .is_some_and(|m| self.node_uses_web_surface(m))
        {
            match command {
                ScopeCommand::Navigate(ScopeDirection::Back) if self.focused_can_back() => {
                    return self.nav_back();
                },
                ScopeCommand::Navigate(ScopeDirection::Forward) if self.focused_can_forward() => {
                    return self.nav_forward();
                },
                _ => {},
            }
        }
        if origin.place.level == ScopeLevel::Document {
            self.reading.scope.reader = self.active_pane.or(self.reading.scope.reader);
        }
        self.reading.scope.navigation.capture(origin.clone());
        let history = match command {
            ScopeCommand::Navigate(ScopeDirection::Back) => Some(false),
            ScopeCommand::Navigate(ScopeDirection::Forward) => Some(true),
            _ => None,
        };
        let destination = if let Some(forward) = history {
            self.reading.scope.navigation.destination(forward).cloned()
        } else {
            let place = match command {
                ScopeCommand::Navigate(ScopeDirection::Up) => origin.place.up(),
                ScopeCommand::Navigate(ScopeDirection::Down) => origin.place.down(),
                ScopeCommand::To(ref destination) => destination.resolve(&origin.place).filter(|p| p != &origin.place || requested_reader.is_some_and(|reader|origin.anchor.as_ref().and_then(|b|b.reader)!=Some(reader))),
                _ => None,
            };
            place.map(|place| {
                let saved = self.reading.scope.navigation.saved_matching(|p| p.same_scope(&place)).filter(|v|requested_reader.is_none_or(|reader|v.anchor.as_ref().and_then(|b|b.reader)==Some(reader)));
                cambium::scope_navigation::ScopeVisit {
                    camera: saved.and_then(|v| v.camera),
                    anchor: Some(ScopeBookmark {
                        reader: requested_reader.or_else(||saved.and_then(|v|v.anchor.as_ref()).and_then(|b|b.reader)).or_else(||origin.anchor.as_ref().and_then(|b|b.reader)),
                        reading: saved
                            .and_then(|v| v.anchor.as_ref())
                            .and_then(|a| a.reading.clone()),
                        ..origin.anchor.clone().unwrap_or_default()
                    }),
                    place,
                }
            })
        };
        let Some(destination) = destination else {
            return match command {
                ScopeCommand::Navigate(ScopeDirection::Back) => self.nav_back(),
                ScopeCommand::Navigate(ScopeDirection::Forward) => self.nav_forward(),
                _ => vec![Effect::Redraw],
            };
        };
        if destination.place.level != ScopeLevel::Overmap
            && self
                .graph_pane_for_navigation(destination.place.graph)
                .is_none()
        {
            if let Some(id) = self
                .sessions
                .iter()
                .find(|(_, m)| m.root_graph_id == destination.place.graph)
                .map(|(id, _)| id)
            {
                self.reading.scope.pending = Some((destination, history));
                return vec![Effect::SwitchSession { id }];
            }
        }
        self.reading.scope.restoring = true;
        let effects = self.apply_scope_visit(&destination);
        self.reading.scope.restoring = false;
        let Some(mut effects) = effects else {
            return vec![Effect::Redraw];
        };
        if let Some(forward) = history {
            self.reading.scope.navigation.step(forward);
        } else {
            self.reading.scope.navigation.visit(destination);
        }
        self.reading.scope.hint = None;
        effects.extend([
            Effect::SaveReadingPreferences,
            Effect::SaveSession,
            Effect::Redraw,
        ]);
        effects
    }
    pub(crate) fn record_scope_current(&mut self) {
        if !self.reading.scope.restoring {
            let visit = self.scope_visit();
            self.reading.scope.navigation.visit(visit);
        }
    }
    pub(crate) fn capture_scope_origin(&mut self) {
        if !self.reading.scope.restoring {
            let visit = self.scope_visit();
            self.reading.scope.navigation.capture(visit);
        }
    }
    pub(crate) fn retain_scope_anchor(
        &mut self,
        target: ReadTarget,
        anchor: &tabard::ReadingAnchor,
    ) -> bool {
        let Some(mut visit) = self
            .reading
            .scope
            .navigation
            .current()
            .cloned()
            .filter(|v| {
                v.place.level == ScopeLevel::Document
                    && v.place.focus == Some(target.member)
                    && self.graph_for_pane(target.pane) == Some(v.place.graph)
            })
        else {
            return false;
        };
        let bookmark = visit.anchor.get_or_insert_with(Default::default);
        if bookmark.reader.is_some_and(|p| p != target.pane)
            || bookmark.reading.as_ref() == Some(anchor)
        {
            return false;
        }
        bookmark.reader = Some(target.pane);
        bookmark.reading = Some(anchor.clone());
        self.reading.scope.navigation.capture(visit);
        true
    }
    pub(crate) fn select_scope_mere(&mut self, id: crate::panes::SessionId) -> Vec<Effect> {
        if !self
            .active_pane
            .is_some_and(|p| matches!(self.pane_content(p), Some(PaneContent::Overmap(_))))
        {
            return self.update(Action::SwitchSession(id));
        }
        let Some(graph) = self.sessions.get(id).map(|m| m.root_graph_id) else {
            return vec![Effect::Redraw];
        };
        let mut visit = self.scope_visit();
        if visit.place.graph != graph {
            visit.place.graph = graph;
            visit.place.root = None;
            visit.place.focus = None;
            visit.anchor = None;
            visit.camera = None;
        }
        self.reading.scope.navigation.capture(visit);
        vec![Effect::SaveReadingPreferences, Effect::Redraw]
    }
    pub(crate) fn enter_scope_session(&mut self, id: crate::panes::SessionId) {
        self.capture_scope_origin();
        if let Some(graph) = self.sessions.get(id).map(|m| m.root_graph_id) {
            let place = ScopePlace {
                level: ScopeLevel::Graph,
                graph,
                root: None,
                focus: None,
                contexts: None,
            };
            let visit = self
                .reading
                .scope
                .navigation
                .saved_matching(|p| p.same_scope(&place))
                .cloned()
                .unwrap_or(cambium::scope_navigation::ScopeVisit {
                    place,
                    camera: None,
                    anchor: None,
                });
            self.reading.scope.pending = Some((visit, None));
        }
    }
    pub(crate) fn restore_scope_after_session(&mut self) -> Vec<Effect> {
        let pending = self.reading.scope.pending.take();
        let visit = pending
            .as_ref()
            .map(|(v, _)| v.clone())
            .or_else(|| self.reading.scope.navigation.current().cloned());
        let Some(mut visit) = visit else {
            return Vec::new();
        };
        if pending.is_none() && visit.place.level == ScopeLevel::Document {
            let reader = visit
                .anchor
                .as_ref()
                .and_then(|b| b.reader)
                .or(self.reading.scope.reader);
            if let Some(anchor) = reader
                .zip(visit.place.focus)
                .and_then(|(pane, member)| self.reading.anchors.get(&ReadTarget { pane, member }))
                .cloned()
            {
                visit.anchor.get_or_insert_with(Default::default).reading = Some(anchor);
            }
        }
        self.reading.scope.restoring = true;
        let restored = self.apply_scope_visit(&visit);
        self.reading.scope.restoring = false;
        let Some(mut effects) = restored else {
            return Vec::new();
        };
        if let Some((visit, history)) = pending {
            if let Some(forward) = history {
                self.reading.scope.navigation.step(forward);
            } else {
                self.reading.scope.navigation.visit(visit);
            }
        } else {
            let current = self.scope_visit();
            self.reading.scope.navigation.capture(current);
        }
        effects.push(Effect::SaveReadingPreferences);
        effects
    }
    fn apply_scope_visit(&mut self, visit: &Visit) -> Option<Vec<Effect>> {
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
            if place.parents().iter().any(|p|matches!(p.level,ScopeLevel::Forme(_))) {
                let reader=self.scope_forme_reader(place)?;
                if !self.workbench_for_pane(reader)?.has_tile(member) {return None;}
            }
            let reader=visit.anchor.as_ref().and_then(|b|b.reader).filter(|reader|self.reader_matches_scope(*reader,place))
                .or_else(||self.scope_forme_reader(place))
                .or(self.reading.scope.reader).filter(|reader|self.reader_matches_scope(*reader,place));
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
        if matches!(place.level,ScopeLevel::Forme(_)) {
            let reader=visit.anchor.as_ref().and_then(|b|b.reader)
                .filter(|reader|self.reader_matches_scope(*reader,place))
                .or_else(||self.scope_forme_reader(place))?;
            self.reading.scope.reader=Some(reader);
            let members=self.workbench_for_pane(reader)?.open_members();
            let ScopeLevel::Forme(id)=place.level else {unreachable!()};
            self.graph_views.set_forme_scope(pane,id,members);
            let region=self.scope_forme_region(place)?;
            self.graph_views.set_forme_region(pane,Some(region));
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

#[cfg(test)]
mod tests;

mod presentation;
