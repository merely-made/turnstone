// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A mere's route as the mere view (reservoir plan V2b step 3, §7 items 35
//! and 37). The route serves two projections, the mere's sessions and the
//! attached session's graph. This fills a [`MereViewModel`] from them and
//! turns the view's requests into the route's intents.
//!
//! It holds no connection. The host reads the route through the client it
//! has, djinn's door in process or `AppBrokerClient`, and hands the snapshots
//! and the session cards' bytes here. A relation shows its kernel family as
//! its kind (§7 item 43).

use std::collections::HashMap;

use chirograph::{
    ContentHash, EndpointDescriptor, IntentInvocation, IntentResult, PortableCardV1,
    PresentationOffer, ProjectionRequest, ProjectionSnapshot, ResourceRequest,
};
use mere::canvas::MERE_GRAPH_ADAPTER;
use mere_view::{
    GraphModel, MereViewModel, MereViewRequest, NodeEntry, NodeState, Provenance, RelationEntry,
    SessionEntry, SessionStep,
};
use pandect::SessionId;
use sceno::{InstanceId, SourceRef};
use uuid::Uuid;

use crate::session_item::{
    ATTACH_SESSION_INTENT, FORK_SESSION_INTENT, MERE_GRAPH, MERE_SESSIONS, MINT_SESSION_INTENT,
    RESTORE_SESSION_INTENT, SESSION_ITEM_SOURCE, SESSIONS_SOURCE, SessionsActionV1,
    TRASH_SESSION_INTENT,
};

/// Each step a session's card can offer, and the intent that takes it.
const STEPS: [(&str, SessionStep); 4] = [
    (ATTACH_SESSION_INTENT, SessionStep::Switch),
    (FORK_SESSION_INTENT, SessionStep::Fork),
    (TRASH_SESSION_INTENT, SessionStep::Trash),
    (RESTORE_SESSION_INTENT, SessionStep::Restore),
];

/// The route's two projections from its descriptor: sessions, then graph.
pub fn requests(descriptor: &EndpointDescriptor) -> Option<(ProjectionRequest, ProjectionRequest)> {
    let find = |name: &str| {
        descriptor
            .projections
            .iter()
            .find(|offer| offer.request.session.0 == name)
            .map(|offer| offer.request.clone())
    };
    Some((find(MERE_SESSIONS)?, find(MERE_GRAPH)?))
}

/// The cards to read before [`fill`]: the mere's and each session's.
pub fn card_requests(sessions: &ProjectionSnapshot) -> Vec<ResourceRequest> {
    sessions
        .presentation
        .offers
        .values()
        .filter_map(|offers| offers.first())
        .map(|offer| ResourceRequest {
            session: sessions.session.clone(),
            resource: offer.resource,
        })
        .collect()
}

/// Fill `model` from the route's projections and the cards
/// [`card_requests`] named, by content hash. The host's own parts stay: its
/// status, layout, actions and notice.
pub fn fill(
    model: &mut MereViewModel,
    sessions: &ProjectionSnapshot,
    graph: &ProjectionSnapshot,
    cards: &HashMap<ContentHash, Vec<u8>>,
) {
    // The session this connection is attached to is the one its graph shows.
    let attached = items(graph, SESSION_ITEM_SOURCE)
        .next()
        .and_then(|(_, source)| Uuid::parse_str(&source.id).ok())
        .map(SessionId);
    model.sessions.clear();
    model.can_mint = false;
    for (instance, source) in items(sessions, SESSIONS_SOURCE) {
        let Some(offer) = offer(sessions, instance) else {
            continue;
        };
        let title = cards
            .get(&offer.resource)
            .and_then(|bytes| serde_json::from_slice::<PortableCardV1>(bytes).ok())
            .map(|card| card.title)
            .unwrap_or_else(|| offer.semantics.label.clone());
        let offers = |intent: &str| {
            offer
                .semantics
                .actions
                .iter()
                .any(|action| action.intent.0 == intent)
        };
        // A session's card is keyed by the session's id; the mere's is not.
        let Ok(id) = Uuid::parse_str(&source.id).map(SessionId) else {
            model.title = title;
            model.can_mint = offers(MINT_SESSION_INTENT);
            continue;
        };
        model.sessions.push(SessionEntry {
            id,
            name: title,
            detail: None,
            attached: attached == Some(id),
            trashed: offers(RESTORE_SESSION_INTENT),
            steps: STEPS
                .into_iter()
                .filter(|(intent, _)| offers(intent))
                .map(|(_, step)| step)
                .collect(),
        });
    }
    model.graph = graph_model(graph);
}

