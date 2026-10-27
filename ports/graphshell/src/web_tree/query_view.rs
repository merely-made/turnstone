// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Browser adapter for the reusable query editor and retained query readings.
use super::*;
use cambium::{button, lens};
use graphshell::query_views::{PresentedLink, QueryFreezeCorrelation, presented_links};
use mere::kernel::graph::GraphStratum;
use mere::query_view::editor::{Completion, Effect, SaveResult, Scope};
use mere::query_view::{self as model, *};
use mere_view::query_editor::{FIELD_LABELS, QueryEditor};
use sceno::{LinkKey, LinkMarks, LinkStratum};
use uuid::Uuid;

struct CatalogEntry {
    owner: Uuid,
    title: String,
    frozen: bool,
}
#[derive(Default)]
pub(super) struct Pane {
    editor: QueryEditor,
    source: Option<(Scope, u64)>,
    catalog: Vec<CatalogEntry>,
    tags: Vec<(String, String)>,
    pending_freeze: Option<QueryFreezeCorrelation>,
    definition: Option<String>,
    inspection: Option<((Uuid, String), u64, String)>,
}
fn load(page: &mut TreePage, owner: Option<Uuid>) {
    let result: Result<((Uuid, String), QueryViewSpec), String> = (|| {
        let product = page.product.as_ref().ok_or("Saved graph unavailable")?;
        if product.selection_locked() {
            return Err("Wait for the current save".into());
        }
        let scope = product.retained_scope().ok_or("Source unavailable")?;
        let spec = match owner {
            Some(id) => model::read_spec(product.source_graph()?, id)?,
            None => QueryViewSpec::select(
                "Saved query",
                "SELECT ?member WHERE { ?member ?predicate ?object }",
            ),
        };
        Ok((scope, spec))
    })();
    match result {
        Ok((scope, spec)) => {
            page.query_views.editor.open(scope.into(), owner, spec);
            page.query_views.definition = None;
        },
        Err(error) => page.query_views.editor.notice(error),
    }
}
fn source(page: &TreePage) -> Option<(Scope, u64)> {
    let product = page.product.as_ref()?;
    Some((
        product.retained_scope()?.into(),
        product.source_graph().ok()?.revision(),
    ))
}
pub(super) fn ready(page: &TreePage) -> bool {
    let pane = &page.query_views;
    let available = page.product.as_ref().is_some_and(|p| !p.selection_locked());
    pane.editor.has_effects()
        || pane.editor.available() != available
        || page
            .product
            .as_ref()
            .is_some_and(|p| p.query_completion_ready())
        || (source(page).is_some() && source(page) != pane.source)
        || (page.product.is_none() && pane.source.is_some())
}
pub(super) fn poll(page: &mut TreePage) {
    let current = source(page);
    let pane = &mut page.query_views;
    pane.editor
        .set_available(page.product.as_ref().is_some_and(|p| !p.selection_locked()));
    // While a store task owns the app, a missing reading is temporary. Only
    // an actual new binding (or removal) retires the editor's interest.
    if (current.is_some() || page.product.is_none()) && current != pane.source {
        if current.as_ref().map(|(scope, _)| scope) != pane.source.as_ref().map(|(scope, _)| scope)
        {
            if pane
                .editor
                .lifecycle()
                .scope()
                .is_some_and(|scope| current.as_ref().map(|(current, _)| current) != Some(scope))
            {
                pane.editor.retire();
            }
            pane.definition = None;
            pane.pending_freeze = None;
        }
        pane.inspection = None;
        pane.catalog.clear();
        pane.tags.clear();
        if let Some(graph) = page.product.as_ref().and_then(|p| p.source_graph().ok()) {
            for resource in graph.resource_nodes() {
                if let Ok(frozen) = graph.open_frozen_selection(resource.id()) {
                    pane.catalog.push(CatalogEntry {
                        owner: resource.id(),
                        frozen: true,
                        title: format!(
                            "Frozen selection {} · {} members · source revision {}",
                            resource.id(),
                            frozen.selection.members().len(),
                            frozen.selection.revision()
                        ),
                    });
                }
                if let Ok(spec) = model::read_spec(graph, resource.id()) {
                    pane.catalog.push(CatalogEntry {
                        owner: resource.id(),
                        frozen: false,
                        title: spec.title,
                    });
                }
                if let Some(concept) = graph.resource_tag_concept(resource.id()) {
                    let term = serde_json::to_string(resource.canonical_iri())
                        .expect("IRI string serializes");
                    pane.tags.push((format!("Use tag: {} ({})", concept.label, concept.owner_iri),
                        format!("SELECT ?member WHERE {{ ?member <{}> ?tag . FILTER(STR(?tag) = {term}) }}", mere::kernel::graph::resource::TAGGED_WITH_IRI)));
                }
            }
        }
        pane.source = current;
    }
    while let Some(effect) = pane.editor.take_effect() {
        let (id, scope, result, retry) = match effect {
            Effect::Save(request) => {
                let id = request.id;
                let scope = request.scope.clone();
                let result = page
                    .product
                    .as_mut()
                    .ok_or_else(|| "Saved graph unavailable".to_string())
                    .and_then(|p| p.save_query(request));
                (id, scope, result, false)
            },
            Effect::Retry { id, scope } => {
                let result = page
                    .product
                    .as_mut()
                    .ok_or_else(|| "Saved graph unavailable".to_string())
                    .and_then(|p| p.retry_query(id, &scope));
                (id, scope, result, true)
            },
        };
        if let Err(reason) = result {
            let result = if retry {
                SaveResult::Unconfirmed { reason }
            } else {
                SaveResult::Refused { reason }
            };
            pane.editor.complete(Completion { id, scope, result });
        }
    }
    let mut open = Vec::new();
    if let Some(product) = page.product.as_mut() {
        while let Some(completion) = product.take_query_completion() {
            match completion {
                product::QueryCompletion::Save(completion) => {
                    if let Some(access) = pane.editor.complete(completion) {
                        open.push(access);
                    }
                },
                product::QueryCompletion::Freeze {
                    correlation,
                    result,
                } => {
                    if pane.pending_freeze.as_ref() != Some(&correlation) {
                        continue;
                    }
                    match result {
                        Ok(()) => { pane.pending_freeze = None; pane.editor.notice("Query selection frozen and saved"); },
                        Err(reason) => pane.editor.notice(format!("Query freeze unconfirmed: {reason}. Retry the pending save in Graph tools.")),
                    }
                },
            }
        }
    }
    pane.editor
        .set_available(page.product.as_ref().is_some_and(|p| !p.selection_locked()));
    for access in open {
        super::forme::open_retained(page, access);
    }
}
/// The legacy Graph tools retry uses the same component request when its
/// editor is still listening. Two retry controls cannot start distinct attempts.
/// With no listening editor, the owner retains its ordinary retry affordance.
pub(super) fn retry_pending(page: &mut TreePage) -> bool {
    if let Some(pending_freeze) = page.query_views.pending_freeze.clone() {
        let retry = page.product.as_mut().and_then(|product| {
            (product.pending_query_freeze().as_ref() == Some(&pending_freeze)).then(|| {
                let result = product.retry_owner_operation();
                (result, product.pending_query_freeze())
            })
        });
        if let Some((result, current)) = retry {
            match result {
                Ok(_) => {
                    page.query_views.pending_freeze = current;
                    page.query_views
                        .editor
                        .notice("Retrying frozen selection save…");
                },
                Err(error) => page.query_views.editor.notice(error),
            }
            return true;
        }
    }
    let Some(pending) = page.product.as_ref().and_then(|p| p.pending_query_id()) else {
        return false;
    };
    match *page.query_views.editor.lifecycle().state() {
        model::editor::SaveState::Saving(id) if id.same_operation(pending) => true,
        model::editor::SaveState::Unconfirmed(id) if id.same_operation(pending) => {
            page.query_views.editor.retry();
            true
        },
        _ => false,
    }
}

