// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Host bindings for the shared Forme miniature. A card reads the existing
//! workbench and enters its existing access; it never creates an arrangement.
use crate::{
    app::App,
    focus_card::{Origin, Target},
    panes::{GraphId, PaneContent, PaneId},
    scope_navigation::{ScopeLevel, ScopePlace},
};
use mere::forme::FormeRef;
use uuid::Uuid;

fn identity(forme: FormeRef) -> Uuid {
    match forme {
        FormeRef::Stored(id) => id.as_uuid(),
        FormeRef::Identity(graph) => graph,
    }
}
impl App {
    pub(crate) fn reader_matches_scope(
        &self,
        reader: PaneId,
        place: &ScopePlace<GraphId, Uuid>,
    ) -> bool {
        self.pane_content(reader) == Some(&PaneContent::Workbench)
            && self.graph_for_pane(reader) == Some(place.graph)
            && self.workbench_for_pane(reader).is_some_and(|w| {
                place.level != ScopeLevel::Document || place.focus.is_some_and(|m| w.has_tile(m))
            })
            && place.trail().iter().all(|p| {
                if let ScopeLevel::Forme(id) = p.level {
                    self.forme_for_pane(reader)
                        .is_some_and(|key| identity(key.forme) == id)
                } else {
                    true
                }
            })
    }

    fn forme_preview_tree(&self, reader: PaneId) -> Option<(workbench::TileTree, Vec<Uuid>)> {
        let workbench = self.workbench_for_pane(reader)?;
        let graph = self
            .graph_runtimes
            .canvas(self.graph_for_pane(reader)?)?
            .graph();
        let members = workbench.open_members();
        let tree = workbench.to_tile_tree(|member| workbench::Tile {
            id: workbench::TileId(members.iter().position(|m| *m == member).unwrap() as u64),
            title: graph
                .get_node_by_id(member)
                .map(|(k, _)| graph.node_display_label(k))
                .unwrap_or_else(|| "Unavailable access".into()),
            content: workbench::ContentSource::Open {
                kind: "reading".into(),
                id: member.to_string(),
            },
            accent: None,
        })?;
        Some((tree, members))
    }