/// The attached session's nodes and relations. Each relation stays apart,
/// keyed by its endpoints, its kind and its place among relations that share
/// both.
fn graph_model(graph: &ProjectionSnapshot) -> GraphModel {
    let mut keys = HashMap::new();
    let mut nodes = Vec::new();
    for (instance, source) in items(graph, MERE_GRAPH_ADAPTER) {
        let label = offer(graph, instance)
            .map(|offer| offer.semantics.label.clone())
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| source.id.clone());
        keys.insert(instance, source.id.clone());
        nodes.push(NodeEntry {
            key: source.id.clone(),
            label,
            state: NodeState::Available,
        });
    }
    let mut places: HashMap<(String, String, String), usize> = HashMap::new();
    let mut relations = Vec::new();
    for relation in graph.scene.tables.relations.iter().flatten() {
        let (Some(from), Some(to)) = (keys.get(&relation.from), keys.get(&relation.to)) else {
            continue;
        };
        let kind = relation
            .kind
            .clone()
            .unwrap_or_else(|| "related".to_string());
        let place = places
            .entry((from.clone(), to.clone(), kind.clone()))
            .or_default();
        relations.push(RelationEntry {
            key: format!("{from}>{to}:{kind}:{place}"),
            from: from.clone(),
            to: to.clone(),
            provenance: Provenance::Other(kind),
        });
        *place += 1;
    }
    GraphModel { nodes, relations }
}

/// The route's intent for a request, or `None` for one that is the host's
/// own: opening a node, a host action or a layout. A step names its session
/// by id, so it holds however the list has moved (§7 item 33).
pub fn intent(
    request: &MereViewRequest,
    sessions: &ProjectionSnapshot,
) -> Option<IntentInvocation> {
    let (intent, payload, target) = match request {
        MereViewRequest::Mint => (
            MINT_SESSION_INTENT,
            SessionsActionV1::mint(None),
            mere_card(sessions),
        ),
        MereViewRequest::Session(id, step) => {
            let (intent, _) = STEPS.into_iter().find(|(_, of)| of == step)?;
            let card = items(sessions, SESSIONS_SOURCE)
                .find(|(_, source)| Uuid::parse_str(&source.id).ok() == Some(*id.as_uuid()))
                .map_or_else(|| mere_card(sessions), |(instance, _)| instance);
            (intent, SessionsActionV1::on(*id), card)
        },
        MereViewRequest::Activate(_) | MereViewRequest::Host(_) | MereViewRequest::Layout(_) => {
            return None;
        },
    };
    Some(IntentInvocation {
        session: sessions.session.clone(),
        target,
        observed_epoch: sessions.scene.epoch,
        observed_revision: sessions.scene.revision,
        intent: intent.into(),
        payload: serde_json::to_vec(&payload).expect("SessionsActionV1 always serializes"),
    })
}

/// What the view shows when the route declines a step: its reason.
pub fn declined(result: &IntentResult) -> Option<String> {
    match result {
        IntentResult::Accepted => None,
        IntentResult::Rejected { reason } => Some(reason.clone()),
        IntentResult::Stale { .. } => Some("The sessions changed; read them again.".to_string()),
    }
}

/// The items whose source is `adapter`'s, in order.
fn items<'a>(
    snapshot: &'a ProjectionSnapshot,
    adapter: &'a str,
) -> impl Iterator<Item = (InstanceId, &'a SourceRef)> + 'a {
    snapshot
        .scene
        .active_items_in_order()
        .into_iter()
        .filter_map(move |(instance, item)| {
            let source = snapshot.scene.tables.sources[item.source.0 as usize].as_ref()?;
            (source.adapter == adapter).then_some((instance, source))
        })
}

fn offer(snapshot: &ProjectionSnapshot, instance: InstanceId) -> Option<&PresentationOffer> {
    let binding = snapshot
        .presentation
        .bindings
        .iter()
        .find(|binding| binding.instance == instance)?;
    snapshot.presentation.offers.get(&binding.key)?.first()
}

/// The mere's own card, which offers minting.
fn mere_card(sessions: &ProjectionSnapshot) -> InstanceId {
    items(sessions, SESSIONS_SOURCE)
        .find(|(_, source)| Uuid::parse_str(&source.id).is_err())
        .map_or(InstanceId(0), |(instance, _)| instance)
}
