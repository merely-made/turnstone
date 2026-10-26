// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Host-resolved, transient focus cards. A kept card holds a source reference,
//! never a copied graph object or a second reading runtime.
use super::*;
use graphshell::scope_policy::{Place, ScopeCommand, ScopeDestination, ScopeLevel};
use mere::navigation::{RelationshipTarget, relationships};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Target {
    owner: Uuid,
    epoch: u64,
    member: Option<Uuid>,
    scope: Option<Place>,
    forme: Option<Uuid>,
    address: String,
    source: Option<Uuid>,
    relation: Option<RelationshipTarget>,
}
#[derive(Default)]
pub(super) struct State {
    deck: cambium::FocusCardDeck<Target>,
}

fn target(
    page: &TreePage,
    member: Uuid,
    source: Option<Uuid>,
    relation: Option<RelationshipTarget>,
) -> Option<Target> {
    let owner = super::scope_navigation::owner(page)?;
    let canvas = page.shared.canvas.borrow();
    let (_, node) = canvas.graph().get_node_by_id(member)?;
    Some(Target {
        owner,
        epoch: page.instrument.scope.gesture_epoch,
        member: Some(member),
        scope: None,
        forme: None,
        address: node.url().into(),
        source,
        relation,
    })
}
fn valid(page: &TreePage, t: &Target) -> bool {
    if super::scope_navigation::owner(page) != Some(t.owner)
        || page.instrument.scope.gesture_epoch != t.epoch
    {
        return false;
    }
    if let Some(id) = t.forme {
        return page.forme.as_ref().is_some_and(|p| {
            p.current().document.id.as_uuid() == id && !p.current().members().is_empty()
        });
    }
    let canvas = page.shared.canvas.borrow();
    let graph = canvas.graph();
    if let Some(place) = &t.scope {
        if place.trail().iter().any(|p|matches!(p.level,ScopeLevel::Forme(id) if page.forme.as_ref().is_none_or(|f|f.current().document.id.as_uuid()!=id))) {return false;}
        return place.valid()
            && place.graph == t.owner
            && [place.root, place.focus]
                .into_iter()
                .flatten()
                .all(|m| graph.get_node_by_id(m).is_some())
            && (place.level != ScopeLevel::Document
                || place
                    .focus
                    .and_then(|m| graph.get_node_by_id(m))
                    .is_some_and(|(_, n)| n.url() == t.address));
    }
    t.member.is_some()
        && graph
            .get_node_by_id(t.member.unwrap())
            .is_some_and(|(_, n)| n.url() == t.address)
        && t.source.is_none_or(|s| graph.get_node_by_id(s).is_some())
        && t.relation.is_none_or(|r| {
            relationships(t.owner, graph, r.source)
                .iter()
                .any(|row| row.target == r)
        })
}
fn show(page: &mut TreePage, t: Target) {
    if !valid(page, &t) {
        return;
    }
    page.instrument.cards.deck.show(t);
    page.instrument.navigation.open = false;
    page.instrument.panes_open = false;
    page.shared.dirty.set(true);
}
pub(super) fn related(page: &mut TreePage, r: RelationshipTarget) {
    if let Some(t) = target(page, r.member, Some(r.source), Some(r)) {
        show(page, t);
    }
}
pub(super) fn reference(page: &mut TreePage, source: Uuid, member: Uuid) {
    if let Some(t) = target(page, member, Some(source), None) {
        show(page, t);
    }
}
fn lower_scope(page: &TreePage) -> Option<Target> {
    let place = super::scope_navigation::place(page)?.down()?;
    let address = place
        .focus
        .and_then(|m| {
            page.shared
                .canvas
                .borrow()
                .graph()
                .get_node_by_id(m)
                .map(|(_, n)| n.url().to_string())
        })
        .unwrap_or_default();
    Some(Target {
        owner: place.graph,
        epoch: page.instrument.scope.gesture_epoch,
        member: place.focus,
        address,
        source: None,
        relation: None,
        scope: Some(place),
        forme: None,
    })
}
pub(super) fn control(page: &TreePage) -> Child {
    let target = lower_scope(page);
    let name = target
        .as_ref()
        .and_then(|t| t.scope.as_ref())
        .map(|p| format!("Peek below: {}", super::scope_navigation::label(page, p)))
        .unwrap_or_else(|| "Peek below".into());
    cambium::shell_control(
        "Peek",
        name,
        cambium::ShellControl {
            disabled: target.is_none(),
            ..Default::default()
        },
        move |page: &mut TreePage, _| {
            if let Some(t) = target.clone() {
                show(page, t);
            }
        },
    )
}