pub(super) fn focused_text(
    runner: &cambium_rootstock::Runner<TreePage, Logic, Child>,
) -> Option<cambium_rootstock::FocusedTextSlot<TreePage>> {
    if !runner.state().query_views.editor.editable() {
        return None;
    }
    let node = runner.focus()?;
    let dom = runner.dom();
    let dom = dom.borrow();
    let index = FIELD_LABELS
        .iter()
        .position(|label| focused_field_has_label(&dom, node, label))?;
    Some(cambium_rootstock::FocusedTextSlot {
        node,
        get: Box::new(move |page| page.query_views.editor.field(index)),
        get_mut: Box::new(move |page| page.query_views.editor.field_mut(index)),
    })
}
pub(super) fn section(page: &TreePage) -> Child {
    let pane = &page.query_views;
    let mut children: Vec<Child> = vec![
        Box::new(el("h3", "Saved query views")),
        Box::new(button("New query view", |page: &mut TreePage, _| {
            load(page, None)
        })),
    ];
    for entry in &pane.catalog {
        let owner = entry.owner;
        if entry.frozen {
            let lease = pane.source.as_ref().map(|(scope, _)| scope.pair());
            children.push(Box::new(el("p", entry.title.clone())));
            children.push(Box::new(button(format!("Inspect frozen selection {owner}"), move |page: &mut TreePage, _| {
                let result = page.product.as_ref().ok_or("Source unavailable".into()).and_then(|product| {
                    if product.retained_scope() != lease { return Err("Frozen selection belongs to another owner".into()); }
                    let graph = product.source_graph()?;
                    let frozen = graph.open_frozen_selection(owner)?;
                    serde_json::to_string_pretty(&json!({"owner": owner, "selection": frozen.selection, "coverage": frozen.coverage})).map(|text| (graph.revision(), text)).map_err(|e| e.to_string())
                });
                match result {
                    Ok((revision, text)) => { if let Some(scope) = lease.clone() { page.query_views.inspection = Some((scope, revision, text)); } },
                    Err(error) => page.query_views.editor.notice(error),
                }
            })));
        } else {
            children.push(Box::new(button(
                format!("Edit query: {}", entry.title),
                move |page: &mut TreePage, _| load(page, Some(owner)),
            )));
        }
    }
    if pane.editor.editable() {
        for (label, query) in &pane.tags {
            let query = query.clone();
            children.push(Box::new(button(
                label.clone(),
                move |page: &mut TreePage, _| page.query_views.editor.use_query(&query),
            )));
        }
        if let Some(owner) = pane.editor.lifecycle().owner() {
            let lease = pane.editor.lifecycle().scope().map(Scope::pair);
            children.push(Box::new(button(
                "Copyable saved query definition",
                move |page: &mut TreePage, _| {
                    let result = page
                        .product
                        .as_ref()
                        .ok_or("Source unavailable".into())
                        .and_then(|product| {
                            if product.selection_locked() || product.retained_scope() != lease {
                                return Err(
                                    "Query definition belongs to another owner or a pending save"
                                        .into(),
                                );
                            }
                            model::shared_spec(product.source_graph()?, owner)
                        });
                    match result {
                        Ok(text) => page.query_views.definition = Some(text),
                        Err(error) => page.query_views.editor.notice(error),
                    }
                },
            )));
        }
    }
    children.push(Box::new(lens(
        |editor: &mut QueryEditor| mere_view::query_editor::view(editor),
        |page: &mut TreePage| &mut page.query_views.editor,
    )));
    if let Some((_, _, text)) = pane.inspection.as_ref().filter(|(scope, revision, _)| {
        page.product.as_ref().is_some_and(|p| {
            p.retained_scope().as_ref() == Some(scope)
                && p.source_graph().is_ok_and(|g| g.revision() == *revision)
        })
    }) {
        children.push(Box::new(
            el("pre", text.clone())
                .attr("aria-label", "Query source record")
                .attr("style", "white-space:pre-wrap;user-select:text"),
        ));
    }
    if let Some(text) = &pane.definition {
        children.push(Box::new(
            el("pre", text.clone())
                .attr("aria-label", "Shared query definition")
                .attr("style", "white-space:pre-wrap;user-select:text"),
        ));
    }
    Box::new(
        el("section", children)
            .attr("class", "tree-query")
            .attr("aria-label", "Saved query views"),
    )
}

