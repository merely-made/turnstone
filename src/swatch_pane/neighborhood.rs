// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A read-only projection of links observed in the graph's current HTML sources.

use std::collections::{BTreeMap, BTreeSet};

mod extraction;
#[cfg(test)]
mod tests;

use genet_static_dom::StaticDocument;
use graphshell::link_inspection::{
    LinkObservation as Claim, LinkTarget, ObservationSource, ObservedLinkInspection,
};
use url::Url;

use super::{NodeState, ProjectionPreset, SwatchModel, SwatchNode, SwatchRelation, Uuid};
use crate::app::App;
use crate::panes::{GraphId, PaneId};
use extraction::{document_base, excerpt, extract, link_claim};

pub(crate) const RESOURCE_NEIGHBORHOOD: ProjectionPreset = ProjectionPreset {
    id: "resource.neighborhood",
    label: "Resource neighborhood",
    leaf_key: 3,
    node_labels: true,
    expand: false,
    gather: |_| SwatchModel::default(),
};

const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;
const MAX_CLAIMS: usize = 512;
const MAX_NEIGHBORS: usize = 16;
const MAX_SOURCES: usize = 32;
const MAX_EVIDENCE_ROWS: usize = 32;

#[derive(Default)]
struct Page {
    address: String,
    label: Option<String>,
    claims: Vec<Claim>,
    partial: bool,
}

#[derive(Default)]
pub(super) struct Sources {
    graph: Option<GraphId>,
    pages: BTreeMap<Uuid, (Uuid, Page)>,
    displayed_focus: Option<String>,
    model: SwatchModel,
    passage: Option<Passage>,
    truncated: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Passage {
    pub(crate) member: Uuid,
    address: String,
    admission: Uuid,
    claims: Vec<Claim>,
}

impl Passage {
    pub(crate) fn from_clip(
        member: Uuid,
        document: &crate::content::FetchedDocument,
        identity: &crate::source_capture::SourceIdentity,
        admission: Uuid,
        clip: inker::DocumentClip,
    ) -> Option<Self> {
        let source = Url::parse(identity.effective_url.as_deref()?).ok()?;
        let actual = Url::parse(&clip.source_url).ok()?;
        if source != actual || clip.text.is_empty() {
            return None;
        }
        let selector: serde_json::Value = serde_json::from_str(clip.selector.as_deref()?).ok()?;
        if selector["type"] != "dom-range" || selector["version"] != 1 {
            return None;
        }
        let orders: BTreeSet<usize> =
            serde_json::from_value(selector.get("link_occurrence_orders")?.clone()).ok()?;
        let (html, hash, acquisition) = passage_representation(&clip, document, identity, &source)?;
        if html.len() > MAX_HTML_BYTES {
            return None;
        }
        let dom = StaticDocument::parse(html);
        let base = document_base(&dom, &source);
        let claims = fleece::extract_link_occurrences(&dom)
            .into_iter()
            .filter(|occurrence| {
                orders.contains(&occurrence.document_order)
                    && clip.links.contains(&occurrence.link.href)
            })
            .take(MAX_CLAIMS)
            .filter_map(|occurrence| {
                link_claim(occurrence, &base, source.as_str(), &hash, acquisition)
            })
            .collect();
        Some(Self {
            member,
            address: source.to_string(),
            admission,
            claims,
        })
    }
}

fn passage_representation<'a>(
    clip: &'a inker::DocumentClip,
    document: &'a crate::content::FetchedDocument,
    identity: &crate::source_capture::SourceIdentity,
    source: &Url,
) -> Option<(&'a str, String, ObservationSource)> {
    use inker::DocumentClipArtifactRole::{ObservedRepresentation, SourceResponse};

    let artifact = clip.artifacts.iter().find(|artifact| {
        artifact.role == ObservedRepresentation
            && Url::parse(&artifact.canonical_uri).ok().as_ref() == Some(source)
            && artifact
                .media_type
                .split(';')
                .next()
                .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("text/html"))
    });
    if let Some(artifact) = artifact {
        let html = std::str::from_utf8(&artifact.bytes).ok()?;
        return Some((
            html,
            eidetic::Hash::of(&artifact.bytes).to_string(),
            ObservationSource::ObservedDocument,
        ));
    }
    clip.artifacts.iter().find(|artifact| {
        artifact.role == SourceResponse
            && Url::parse(&artifact.canonical_uri).ok().as_ref() == Some(source)
            && artifact.bytes == document.bytes
    })?;
    Some((
        &document.body,
        identity.hash.clone(),
        ObservationSource::Response,
    ))
}