fn forme_target(page: &TreePage) -> Option<Target> {
    let workspace = page.forme.as_ref()?.current();
    if workspace.members().is_empty() {
        return None;
    }
    Some(Target {
        owner: workspace.document.graph_id,
        epoch: page.instrument.scope.gesture_epoch,
        member: None,
        scope: None,
        forme: Some(workspace.document.id.as_uuid()),
        address: String::new(),
        source: None,
        relation: None,
    })
}

pub(super) fn has_forme(page: &TreePage) -> bool {
    forme_target(page).is_some_and(|t| valid(page, &t))
}

fn enter_forme(page: &mut TreePage, target: Target) {
    let Some(id) = target.forme else {
        return;
    };
    if !valid(page, &target) {
        return;
    }
    let Some(workspace) = page.forme.as_ref().map(|p| p.current()) else {
        return;
    };
    let Some(place) = super::scope_navigation::place(page) else {
        return;
    };
    let context = place
        .project(
            ScopeLevel::Graph,
            None,
            workspace.preferences.focused_reading,
        )
        .enter(ScopeLevel::Forme(id));
    page.instrument.cards.deck.peek = None;
    super::scope_navigation::command(
        page,
        ScopeCommand::To(mere::navigation::browser::ScopeDestination::Place(context)),
    );
}

pub(super) fn field_command(page: &mut TreePage, command: cambium::ScopeFieldCommand) {
    page.instrument.panes_open = false;
    let Some(target) = forme_target(page) else {
        return;
    };
    match command {
        cambium::ScopeFieldCommand::Peek => show(page, target),
        cambium::ScopeFieldCommand::Enter => enter_forme(page, target),
    }
}