/// Resolve a retained tile by its explicit shown binding, not by address matching.
pub(super) fn appearance(
    page: &TreePage,
    access: Uuid,
) -> Option<(Result<QueryProjection, String>, Option<(Uuid, String)>)> {
    let product = page.product.as_ref()?;
    let graph = match product.source_graph() {
        Ok(graph) => graph,
        Err(error) => {
            let canvas = page.shared.canvas.borrow();
            let graph = canvas.graph();
            let key = graph.get_node_key_by_id(access)?;
            let owner = graph.shown_resource_id(key)?;
            return graph
                .resource_record(owner)?
                .facets
                .iter()
                .any(|f| f.facet == QUERY_VIEW_FACET)
                .then_some((Err(error), None));
        },
    };
    let key = graph.get_node_key_by_id(access)?;
    let owner = graph.shown_resource_id(key)?;
    if !graph
        .resource_record(owner)?
        .facets
        .iter()
        .any(|f| f.facet == QUERY_VIEW_FACET)
    {
        return None;
    }
    Some((model::project_view(graph, owner), product.retained_scope()))
}
pub(super) fn render(snapshot: (Result<QueryProjection, String>, Option<(Uuid, String)>)) -> Child {
    let (projection, scope) = snapshot;
    let projection = match projection {
        Ok(p) => Rc::new(p),
        Err(e) => return Box::new(el("p", e).attr("role", "alert")),
    };
    let mut positions = vec![];
    let mut row = [0usize; 2];
    for node in &projection.nodes {
        let column = if node.identity.stratum == GraphStratum::Resource {
            0
        } else {
            1
        };
        positions.push((
            node.identity,
            20.0 + column as f64 * 350.0,
            20.0 + row[column] as f64 * 170.0,
        ));
        row[column] += 1;
    }
    let mut diagram: Vec<Child> = vec![];
    let links = presented_links(&projection);
    let mut add_stroke =
        |relation: &QueryRelation,
         cue,
         label: String,
         data_kind: String,
         link_attrs: Option<(LinkKey, LinkMarks, usize)>| {
            let Some((_, ax, ay)) = positions.iter().find(|(id, ..)| *id == relation.from) else {
                return;
            };
            let Some((_, bx, by)) = positions.iter().find(|(id, ..)| *id == relation.to) else {
                return;
            };
            let (dx, dy) = (bx - ax, by - ay);
            let length = dx.hypot(dy);
            let angle = dy.atan2(dx).to_degrees();
            let pieces: Vec<Child> = model::stroke_parts(cue, length).into_iter().map(|part| {
                Box::new(el("div", ()).attr("aria-hidden", "true").attr("style", format!(
                    "display:block;position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;background:currentColor;border-radius:{}px",
                part.offset, part.top, part.width, part.height, if part.rounded { 1.5 } else { 0.0 }
            ))) as Child
        }).collect();
            let mut stroke = el("div", pieces)
            .attr("role", "img")
            .attr("aria-label", label)
            .attr("data-query-relation", data_kind)
            .attr("style", format!("position:absolute;left:{}px;top:{}px;width:{length}px;height:3px;transform-origin:0 0;transform:rotate({angle}deg)", ax+145.0, ay+48.0));
            if let Some((key, marks, strength)) = link_attrs {
                stroke = stroke
                    .attr("data-link-key", key.stable_id())
                    .attr("data-link-forward", marks.forward.to_string())
                    .attr("data-link-reverse", marks.reverse.to_string())
                    .attr("data-link-opposed", marks.opposed.to_string())
                    .attr("data-link-strength", strength.to_string());
            }
            diagram.push(Box::new(stroke));
        };
    for link in &links {
        let relation = &projection.relations[link.representative];
        let kind = match link.key.stratum {
            LinkStratum::Resource => model::VisualRelationKind::ResourceLink,
            LinkStratum::Surface => model::VisualRelationKind::SurfaceLink,
        };
        let label = format!(
            "{}: {} to {}; {} held contributions; forward {}, reverse {}, opposed {}",
            kind.label(),
            link.key.first,
            link.key.second,
            link.strength,
            link.marks.forward,
            link.marks.reverse,
            link.marks.opposed
        );
        add_stroke(
            relation,
            kind.cue(&projection.spec.styles),
            label,
            format!("{kind:?}"),
            Some((link.key.clone(), link.marks, link.strength)),
        );
    }
    for relation in projection
        .relations
        .iter()
        .filter(|relation| relation.kind == model::VisualRelationKind::ShownBinding)
    {
        add_stroke(
            relation,
            relation.kind.cue(&projection.spec.styles),
            format!(
                "{}: {:?} {} to {:?} {}",
                relation.kind.label(),
                relation.source_from.stratum,
                relation.source_from.id,
                relation.source_to.stratum,
                relation.source_to.id
            ),
            format!("{:?}", relation.kind),
            None,
        );
    }
    for (node, (_, x, y)) in projection.nodes.iter().zip(&positions) {
        let mut content: Vec<Child> = vec![
            Box::new(el("strong", node.title.clone())),
            Box::new(
                el(
                    "p",
                    format!("{:?} · {}", node.identity.stratum, node.identity.id),
                )
                .attr("style", "font-size:10px;overflow-wrap:anywhere"),
            ),
        ];
        if node.identity.stratum == GraphStratum::Resource {
            if node.held_accesses.is_empty() {
                content.push(Box::new(el("span", "No held appearance")));
            }
            for (ordinal, access) in node.held_accesses.iter().enumerate() {
                let access = *access;
                let label = projection
                    .nodes
                    .iter()
                    .find(|n| n.identity == QueryNodeId::surface(access))
                    .map(|n| n.title.clone())
                    .unwrap_or_else(|| format!("appearance {}", ordinal + 1));
                let resource = node.identity.id;
                let source = projection.clone();
                let scope = scope.clone();
                content.push(Box::new(button(
                    format!("Open {label}"),
                    move |p: &mut TreePage, _| {
                        let result = p
                            .product
                            .as_ref()
                            .ok_or("Source unavailable".into())
                            .and_then(|product| {
                                if product.retained_scope() != scope {
                                    return Err("Query appearance belongs to another owner".into());
                                }
                                source.access_for(product.source_graph()?, resource, access)
                            });
                        match result {
                            Ok(()) => super::forme::open_retained(p, access),
                            Err(e) => p.query_views.editor.notice(e),
                        }
                    },
                )));
            }
        }
        diagram.push(Box::new(el("article", content).attr("aria-label", format!("{:?}: {}", node.identity.stratum, node.title)).attr("style", format!("position:absolute;z-index:1;left:{x}px;top:{y}px;width:290px;height:140px;overflow:auto;border:1px solid currentColor;background:var(--tabard-color-surface);padding:6px;overflow-wrap:anywhere"))));
    }
    let coverage = if projection.coverage.limits.is_empty() {
        "No known gaps in the supplied graph.".to_owned()
    } else {
        format!(
            "Known coverage limits: {}.",
            projection
                .coverage
                .limits
                .iter()
                .map(|limit| {
                    match limit.count {
                        Some(count) => format!("{} ({count})", limit.reason),
                        None => limit.reason.clone(),
                    }
                })
                .collect::<Vec<_>>()
                .join("; ")
        )
    };
    let mut children: Vec<Child> = vec![
        Box::new(el("h3", projection.spec.title.clone())),
        Box::new(el(
            "p",
            format!(
                "{} selected Resources; {} unavailable; source revision {}. {coverage}",
                projection.members.len(),
                projection.missing_resources.len(),
                projection.source_revision
            ),
        )),
        Box::new(el(
            "p",
            format!(
                "Resource links: {}. Surface links: {}. Cross-stratum edges: {}. Shown bindings have zero hop cost.",
                projection.spec.styles.resource_link.css(),
                projection.spec.styles.surface_link.css(),
                projection.spec.styles.cross_stratum_edge.css()
            ),
        )),
        Box::new(el("div", diagram).attr(
            "style",
            format!(
                "position:relative;min-width:700px;height:{}px",
                row[0].max(row[1]) * 170 + 30
            ),
        )),
    ];
    for link in &links {
        let source = projection.clone();
        let lease = scope.clone();
        let key = link.key.clone();
        let representative = projection.relations[link.representative].clone();
        let direction = match (link.marks.forward, link.marks.reverse) {
            (true, true) => "↔",
            (true, false) => "→",
            (false, true) => "←",
            (false, false) => "—",
        };
        let opposed = if link.marks.opposed {
            " · opposed"
        } else {
            ""
        };
        children.push(Box::new(button(
            format!(
                "Open {:?} link {} {} {} · {} held contributions{}",
                key.stratum, key.first, direction, key.second, link.strength, opposed
            ),
            move |p: &mut TreePage, _| {
                let result = p
                    .product
                    .as_ref()
                    .ok_or_else(|| "Source unavailable".to_owned())
                    .and_then(|product| {
                        if product.retained_scope() != lease
                            || !source.is_current(product.source_graph()?)
                        {
                            return Err("Query link belongs to an obsolete source".into());
                        }
                        let graph = product.source_graph()?;
                        let (first, second) = match key.stratum {
                            LinkStratum::Surface => (
                                representative.source_from.id,
                                representative.source_to.id,
                            ),
                            LinkStratum::Resource => {
                                let from = source
                                    .nodes
                                    .iter()
                                    .find(|node| {
                                        node.identity == representative.source_from
                                    })
                                    .and_then(|node| node.held_accesses.first())
                                    .copied()
                                    .ok_or("Resource link has no held appearance at its first endpoint")?;
                                let to = source
                                    .nodes
                                    .iter()
                                    .find(|node| node.identity == representative.source_to)
                                    .and_then(|node| node.held_accesses.first())
                                    .copied()
                                    .ok_or("Resource link has no held appearance at its second endpoint")?;
                                source.access_for(graph, representative.source_from.id, from)?;
                                source.access_for(graph, representative.source_to.id, to)?;
                                (from, to)
                            },
                        };
                        Ok(graphshell::link_inspection::LinkTarget::new(first, second))
                    });
                match result {
                    Ok(target) => {
                        if p.inspected_link != Some(target) {
                            p.opened_link_layout = None;
                            p.opened_link_error = None;
                            p.inspected_statement = None;
                            p.inspected_link_member = None;
                        }
                        p.inspected_link = Some(target);
                        p.tools_open = true;
                        p.query_views.editor.notice("Opened source link reading");
                        p.shared.dirty.set(true);
                    },
                    Err(error) => p.query_views.editor.notice(error),
                }
            },
        )));
        children.push(Box::new(el(
            "p",
            format!(
                "{:?} link · {} merged projection rows · forward {} · reverse {} · opposed {} · strength {}",
                link.key.stratum,
                link.rows,
                link.marks.forward,
                link.marks.reverse,
                link.marks.opposed,
                link.strength
            ),
        )));
    }
    for relation in projection
        .relations
        .iter()
        .filter(|relation| relation.kind == model::VisualRelationKind::ShownBinding)
    {
        children.push(Box::new(el(
            "p",
            format!(
                "{} · {:?} {} → {:?} {} · hop cost {}",
                relation.kind.label(),
                relation.source_from.stratum,
                relation.source_from.id,
                relation.source_to.stratum,
                relation.source_to.id,
                relation.hop_cost
            ),
        )));
    }
    let source = projection.clone();
    children.push(Box::new(button(
        "Freeze current query selection",
        move |p: &mut TreePage, _| {
            let result = scope
                .as_ref()
                .ok_or_else(|| String::from("Query owner unavailable"))
                .and_then(|scope| {
                    p.product
                        .as_mut()
                        .ok_or_else(|| String::from("Saved graph unavailable"))?
                        .freeze_query(scope, &source)
                });
            match result {
                Ok(correlation) => {
                    p.query_views.pending_freeze = Some(correlation);
                    p.query_views.editor.notice("Saving frozen selection…");
                },
                Err(e) => p.query_views.editor.notice(e),
            }
        },
    )));
    Box::new(
        el("section", children)
            .attr("aria-label", "Query graph")
            .attr("style", "overflow:auto;max-height:650px"),
    )
}

pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    let pane = &page.query_views;
    let inspection = pane.inspection.as_ref().filter(|(scope, revision, _)| {
        page.product.as_ref().is_some_and(|p| {
            p.retained_scope().as_ref() == Some(scope)
                && p.source_graph().is_ok_and(|g| g.revision() == *revision)
        })
    });
    let inspected =
        inspection.and_then(|(_, _, text)| serde_json::from_str::<QueryRelation>(text).ok());
    let frozen: Vec<_> = page
        .product
        .as_ref()
        .and_then(|p| p.source_graph().ok())
        .map(|g| {
            g.resource_nodes()
                .filter(|r| g.open_frozen_selection(r.id()).is_ok())
                .map(|r| r.id().to_string())
                .collect()
        })
        .unwrap_or_default();
    let projection = pane.editor.lifecycle().owner().and_then(|owner| {
        page.product
            .as_ref()
            .and_then(|p| p.source_graph().ok())
            .and_then(|g| model::project_view(g, owner).ok())
    });
    snapshot
        .with_field(
            "query-projection",
            projection
                .as_ref()
                .and_then(|p| serde_json::to_string(p).ok())
                .unwrap_or_default(),
        )
        .with_field(
            "query-members",
            projection
                .as_ref()
                .map(|p| p.members.len().to_string())
                .unwrap_or_default(),
        )
        .with_field(
            "query-resource-nodes",
            projection
                .as_ref()
                .map(|p| {
                    p.nodes
                        .iter()
                        .filter(|n| n.identity.stratum == GraphStratum::Resource)
                        .count()
                        .to_string()
                })
                .unwrap_or_default(),
        )
        .with_field(
            "query-surface-nodes",
            projection
                .as_ref()
                .map(|p| {
                    p.nodes
                        .iter()
                        .filter(|n| n.identity.stratum == GraphStratum::Surface)
                        .count()
                        .to_string()
                })
                .unwrap_or_default(),
        )
        .with_field("query-frozen-count", frozen.len().to_string())
        .with_field(
            "query-frozen-ids",
            serde_json::to_string(&frozen).unwrap_or_default(),
        )
        .with_field(
            "query-view-owner",
            pane.editor
                .lifecycle()
                .owner()
                .map(|id| id.to_string())
                .unwrap_or_default(),
        )
        .with_field(
            "query-view-status",
            pane.editor.lifecycle().status().to_owned(),
        )
        .with_field(
            "query-inspection-kind",
            inspected
                .as_ref()
                .map(|r| r.kind.label())
                .unwrap_or_default(),
        )
        .with_field(
            "query-inspection-from",
            inspected
                .as_ref()
                .map(|r| format!("{:?}", r.from.stratum))
                .unwrap_or_default(),
        )
        .with_field(
            "query-inspection-source-from",
            inspected
                .as_ref()
                .map(|r| format!("{:?}", r.source_from.stratum))
                .unwrap_or_default(),
        )
        .with_field(
            "query-inspection-hop-cost",
            inspected
                .as_ref()
                .map(|r| r.hop_cost.to_string())
                .unwrap_or_default(),
        )
        .with_field(
            "query-inspection",
            pane.inspection
                .as_ref()
                .filter(|(scope, revision, _)| {
                    page.product.as_ref().is_some_and(|p| {
                        p.retained_scope().as_ref() == Some(scope)
                            && p.source_graph().is_ok_and(|g| g.revision() == *revision)
                    })
                })
                .map(|(_, _, text)| text.clone())
                .unwrap_or_default(),
        )
        .with_field(
            "query-view-pending",
            (!matches!(
                pane.editor.lifecycle().state(),
                model::editor::SaveState::Idle
            ) || pane.pending_freeze.is_some())
            .to_string(),
        )
        .with_field(
            "query-view-edge-stroke",
            pane.editor
                .lifecycle()
                .draft()
                .map_or(StrokeCue::Dashed, |draft| draft.styles.cross_stratum_edge)
                .css(),
        )
}