    pub(crate) fn scope_forme_region(
        &self,
        place: &ScopePlace<GraphId, Uuid>,
    ) -> Option<mere::canvas::FormeRegion> {
        let reader = self.scope_forme_reader(place)?;
        let key = self.forme_for_pane(reader)?;
        let (tree, members) = self.forme_preview_tree(reader)?;
        let graph = self.graph_runtimes.canvas(place.graph)?.graph();
        let swatch = cambium::FormeSwatch::from_tree(String::new(), &tree, |tile| {
            members
                .get(tile.id.0 as usize)
                .copied()
                .filter(|id| graph.get_node_by_id(*id).is_some())
                .map(|id| cambium::FormeSwatchTile {
                    id,
                    title: String::new(),
                    passage: None,
                    focused: false,
                })
        });
        Some(mere::canvas::FormeRegion {
            id: identity(key.forme),
            bounds: [-400.0, -240.0, 400.0, 240.0],
            cells: swatch
                .cells
                .iter()
                .filter_map(|c| {
                    c.tabs.get(c.active).map(|t| mere::canvas::FormeCell {
                        member: t.id,
                        bounds: c.bounds,
                    })
                })
                .collect(),
            locked: true,
            visible: true,
        })
    }
    pub(crate) fn scope_forme_reader(&self, place: &ScopePlace<GraphId, Uuid>) -> Option<PaneId> {
        let id = std::iter::once(place.level)
            .chain(place.parents().into_iter().rev().map(|p| p.level))
            .find_map(|level| {
                if let ScopeLevel::Forme(id) = level {
                    Some(id)
                } else {
                    None
                }
            })?;
        let matches = |pane: PaneId| {
            self.pane_content(pane) == Some(&PaneContent::Workbench)
                && self
                    .forme_for_pane(pane)
                    .is_some_and(|k| k.graph == place.graph && identity(k.forme) == id)
                && self.workbench_for_pane(pane).is_some()
        };
        self.active_pane
            .filter(|p| matches(*p))
            .or_else(|| self.reading.scope.reader.filter(|p| matches(*p)))
            .or_else(|| {
                self.frisket
                    .iter_leaves()
                    .chain(self.lenses.iter().flatten().flat_map(|s| s.iter_leaves()))
                    .find_map(|(pane, content, _)| {
                        (content == &PaneContent::Workbench
                            && self
                                .forme_for_pane(pane)
                                .is_some_and(|k| k.graph == place.graph && identity(k.forme) == id)
                            && self.workbench_for_pane(pane).is_some())
                        .then_some(pane)
                    })
            })
    }
    pub(crate) fn forme_card_target(&self) -> Option<Target> {
        let place = self.scope_place();
        let matches = |pane: PaneId| {
            self.pane_content(pane) == Some(&PaneContent::Workbench)
                && self.graph_for_pane(pane) == Some(place.graph)
                && self
                    .workbench_for_pane(pane)
                    .is_some_and(|w| !w.open_members().is_empty())
        };
        let reader = self
            .active_pane
            .filter(|p| matches(*p))
            .or_else(|| self.scope_forme_reader(&place).filter(|p| matches(*p)))
            .or_else(|| self.reading.scope.reader.filter(|p| matches(*p)))
            .or_else(|| {
                self.frisket
                    .iter_leaves()
                    .chain(self.lenses.iter().flatten().flat_map(|s| s.iter_leaves()))
                    .find_map(|(pane, content, _)| {
                        (content == &PaneContent::Workbench
                            && self.graph_for_pane(pane) == Some(place.graph)
                            && self
                                .workbench_for_pane(pane)
                                .is_some_and(|w| !w.open_members().is_empty()))
                        .then_some(pane)
                    })
            })?;
        let key = self.forme_for_pane(reader)?;
        Some(Target {
            graph: key.graph,
            instance: self.graph_runtimes.get(key.graph)?.instance,
            member: None,
            reader: Some(reader),
            address: String::new(),
            origin: Origin::Forme {
                reader,
                forme: key.forme,
            },
        })
    }
    pub(crate) fn valid_forme_card(&self, target: &Target) -> bool {
        let Origin::Forme { reader, forme } = target.origin else {
            return false;
        };
        self.pane_content(reader) == Some(&PaneContent::Workbench)
            && self
                .forme_for_pane(reader)
                .is_some_and(|key| key.graph == target.graph && key.forme == forme)
            && self
                .workbench_for_pane(reader)
                .is_some_and(|w| !w.open_members().is_empty())
    }
    pub(crate) fn forme_card_view(
        &self,
        target: Target,
        kept: bool,
    ) -> Option<crate::focus_card::CardView> {
        let Origin::Forme { reader, forme } = target.origin else {
            return None;
        };
        let graph = self.graph_runtimes.canvas(target.graph)?.graph();
        let (tree, members) = self.forme_preview_tree(reader)?;
        let focused = self
            .pane_context
            .resolve_context(
                reader,
                &crate::panes::ContextBinding::Own,
                crate::panes::SourceSelector::Graph,
            )
            .and_then(|c| c.member)
            .filter(|m| members.contains(m));
        let swatch = cambium::FormeSwatch::from_tree("Reading layout".into(), &tree, |tile| {
            let member = members.get(tile.id.0 as usize).copied()?;
            let (_, node) = graph.get_node_by_id(member)?;
            let passage = self
                .reading
                .anchors
                .get(&crate::reading_view::ReadTarget {
                    pane: reader,
                    member,
                })
                .filter(|a| a.source == node.url())
                .map(|a| a.exact.clone());
            Some(cambium::FormeSwatchTile {
                id: member,
                title: tile.title.clone(),
                passage,
                focused: focused == Some(member),
            })
        });
        let mut context = ScopePlace {
            level: ScopeLevel::Graph,
            graph: target.graph,
            root: None,
            focus: focused,
            contexts: None,
        }
        .enter(ScopeLevel::Forme(identity(forme)));
        context.focus = focused.or_else(|| {
            swatch
                .cells
                .first()
                .and_then(|c| c.tabs.get(c.active))
                .map(|t| t.id)
        });
        let related = swatch
            .cells
            .iter()
            .flat_map(|c| &c.tabs)
            .filter_map(|tile| {
                let place = context.reading(target.graph, tile.id);
                self.focus_card_target(target.graph, tile.id, Origin::Scope(place))
                    .map(|mut t| {
                        t.reader = Some(reader);
                        (t, tile.title.clone(), String::new())
                    })
            })
            .collect();
        Some(crate::focus_card::CardView {
            target,
            kept,
            info: cambium::FocusCardInfo {
                title: "Reading layout".into(),
                subtitle: format!("{} accesses · {} groups", members.len(), swatch.cells.len()),
                context: "Forme · existing working arrangement".into(),
                preview_status: "Live layout · retained passages".into(),
                excerpt: None,
                primary: None,
                secondary: None,
            },
            related,
            swatch: None,
            forme: Some(swatch),
        })
    }
}