pub(super) fn field_controls(page: &TreePage) -> Option<Child> {
    if !super::instrument::enabled(page)
        || super::blocks_active(page)
        || page.instrument.panes_open
        || page.instrument.navigation.open
        || page.commands.open.is_some()
        || page.product.as_ref().is_some_and(|p| p.menu_open())
        || page.forme.as_ref()?.workbench
    {
        return None;
    }
    let target = forme_target(page)?;
    if !valid(page, &target) {
        return None;
    }
    let place = super::scope_navigation::place(page)?;
    if place.level == ScopeLevel::Overmap {
        return None;
    }
    let canvas = page.shared.canvas.borrow();
    let region = canvas
        .forme_region()
        .filter(|r| Some(r.id) == target.forme)?;
    let viewport = canvas.viewport();
    let field = cambium::ScopeField {
        label: "Reading layout".into(),
        bounds: region.screen_bounds(viewport)?,
        aperture: [0., 0., viewport.view.0 as f32, viewport.view.1 as f32],
        can_enter: place.level != ScopeLevel::Forme(region.id),
        scale: 1.0,
    };
    cambium::scope_field_controls(&field, move |page: &mut TreePage, command| {
        if !valid(page, &target) {
            return;
        }
        match command {
            cambium::ScopeFieldCommand::Peek => show(page, target.clone()),
            cambium::ScopeFieldCommand::Enter => enter_forme(page, target.clone()),
        }
    })
}
pub(super) fn forme_control(page: &TreePage) -> Child {
    let target = forme_target(page);
    cambium::shell_control(
        "Layout",
        "Preview reading layout",
        cambium::ShellControl {
            disabled: target.is_none(),
            ..Default::default()
        },
        move |page: &mut TreePage, _| {
            if let Some(target) = target.clone() {
                show(page, target);
            }
        },
    )
}
fn forme_card(page: &TreePage, t: &Target, kept: bool) -> Option<Child> {
    let pane = page.forme.as_ref()?;
    let workspace = pane.current();
    let canvas = page.shared.canvas.borrow();
    let tree = workspace.tile_tree(canvas.graph())?;
    let anchors = super::reading::capture_progress(page).0;
    let swatch = cambium::FormeSwatch::from_tree("Reading layout".into(), &tree, |tile| {
        let member = workspace.member_for_tile(tile.id)?;
        let address = canvas.graph().get_node_by_id(member)?.1.url();
        let passage = anchors
            .get(&member)
            .filter(|a| a.source == address && !a.exact.is_empty())
            .map(|a| a.exact.clone())
            .or_else(|| {
                super::reading::preview_excerpt(page, member).map(|p| {
                    p.trim_start_matches("Retained document excerpt: ")
                        .trim_start_matches("Last reading position: ")
                        .to_string()
                })
            });
        Some(cambium::FormeSwatchTile {
            id: member,
            title: tile.title.clone(),
            passage,
            focused: workspace.preferences.focused_reading == Some(member),
        })
    });
    let mut context = super::scope_navigation::place(page)?
        .project(
            ScopeLevel::Graph,
            None,
            workspace.preferences.focused_reading,
        )
        .enter(ScopeLevel::Forme(t.forme?));
    context.focus = workspace.preferences.focused_reading;
    let destinations = swatch
        .cells
        .iter()
        .flat_map(|c| &c.tabs)
        .map(|tile| (tile.id, context.reading(t.owner, tile.id)))
        .collect::<Vec<_>>();
    let target = t.clone();
    let body = cambium::forme_swatch(&swatch, move |page: &mut TreePage, member| {
        if !valid(page, &target)
            || !page
                .instrument
                .cards
                .deck
                .cards()
                .iter()
                .any(|(t, _)| t == &target)
        {
            return;
        }
        if let Some((_, place)) = destinations.iter().find(|(id, _)| *id == member) {
            page.instrument.cards.deck.peek = None;
            super::scope_navigation::command(
                page,
                ScopeCommand::To(ScopeDestination::Place(place.clone())),
            );
        }
    });
    let info = cambium::FocusCardInfo {
        title: "Reading layout".into(),
        subtitle: format!(
            "{} accesses · {} groups",
            workspace.members().len(),
            swatch.cells.len()
        ),
        context: "Forme · existing working arrangement".into(),
        preview_status: "Live layout · retained passages".into(),
        excerpt: None,
        primary: None,
        secondary: None,
    };
    let target = t.clone();
    Some(cambium::focus_card_with_body(
        info,
        kept,
        body,
        Vec::new(),
        move |page: &mut TreePage, command| {
            use cambium::FocusCardCommand as C;
            match command {
                C::Close => page.instrument.cards.deck.close(&target),
                C::KeepOpen if valid(page, &target) => page.instrument.cards.deck.keep(&target),
                _ => {},
            }
            page.shared.dirty.set(true);
        },
    ))
}
fn scoped_swatch(
    page: &TreePage,
    t: &Target,
    index: usize,
) -> Option<cambium::GraphCanvasSwatch<Uuid, ()>> {
    let place = t.scope.as_ref()?;
    if place.level == ScopeLevel::Document {
        return None;
    }
    let canvas = page.shared.canvas.borrow();
    let graph = canvas.graph();
    let mut members = mere::navigation::scope_members(graph, place)?;
    members.sort();
    if let Some(i) = members.iter().position(|m| Some(*m) == place.focus) {
        members.swap(0, i);
    }
    members.truncate(32);
    let nodes = members
        .iter()
        .filter_map(|m| {
            graph
                .get_node_by_id(*m)
                .map(|(k, n)| (*m, graph.node_display_label(k), n.url().into()))
        })
        .collect();
    let edges = graph
        .projected_relations()
        .filter_map(|(_, r)| {
            let from = graph.get_node(r.from)?.id;
            let to = graph.get_node(r.to)?.id;
            (members.contains(&from) && members.contains(&to)).then_some((from, to))
        })
        .collect();
    Some(cambium::context_graph_swatch(
        0x7300_0000 + index as u64,
        nodes,
        edges,
        place.focus,
        super::scope_navigation::label(page, place),
    ))
}
pub(super) fn register(ctx: &mut AppCtx<'_, TreePage, Logic, Child>) {
    let page = ctx.runner.state();
    for (index, (t, _)) in page.instrument.cards.deck.cards().into_iter().enumerate() {
        if valid(page, &t) {
            if let Some(swatch) = scoped_swatch(page, &t, index) {
                ctx.leaves.insert(
                    swatch.leaf_key,
                    Box::new(swatch.paint_leaf(|_| cambium::ColorF {
                        r: 0.48,
                        g: 0.68,
                        b: 0.86,
                        a: 1.0,
                    })),
                );
            }
        }
    }
}
pub(super) fn selection_control(page: &TreePage) -> Child {
    let member = page.shared.canvas.borrow().focused_member();
    let t = member.and_then(|m| target(page, m, None, None));
    cambium::shell_control(
        "Peek selection",
        "Peek selection",
        cambium::ShellControl {
            disabled: t.is_none(),
            ..Default::default()
        },
        move |page: &mut TreePage, _| {
            if let Some(t) = t.clone() {
                show(page, t);
            }
        },
    )
}
pub(super) fn dismiss(page: &mut TreePage) -> bool {
    let had = page.instrument.cards.deck.peek.take().is_some();
    if had {
        page.shared.dirty.set(true);
    }
    had
}
pub(super) fn fields(page: &TreePage, snap: ProbeSnapshot) -> ProbeSnapshot {
    let has_forme = page
        .instrument
        .cards
        .deck
        .cards()
        .iter()
        .any(|(t, _)| t.forme.is_some() && valid(page, t));
    snap.with_field(
        "forme-field-controls",
        usize::from(field_controls(page).is_some()).to_string(),
    )
    .with_field("focus-card-forme", has_forme.to_string())
    .with_field(
        "focus-card-scope",
        page.instrument
            .cards
            .deck
            .peek
            .as_ref()
            .and_then(|t| t.scope.as_ref())
            .map(|p| p.level.label())
            .unwrap_or_default(),
    )
    .with_field(
        "focus-card-count",
        page.instrument
            .cards
            .deck
            .cards()
            .iter()
            .filter(|(t, _)| valid(page, t))
            .count()
            .to_string(),
    )
    .with_field(
        "focus-card-kept",
        page.instrument
            .cards
            .deck
            .kept
            .iter()
            .filter(|t| valid(page, t))
            .count()
            .to_string(),
    )
    .with_field(
        "focus-card-peek",
        page.instrument
            .cards
            .deck
            .peek
            .as_ref()
            .filter(|t| valid(page, t))
            .map(|t| t.address.clone())
            .unwrap_or_default(),
    )
}
pub(super) fn view(page: &TreePage) -> Option<Child> {
    // Command overlays temporarily own the column; keeping the binding does
    // not give a card authority to intercept those controls.
    if page.instrument.panes_open
        || page.instrument.navigation.open
        || page.instrument.location.open
    {
        return None;
    }
    let mut cards = Vec::new();
    for (index, (t, kept)) in page.instrument.cards.deck.cards().into_iter().enumerate() {
        if !valid(page, &t) {
            continue;
        }
        if t.forme.is_some() {
            if let Some(card) = forme_card(page, &t, kept) {
                cards.push(card);
            }
            continue;
        }
        let canvas = page.shared.canvas.borrow();
        let graph = canvas.graph();
        let swatch = scoped_swatch(page, &t, index);
        let title = if let Some(place) = &t.scope {
            super::scope_navigation::label(page, place)
        } else {
            graph.node_display_label(graph.get_node_by_id(t.member?)?.0)
        };
        let context = if let Some(place) = &t.scope {
            format!("Scope below · {}", place.level.label())
        } else if let Some(r) = t.relation {
            relationships(t.owner, graph, r.source)
                .into_iter()
                .find(|row| row.target == r)
                .map(|r| r.relationship)
                .unwrap_or_default()
        } else if let Some(source) = t.source {
            format!(
                "Reference from {}",
                graph
                    .get_node_by_id(source)
                    .map(|(k, _)| graph.node_display_label(k))
                    .unwrap_or_else(|| "document".into())
            )
        } else {
            "Selected object · Document".into()
        };
        let related = t
            .member
            .map(|m| relationships(t.owner, graph, m))
            .unwrap_or_default()
            .into_iter()
            .take(3)
            .filter_map(|row| {
                let next = target(
                    page,
                    row.target.member,
                    Some(row.target.source),
                    Some(row.target),
                )?;
                Some(cambium::shell_navigation_item(
                    row.title,
                    row.relationship,
                    Default::default(),
                    move |page: &mut TreePage, _| show(page, next.clone()),
                ))
            })
            .collect();
        drop(canvas);
        let preview = t
            .member
            .and_then(|m| super::reading::preview_excerpt(page, m));
        let (mut preview_status, mut excerpt) = match preview {
            Some(text) if text.starts_with("Retained document excerpt: ") => (
                "Retained excerpt".into(),
                Some(
                    text.trim_start_matches("Retained document excerpt: ")
                        .to_string(),
                ),
            ),
            Some(text) => (
                "Last retained reading passage".into(),
                Some(
                    text.trim_start_matches("Last reading position: ")
                        .to_string(),
                ),
            ),
            None => ("No retained passage available".into(), None),
        };
        let body: Child = if let Some(swatch) = &swatch {
            preview_status = "Live scoped graph".into();
            excerpt = None;
            let targets = swatch
                .graph
                .nodes
                .iter()
                .filter_map(|n| {
                    let mut selected = t.scope.as_ref()?.clone();
                    selected.select(n.id);
                    let mut next = target(page, n.id, None, None)?;
                    next.scope = Some(selected.down()?);
                    Some((n.id, next))
                })
                .collect::<Vec<_>>();
            Box::new(cambium::graph_canvas_swatch(
                swatch,
                move |page: &mut TreePage, member| {
                    if let Some((_, t)) = targets.iter().find(|(m, _)| *m == member) {
                        show(page, t.clone());
                    }
                },
                |_: &mut TreePage, _| {},
                |_: &mut TreePage| {},
            ))
        } else {
            Box::new(el(
                "div",
                excerpt
                    .clone()
                    .map(|text| el("p", text).attr("class", "focus-card-excerpt")),
            ))
        };
        let subtitle = if let Some(swatch) = &swatch {
            let total = mere::navigation::scope_members(
                page.shared.canvas.borrow().graph(),
                t.scope.as_ref().unwrap(),
            )
            .map(|m| m.len())
            .unwrap_or(0);
            format!("{total} objects · {} shown", swatch.graph.nodes.len())
        } else {
            t.address.clone()
        };
        let info = cambium::FocusCardInfo {
            title,
            subtitle,
            context,
            preview_status,
            excerpt,
            primary: Some(if t.scope.is_some() { "Enter" } else { "Read" }.into()),
            secondary: t.scope.is_none().then(|| "Show in Map".into()),
        };
        cards.push(cambium::focus_card_with_body(
            info,
            kept,
            body,
            if swatch.is_some() {
                Vec::new()
            } else {
                related
            },
            move |page: &mut TreePage, command| {
                use cambium::FocusCardCommand as C;
                if command == C::Close {
                    page.instrument.cards.deck.close(&t);
                    page.shared.dirty.set(true);
                    return;
                }
                if !valid(page, &t)
                    || !page
                        .instrument
                        .cards
                        .deck
                        .cards()
                        .iter()
                        .any(|(shown, _)| shown == &t)
                {
                    page.instrument.cards.deck.close(&t);
                    return;
                }
                match command {
                    C::KeepOpen => page.instrument.cards.deck.keep(&t),
                    C::Primary => {
                        page.instrument.cards.deck.peek = None;
                        if let Some(place) = &t.scope {
                            super::scope_navigation::command(
                                page,
                                ScopeCommand::To(ScopeDestination::Place(place.clone())),
                            );
                        } else if let Some(member) = t.member {
                            super::location::read_document(page, member);
                        }
                    },
                    C::Secondary => {
                        page.instrument.cards.deck.peek = None;
                        if let Some(member) = t.member {
                            super::reading::show_graph(page, member);
                        }
                    },
                    C::Close => {},
                }
                page.shared.dirty.set(true);
            },
        ));
    }
    if cards.is_empty() {
        return None;
    }
    Some(Box::new(el("div",cards).attr("class","focus-card-deck workspace-focus-cards")
        .attr("style",format!("position:absolute;top:48px;right:12px;width:{}px;max-height:{}px;overflow-y:auto;z-index:24;",360.0_f32.min((page.size.0 as f32-24.0).max(0.0)),(page.size.1 as f32-72.0).max(0.0)))))
}
