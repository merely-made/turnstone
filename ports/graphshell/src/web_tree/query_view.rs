// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Author shared query Resources; read-only views retain source strata and access identities.
use super::*;
use cambium::{TextInput, button, lens, text_field_typed};
use mere::kernel::graph::GraphStratum;
use mere::query_view::{self as model, *};
use uuid::Uuid;
const LABELS: [&str; 5] = [
    "Query view title",
    "SELECT query",
    "Query member budget",
    "Query appearance budget",
    "Query relation budget",
];
const SAVE_FAILED: &str = "Query write failed. Retry the existing pending save in Graph tools.";
pub(super) struct Pane {
    inputs: Vec<TextInput>,
    scope: Option<(Uuid, String)>,
    owner: Option<Uuid>,
    expected: Option<QueryViewSpec>,
    options: QueryViewSpec,
    status: String,
    pending: Option<(Uuid, Uuid)>,
    pending_freeze: Option<((Uuid, String), Uuid)>,
    definition: Option<String>,
    inspection: Option<((Uuid, String), u64, String)>,
}
impl Default for Pane {
    fn default() -> Self {
        Self {
            inputs: vec![
                TextInput::new("Saved query"),
                TextInput::new("SELECT ?member WHERE { ?member ?predicate ?object }"),
                TextInput::new("256"),
                TextInput::new("256"),
                TextInput::new("1024"),
            ],
            scope: None,
            owner: None,
            expected: None,
            options: QueryViewSpec::select("Saved query", ""),
            status: String::new(),
            pending: None,
            pending_freeze: None,
            definition: None,
            inspection: None,
        }
    }
}
impl Pane {
    fn spec(&self) -> Result<QueryViewSpec, String> {
        let mut spec = self.options.clone();
        spec.title = self.inputs[0].text().into();
        spec.set_query_text(self.inputs[1].text())?;
        spec.max_members = self.inputs[2]
            .text()
            .parse()
            .map_err(|_| "Enter a positive member budget")?;
        spec.styles = self.options.styles.clone();
        spec.max_appearances = self.inputs[3]
            .text()
            .parse()
            .map_err(|_| "Enter a positive appearance budget")?;
        spec.max_relations = self.inputs[4]
            .text()
            .parse()
            .map_err(|_| "Enter a positive relation budget")?;
        spec.appearances = self.options.appearances;
        spec.include_tag_concepts = self.options.include_tag_concepts;
        spec.validate()?;
        Ok(spec)
    }
    fn load(&mut self, scope: (Uuid, String), owner: Option<Uuid>, spec: QueryViewSpec) {
        self.inputs = vec![
            TextInput::new(&spec.title),
            TextInput::new(spec.query_text()),
            TextInput::new(spec.max_members.to_string()),
            TextInput::new(spec.max_appearances.to_string()),
            TextInput::new(spec.max_relations.to_string()),
        ];
        self.scope = Some(scope);
        self.owner = owner;
        self.expected = owner.map(|_| spec.clone());
        self.options = spec;
        self.pending = None;
        self.pending_freeze = None;
        self.definition = None;
        self.inspection = None;
        self.status = "Query draft opened".into();
    }
}
fn load(page: &mut TreePage, owner: Option<Uuid>) {
    let result = (|| {
        let product = page.product.as_ref().ok_or("Saved graph unavailable")?;
        if product.selection_locked() {
            return Err("Wait for the current save".into());
        }
        let scope = product.retained_scope().ok_or("Source unavailable")?;
        let spec = match owner {
            Some(id) => model::read_spec(product.source_graph()?, id)?,
            None => Pane::default().spec()?,
        };
        Ok((scope, spec))
    })();
    match result {
        Ok((scope, spec)) => page.query_views.load(scope, owner, spec),
        Err(e) => page.query_views.status = e,
    }
}
fn save(page: &mut TreePage) {
    let result = (|| {
        let pane = &page.query_views;
        let spec = pane.spec()?;
        let scope = pane
            .scope
            .as_ref()
            .ok_or("Open a new or saved query draft first")?;
        page.product
            .as_mut()
            .ok_or("Saved graph unavailable")?
            .save_query(scope, pane.owner, pane.expected.as_ref(), &spec)
            .map(|result| (result, spec))
    })();
    match result {
        Ok(((owner, access), spec)) => {
            let pane = &mut page.query_views;
            pane.owner = Some(owner);
            pane.expected = Some(spec);
            pane.pending = Some((owner, access));
            pane.status = "Saving query view…".into();
        },
        Err(e) => page.query_views.status = e,
    }
    page.shared.dirty.set(true);
}
pub(super) fn ready(page: &TreePage) -> bool {
    let pane = &page.query_views;
    page.product.as_ref().is_some_and(|p| {
        !p.saving
            && ((pane
                .inspection
                .as_ref()
                .is_some_and(|(scope, _, _)| p.retained_scope().as_ref() != Some(scope)))
                || (pane.scope.is_some() && pane.scope != p.retained_scope())
                || ((pane.pending.is_some() || pane.pending_freeze.is_some())
                    && ((p.save_state == "error" && pane.status != SAVE_FAILED)
                        || (!p.selection_locked() && p.save_state != "error"))))
    })
}
pub(super) fn poll(page: &mut TreePage) {
    let pane = &mut page.query_views;
    let Some(product) = page.product.as_ref() else {
        return;
    };
    if product.saving {
        return;
    }
    if pane
        .inspection
        .as_ref()
        .is_some_and(|(scope, _, _)| product.retained_scope().as_ref() != Some(scope))
    {
        pane.inspection = None;
    }
    if let Some((scope, frozen)) = pane.pending_freeze.clone() {
        if product.retained_scope().as_ref() != Some(&scope) {
            pane.pending_freeze = None;
            pane.status = "Frozen selection completion belongs to another owner".into();
            return;
        }
        if product.save_state == "error" {
            pane.status = SAVE_FAILED.into();
            return;
        }
        if !product.selection_locked()
            && product
                .source_graph()
                .is_ok_and(|g| g.open_frozen_selection(frozen).is_ok())
        {
            pane.pending_freeze = None;
            pane.status = "Query selection frozen and saved".into();
        }
    }
    if pane.scope.is_some() && pane.scope != product.retained_scope() {
        *pane = Pane::default();
        pane.status = "Query draft retired after owner change".into();
        return;
    }
    if let Some((owner, access)) = pane.pending {
        if product.save_state == "error" {
            pane.status = SAVE_FAILED.into();
            return;
        }
        if product.selection_locked() {
            return;
        }
        if product
            .source_graph()
            .is_ok_and(|g| model::read_spec(g, owner).ok().as_ref() == pane.expected.as_ref())
        {
            pane.pending = None;
            pane.status = "Query view saved".into();
            super::forme::open_retained(page, access);
        }
    }
}
fn field(index: usize) -> Child {
    Box::new(el(
        "label",
        (
            LABELS[index],
            lens(
                move |input: &mut TextInput| {
                    text_field_typed(input).attr("aria-label", LABELS[index])
                },
                move |page: &mut TreePage| &mut page.query_views.inputs[index],
            ),
        ),
    ))
}
pub(super) fn focused_text(
    runner: &cambium_rootstock::Runner<TreePage, Logic, Child>,
) -> Option<cambium_rootstock::FocusedTextSlot<TreePage>> {
    if runner.state().query_views.scope.is_none()
        || runner
            .state()
            .product
            .as_ref()
            .is_none_or(|p| p.selection_locked())
    {
        return None;
    }
    let node = runner.focus()?;
    let dom = runner.dom();
    let dom = dom.borrow();
    let index = LABELS.iter().position(|label| {
        taproot::matching(&dom, &Selector::role("textbox").containing(*label)).contains(&node)
    })?;
    Some(cambium_rootstock::FocusedTextSlot {
        node,
        get: Box::new(move |page| &page.query_views.inputs[index]),
        get_mut: Box::new(move |page| &mut page.query_views.inputs[index]),
    })
}
pub(super) fn section(page: &TreePage) -> Child {
    let mut children: Vec<Child> = vec![
        Box::new(el("h3", "Saved query views")),
        Box::new(button("New query view", |p: &mut TreePage, _| {
            load(p, None)
        })),
    ];
    if let Some(graph) = page.product.as_ref().and_then(|p| p.source_graph().ok()) {
        for resource in graph.resource_nodes() {
            if let Ok(frozen) = graph.open_frozen_selection(resource.id()) {
                let owner = resource.id();
                let lease = page.product.as_ref().and_then(|p| p.retained_scope());
                children.push(Box::new(el(
                    "p",
                    format!(
                        "Frozen selection {owner} · {} members · source revision {}",
                        frozen.selection.members().len(),
                        frozen.selection.revision()
                    ),
                )));
                children.push(Box::new(button(format!("Inspect frozen selection {owner}"), move |p: &mut TreePage, _| {
                    let result = p.product.as_ref().ok_or("Source unavailable".into()).and_then(|product| {
                        if product.retained_scope() != lease { return Err("Frozen selection belongs to another owner".into()); }
                        let graph = product.source_graph()?;
                        let frozen = graph.open_frozen_selection(owner)?;
                        serde_json::to_string_pretty(&json!({"owner": owner, "selection": frozen.selection, "coverage": frozen.coverage})).map(|text| (graph.revision(), text)).map_err(|e| e.to_string())
                    });
                    match result { Ok((revision, text)) => { if let Some(scope) = lease.clone() { p.query_views.inspection = Some((scope, revision, text)); } }, Err(e) => p.query_views.status = e }
                })));
            }
            if let Ok(spec) = model::read_spec(graph, resource.id()) {
                let owner = resource.id();
                children.push(Box::new(button(
                    format!("Edit query: {}", spec.title),
                    move |p: &mut TreePage, _| load(p, Some(owner)),
                )));
            }
        }
    }
    let pane = &page.query_views;
    if pane.scope.is_some() && page.product.as_ref().is_some_and(|p| !p.selection_locked()) {
        children.extend((0..5).map(field));
        if let Some(graph) = page.product.as_ref().and_then(|p| p.source_graph().ok()) {
            for resource in graph.resource_nodes() {
                if let Some(concept) = graph.resource_tag_concept(resource.id()) {
                    // Compare a serialized literal rather than injecting a foreign IRI into query syntax.
                    let term = serde_json::to_string(resource.canonical_iri())
                        .expect("IRI string serializes");
                    let query = format!(
                        "SELECT ?member WHERE {{ ?member <{}> ?tag . FILTER(STR(?tag) = {term}) }}",
                        mere::kernel::graph::resource::TAGGED_WITH_IRI
                    );
                    children.push(Box::new(button(
                        format!("Use tag: {} ({})", concept.label, concept.owner_iri),
                        move |p: &mut TreePage, _| {
                            p.query_views.options.definition =
                                QueryViewSpec::select("", &query).definition;
                            p.query_views.inputs[1] = TextInput::new(&query);
                        },
                    )));
                }
            }
        }
        children.push(Box::new(button(
            match pane.options.appearances {
                AppearancePolicy::AllHeldAccesses => "Show Resources and held appearances",
                AppearancePolicy::ResourcesOnly => "Show Resources only",
            },
            |p: &mut TreePage, _| {
                let v = &mut p.query_views.options.appearances;
                *v = if *v == AppearancePolicy::AllHeldAccesses {
                    AppearancePolicy::ResourcesOnly
                } else {
                    AppearancePolicy::AllHeldAccesses
                };
            },
        )));
        children.push(Box::new(button(
            format!("Tag context: {}", pane.options.include_tag_concepts),
            |p: &mut TreePage, _| {
                p.query_views.options.include_tag_concepts =
                    !p.query_views.options.include_tag_concepts
            },
        )));
        for index in 0..3 {
            let cue = match index {
                0 => pane.options.styles.resource_link,
                1 => pane.options.styles.surface_link,
                _ => pane.options.styles.cross_stratum_edge,
            };
            children.push(Box::new(button(
                format!(
                    "{} stroke: {}",
                    ["Resource link", "Surface link", "Cross-stratum edge"][index],
                    cue.css()
                ),
                move |p: &mut TreePage, _| {
                    let styles = &mut p.query_views.options.styles;
                    let current = match index {
                        0 => styles.resource_link,
                        1 => styles.surface_link,
                        _ => styles.cross_stratum_edge,
                    };
                    let start = StrokeCue::ALL
                        .iter()
                        .position(|cue| *cue == current)
                        .unwrap();
                    let next = StrokeCue::ALL
                        .iter()
                        .cycle()
                        .skip(start + 1)
                        .take(4)
                        .copied()
                        .find(|cue| {
                            if index < 2 {
                                *cue != styles.cross_stratum_edge
                            } else {
                                *cue != styles.resource_link && *cue != styles.surface_link
                            }
                        })
                        .expect("four cues provide a distinct link and edge choice");
                    match index {
                        0 => styles.resource_link = next,
                        1 => styles.surface_link = next,
                        _ => styles.cross_stratum_edge = next,
                    }
                },
            )));
        }
        children.push(Box::new(button(
            "Save query view",
            |p: &mut TreePage, _| save(p),
        )));
        if let Some(owner) = pane.owner {
            let lease = pane.scope.clone();
            children.push(Box::new(button(
                "Copyable saved query definition",
                move |p: &mut TreePage, _| match p
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
                    }) {
                    Ok(text) => p.query_views.definition = Some(text),
                    Err(e) => p.query_views.status = e,
                },
            )));
        }
    }
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
    children.push(Box::new(
        el("p", pane.status.clone()).attr("role", "status"),
    ));
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
    for relation in &projection.relations {
        let Some((_, ax, ay)) = positions.iter().find(|(id, ..)| *id == relation.from) else {
            continue;
        };
        let Some((_, bx, by)) = positions.iter().find(|(id, ..)| *id == relation.to) else {
            continue;
        };
        let (dx, dy) = (bx - ax, by - ay);
        let length = dx.hypot(dy);
        let angle = dy.atan2(dx).to_degrees();
        let cue = relation.kind.cue(&projection.spec.styles);
        let pieces: Vec<Child> = model::stroke_parts(cue, length).into_iter().map(|part| {
                Box::new(el("div", ()).attr("aria-hidden", "true").attr("style", format!(
                    "display:block;position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;background:currentColor;border-radius:{}px",
                part.offset, part.top, part.width, part.height, if part.rounded { 1.5 } else { 0.0 }
            ))) as Child
        }).collect();
        diagram.push(Box::new(el("div", pieces).attr("role", "img").attr("aria-label", format!("{}: {} to {}; source {:?} to {:?}", relation.kind.label(), relation.from.id, relation.to.id, relation.source_from.stratum, relation.source_to.stratum)).attr("data-query-relation", format!("{:?}", relation.kind)).attr("style", format!("position:absolute;left:{}px;top:{}px;width:{length}px;height:3px;transform-origin:0 0;transform:rotate({angle}deg)", ax+145.0, ay+48.0))));
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
                            Err(e) => p.query_views.status = e,
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
    for (index, relation) in projection.relations.iter().enumerate() {
        let source = projection.clone();
        let lease = scope.clone();
        children.push(Box::new(button(
            format!(
                "Inspect query relation {}: {}",
                index + 1,
                relation.kind.label()
            ),
            move |p: &mut TreePage, _| {
                let result = p
                    .product
                    .as_ref()
                    .ok_or("Source unavailable".into())
                    .and_then(|product| {
                        if product.retained_scope() != lease
                            || !source.is_current(product.source_graph()?)
                        {
                            return Err("Query relation belongs to an obsolete source".into());
                        }
                        serde_json::to_string_pretty(&source.relations[index])
                            .map_err(|e| e.to_string())
                    });
                match result {
                    Ok(text) => {
                        if let Some(scope) = lease.clone() {
                            p.query_views.inspection = Some((scope, source.source_revision, text));
                        }
                    },
                    Err(e) => p.query_views.status = e,
                }
            },
        )));
        children.push(Box::new(el(
            "p",
            format!(
                "{} · {:?} {} → {:?} {} · {} statements · hop cost {}",
                relation.kind.label(),
                relation.source_from.stratum,
                relation.source_from.id,
                relation.source_to.stratum,
                relation.source_to.id,
                relation.statements.len(),
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
                Ok(frozen) => {
                    if let Some(scope) = scope.clone() {
                        p.query_views.pending_freeze = Some((scope, frozen));
                    }
                    p.query_views.status = "Saving frozen selection…".into();
                },
                Err(e) => p.query_views.status = e,
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
    let projection = pane.owner.and_then(|owner| {
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
            pane.owner.map(|id| id.to_string()).unwrap_or_default(),
        )
        .with_field("query-view-status", pane.status.clone())
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
            (pane.pending.is_some() || pane.pending_freeze.is_some()).to_string(),
        )
        .with_field(
            "query-view-edge-stroke",
            pane.options.styles.cross_stratum_edge.css(),
        )
}