#[derive(Default)]
pub(super) struct Selection {
    origin: Option<String>,
    pub(super) focus: Option<String>,
    pub(super) link: Option<String>,
    pub(super) details: BTreeMap<String, ObservedLinkInspection>,
    pub(super) status: String,
}

impl Sources {
    pub(super) fn sync(
        &mut self,
        app: &App,
        pane: PaneId,
        passage: Option<&Passage>,
        selection: &mut Selection,
    ) -> SwatchModel {
        let context = app.follower_context(pane);
        let graph_id = context
            .and_then(|context| context.graph)
            .or_else(|| app.graph_for_pane(pane));
        if self.graph != graph_id {
            *self = Self {
                graph: graph_id,
                ..Self::default()
            };
            *selection = Selection::default();
        }
        let Some(canvas) = graph_id.and_then(|id| app.graph_runtimes.canvas(id)) else {
            *self = Self {
                graph: graph_id,
                ..Self::default()
            };
            *selection = Selection::default();
            selection.status = "Open a document in this graph to see its resource links.".into();
            return SwatchModel::default();
        };
        let graph = canvas.graph();
        let mut live = BTreeSet::new();
        let mut changed = false;
        let mut admitted: Vec<_> = app
            .content
            .source_capture_members()
            .filter_map(|member| {
                let (_, node) = graph.get_node_by_id(member)?;
                let (token, identity) = app.content.source_capture_admission(member, node.url())?;
                Some((member, node.url(), token, identity))
            })
            .collect();
        admitted.sort_by_key(|(member, _, _, source)| {
            (
                context.and_then(|context| context.member) != Some(*member),
                std::cmp::Reverse(source.acquired_at_ms),
                *member,
            )
        });
        let truncated = admitted.len() > MAX_SOURCES;
        changed |= truncated != self.truncated;
        self.truncated = truncated;
        for (member, address, token, identity) in admitted.into_iter().take(MAX_SOURCES) {
            live.insert(member);
            if self
                .pages
                .get(&member)
                .is_some_and(|(seen, _)| *seen == token)
            {
                continue;
            }
            if let Some(document) = app.content.fetched(member, address) {
                self.pages
                    .insert(member, (token, extract(document, &identity.hash)));
                changed = true;
            }
        }
        let previous_len = self.pages.len();
        self.pages.retain(|member, _| live.contains(member));
        changed |= previous_len != self.pages.len();
        let origin = context
            .and_then(|context| context.member)
            .or_else(|| canvas.focused_member())
            .and_then(|member| {
                self.pages
                    .get(&member)
                    .map(|(_, page)| page.address.clone())
                    .or_else(|| {
                        graph
                            .get_node_by_id(member)
                            .map(|(_, node)| node.url().to_string())
                    })
            });
        if selection.origin != origin {
            selection.origin = origin.clone();
            selection.focus = origin;
            selection.link = None;
        }
        let passage = passage.filter(|passage| {
            self.pages
                .get(&passage.member)
                .is_some_and(|(token, _)| *token == passage.admission)
                && selection.origin.as_deref() == Some(&passage.address)
        });
        if self.passage.as_ref() != passage {
            self.passage = passage.cloned();
            if let Some(passage) = passage {
                selection.focus = Some(passage.address.clone());
                selection.link = None;
            }
            changed = true;
        }
        if changed || self.displayed_focus != selection.focus || selection.status.is_empty() {
            self.model = project(
                self.pages.values().map(|(_, page)| page),
                self.passage.as_ref(),
                self.truncated,
                selection,
            );
            self.displayed_focus.clone_from(&selection.focus);
        }
        self.model.clone()
    }
}

