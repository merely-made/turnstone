// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A read-only projection of links observed in the graph's current HTML sources.

use std::collections::{BTreeMap, BTreeSet};

mod extraction;
mod reopen;
pub(crate) use reopen::{NeighborhoodRead, NeighborhoodReply};
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
use crate::saved_sources::KeptSource;
use crate::trail_memory::KeptDocumentRead;
pub(crate) use extraction::{document_base, interpret_blocks};
use extraction::{excerpt, extract, link_claim};
use reopen::Edition;

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
pub(super) struct Page {
    address: String,
    label: Option<String>,
    claims: Vec<Claim>,
    pub(super) structured: Option<mere::acquire::page_claims::PageClaims>,
    partial: bool,
}

#[derive(Default)]
pub(super) struct Sources {
    graph: Option<GraphId>,
    generation: Option<Uuid>,
    pages: BTreeMap<Uuid, Edition>,
    displayed_focus: Option<String>,
    model: SwatchModel,
    passage: Option<Passage>,
    truncated: bool,
    changed: bool,
}

enum Candidate<'a> {
    Live {
        address: &'a str,
        token: Uuid,
        hash: &'a str,
    },
    Kept(KeptSource),
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
    pub(super) validation: super::validation::State,
}

impl Sources {
    pub(super) fn sync(
        &mut self,
        app: &App,
        pane: PaneId,
        passage: Option<&Passage>,
        selection: &mut Selection,
    ) -> (SwatchModel, Vec<NeighborhoodRead>) {
        let context = app.follower_context(pane);
        let graph_id = context
            .and_then(|context| context.graph)
            .or_else(|| app.graph_for_pane(pane));
        let generation = Some(app.source_capture_generation);
        if self.graph != graph_id || self.generation != generation {
            *self = Self {
                graph: graph_id,
                generation,
                ..Self::default()
            };
            *selection = Selection::default();
        }
        let Some((graph_id, canvas)) =
            graph_id.and_then(|id| app.graph_runtimes.canvas(id).map(|canvas| (id, canvas)))
        else {
            *self = Self {
                graph: graph_id,
                generation,
                ..Self::default()
            };
            *selection = Selection::default();
            selection.status = "Open a document in this graph to see its resource links.".into();
            return (SwatchModel::default(), Vec::new());
        };
        let graph = canvas.graph();
        let mut present = BTreeSet::new();
        let mut requests = Vec::new();
        let mut changed = std::mem::take(&mut self.changed);
        let member = context
            .and_then(|context| context.member)
            .or_else(|| canvas.focused_member());
        let mut admitted: Vec<_> = app
            .content
            .source_capture_members()
            .filter_map(|member| {
                let (_, node) = graph.get_node_by_id(member)?;
                let (token, identity) = app.content.source_capture_admission(member, node.url())?;
                Some((
                    member,
                    identity.acquired_at_ms,
                    Candidate::Live {
                        address: node.url(),
                        token,
                        hash: identity.hash.as_str(),
                    },
                ))
            })
            .collect();
        admitted.extend(
            app.offline_kept_sources(graph_id)
                .into_iter()
                .map(|(source, time)| (source.key.node, time, Candidate::Kept(source))),
        );
        admitted.sort_by_key(|(candidate, time, _)| {
            (
                member != Some(*candidate),
                std::cmp::Reverse(*time),
                *candidate,
            )
        });
        let truncated = admitted.len() > MAX_SOURCES;
        changed |= truncated != self.truncated;
        self.truncated = truncated;
        for (node, _, candidate) in admitted.into_iter().take(MAX_SOURCES) {
            match candidate {
                Candidate::Live {
                    address,
                    token,
                    hash,
                } => {
                    if let Some(Edition::Live { token: seen, .. }) = self.pages.get(&node)
                        && *seen == token
                    {
                        present.insert(node);
                    } else if let Some(document) = app.content.fetched(node, address) {
                        self.pages.insert(
                            node,
                            Edition::Live {
                                token,
                                page: extract(document, hash),
                            },
                        );
                        present.insert(node);
                        changed = true;
                    }
                },
                Candidate::Kept(source) => {
                    if let Some(Edition::Kept { source: seen, .. }) = self.pages.get(&node)
                        && seen == &source
                    {
                        present.insert(node);
                    } else if let Some(attempt) = app.admit_structured_validation(&source)
                        && let Some(stored) = &attempt.stored
                    {
                        let request = Uuid::new_v4();
                        requests.push(NeighborhoodRead {
                            pane,
                            request,
                            read: KeptDocumentRead {
                                generation: app.source_capture_generation,
                                source: source.clone(),
                                capture: attempt.request.clone(),
                                stored: stored.clone(),
                                directory: crate::trail_memory::memory_dir(&app.session_dir()),
                            },
                        });
                        self.pages.insert(
                            node,
                            Edition::Kept {
                                source,
                                request,
                                result: None,
                            },
                        );
                        present.insert(node);
                        changed = true;
                    }
                },
            }
        }
        let previous_len = self.pages.len();
        self.pages.retain(|member, _| present.contains(member));
        changed |= previous_len != self.pages.len();
        selection.validation.sync(
            app,
            member,
            member.and_then(|member| self.pages.get(&member)?.page()),
        );
        let origin = member.and_then(|member| {
            self.pages
                .get(&member)
                .and_then(Edition::page)
                .map(|page| page.address.clone())
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
            matches!(self.pages.get(&passage.member), Some(Edition::Live { token, .. }) if *token == passage.admission)
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
                self.pages.values().filter_map(Edition::page),
                self.passage.as_ref(),
                self.truncated,
                selection,
            );
            if self
                .pages
                .values()
                .any(|edition| matches!(edition, Edition::Kept { result: None, .. }))
            {
                selection.status.push_str(" Loading kept sources…");
            }
            if self.pages.values().any(|edition| {
                matches!(
                    edition,
                    Edition::Kept {
                        result: Some(Err(_)),
                        ..
                    }
                )
            }) {
                selection
                    .status
                    .push_str(" Some kept sources could not be reopened.");
            }
            self.displayed_focus.clone_from(&selection.focus);
        }
        (self.model.clone(), requests)
    }

    pub(super) fn accept_reopen(&mut self, reply: NeighborhoodReply) {
        if self.generation != Some(reply.generation) || self.graph != Some(reply.source.key.graph) {
            return;
        }
        if let Some(Edition::Kept {
            source,
            request,
            result,
        }) = self.pages.get_mut(&reply.source.key.node)
            && *request == reply.request
            && *source == reply.source
            && result.is_none()
        {
            *result = Some(reply.result);
            self.changed = true;
        }
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