fn node_id(address: &str) -> Uuid {
    Uuid::from_bytes(
        eidetic::Hash::of(address.as_bytes()).as_bytes()[..16]
            .try_into()
            .unwrap(),
    )
}

fn project<'a>(
    pages: impl Iterator<Item = &'a Page>,
    passage: Option<&'a Passage>,
    truncated: bool,
    selection: &mut Selection,
) -> SwatchModel {
    selection.details.clear();
    let Some(focus) = selection.focus.as_deref() else {
        selection.status = "Open a document in this graph to see its resource links.".into();
        return SwatchModel::default();
    };
    let mut links: BTreeMap<&str, BTreeSet<&Claim>> = BTreeMap::new();
    let mut source_labels = BTreeMap::new();
    let mut partial = truncated;
    let passage = passage.filter(|passage| passage.address == focus);
    let mut gather = |claim: &'a Claim| {
        let neighbor = if claim.from == focus {
            claim.to.as_str()
        } else if claim.to == focus {
            claim.from.as_str()
        } else {
            return;
        };
        if neighbor != focus {
            links.entry(neighbor).or_default().insert(claim);
        }
    };
    for page in pages {
        source_labels.insert(page.address.as_str(), page.label.as_deref());
        partial |= page.partial;
        if passage.is_none() {
            page.claims.iter().for_each(&mut gather);
        }
    }
    if let Some(passage) = passage {
        passage.claims.iter().for_each(&mut gather);
    }
    let mut ranked: Vec<_> = links
        .iter()
        .map(|(&address, claims)| {
            let sources: BTreeSet<_> = claims.iter().map(|claim| &claim.source).collect();
            (std::cmp::Reverse(sources.len()), address)
        })
        .collect();
    ranked.sort_unstable();
    let visible: BTreeSet<_> = ranked
        .into_iter()
        .take(MAX_NEIGHBORS)
        .map(|(_, address)| address)
        .collect();
    let mut model = SwatchModel {
        selected: Some(node_id(focus)),
        ..SwatchModel::default()
    };
    let count = visible.len().max(1);
    for (i, address) in visible
        .iter()
        .copied()
        .chain(std::iter::once(focus))
        .enumerate()
    {
        let angle = i as f32 / count as f32 * std::f32::consts::TAU;
        let position = if address == focus {
            (0.5, 0.5)
        } else {
            (0.5 + 0.36 * angle.cos(), 0.5 + 0.36 * angle.sin())
        };
        let label = source_labels
            .get(address)
            .copied()
            .flatten()
            .unwrap_or(address);
        model.nodes.push(SwatchNode {
            id: node_id(address),
            position,
            state: if source_labels.contains_key(address) {
                NodeState::Open
            } else {
                NodeState::Idle
            },
            label: excerpt(label),
            key: Some(address.into()),
            activate: None,
        });
    }
    for &neighbor in &visible {
        let claims = &links[neighbor];
        let (from, to) = if claims.iter().all(|claim| claim.to == focus) {
            (neighbor, focus)
        } else {
            (focus, neighbor)
        };
        let relation = SwatchRelation {
            from: node_id(from),
            to: node_id(to),
            kind: "resource-reference",
            label: format!("{} observations", claims.len()),
        };
        selection.details.insert(
            relation.id(),
            ObservedLinkInspection {
                target: LinkTarget::new(relation.from, relation.to),
                observations: claims
                    .iter()
                    .take(MAX_EVIDENCE_ROWS)
                    .copied()
                    .cloned()
                    .collect(),
                total: claims.len(),
            },
        );
        model.relations.push(relation);
    }
    if selection
        .link
        .as_ref()
        .is_some_and(|id| !selection.details.contains_key(id))
    {
        selection.link = None;
    }
    selection.status = if links.is_empty() {
        format!("{focus} · No links found in the available page sources.")
    } else {
        format!(
            "{focus} · {} of {} nearby resources",
            visible.len(),
            links.len()
        )
    };
    if passage.is_some() {
        selection.status.push_str(" · Selected passage");
    }
    if partial {
        selection
            .status
            .push_str(" · Some source data was unavailable or exceeded extraction limits.");
    }
    model
}
