// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Fold the current app-owned snapshot from an open place's stores.
///
/// Factored from the open path so a resync can re-fold WITHOUT dropping the
/// live lanes: the lanes drain received operations into these same stores,
/// and this is where they become authority-filtered, product-visible state.
pub(super) fn place_snapshot(
    open: &OpenPlace,
    moot_id: [u8; 32],
    settings: &PlaceWorkerSettings,
) -> Result<OfflinePlaceSnapshot, String> {
    let moot_snapshot = pollster::block_on(open.moot.snapshot())
        .map_err(|error| format!("materialize Gemot: {error}"))?;
    let at_ms = settings.authority_clock.now_ms();

    // The single authority view both Commons domains project through. It is
    // built from the Moot's own converged constitution and delegation fold, so
    // an operation whose author was never granted, or whose grant was
    // withdrawn, cannot reach a projection this worker emits.
    let delegations = pollster::block_on(open.moot.delegations())
        .map_err(|error| format!("materialize Gemot delegations: {error}"))?;
    let authority = GemotAuthorityView {
        authority: MootAuthority {
            delegations: &delegations,
            rules: &moot_snapshot.governance.rules,
            moot_id,
            now_ms: at_ms,
        },
    };

    // These are refresh-time facts for the local subject, evaluated through
    // the same converged authority and scoped capabilities used by the write
    // preflight above. They describe current effective authority only; a
    // subsequent authoring attempt must still recheck the live fold.
    let message_write = matches!(
        commons::CommonsAuthority::classify(
            &authority,
            servitor::Subject(open.subject),
            &commons::chat::chat_write_capability(open.binding.chat.0),
            servitor::Mode::Write,
        ),
        commons::AuthorityState::Effective
    );
    let graph_write = matches!(
        commons::CommonsAuthority::classify(
            &authority,
            servitor::Subject(open.subject),
            &commons::commons_write_capability(open.binding.root.0),
            servitor::Mode::Write,
        ),
        commons::AuthorityState::Effective
    );

    // The service view applies Gemot's complete converged read policy,
    // including membership and targeted withdrawals. Narrow the roster to
    // that set before any locally held capture text is grouped.
    let authorized_fauna = pollster::block_on(open.moot.authorized_fauna(at_ms))
        .map_err(|error| format!("materialize authorized Gemot fauna: {error}"))?;
    // Raw history only supplies candidate ids. Names and selectable versions
    // come from the service's current authorized projection.
    let collection_ids: BTreeSet<_> = moot_snapshot
        .roster
        .collections
        .iter()
        .map(|fact| match &fact.event {
            CollectionEvent::Declared { collection_id, .. } => *collection_id,
            CollectionEvent::Changed { collection, .. } => collection.collection_id,
        })
        .collect();
    let mut collection_choices = Vec::new();
    for id in collection_ids {
        if let Some(view) = pollster::block_on(open.moot.authorized_collection(id, at_ms))
            .map_err(|error| format!("materialize collection choice: {error}"))?
        {
            collection_choices.push(PlaceCollectionChoice {
                name: view.name,
                version: PlaceCollectionVersion {
                    moot: PlaceId(view.version.collection.moot_id),
                    collection: PlaceCollectionId(view.version.collection.collection_id.0),
                    frontier: view.version.frontier,
                    membership_commitment: view.version.membership_commitment,
                },
            });
        }
    }
    collection_choices.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.version.collection.0.cmp(&b.version.collection.0))
    });
    let (captured_fauna, captured_selection) = match &open.collection_selection {
        None => (
            authorized_fauna.clone(),
            CapturedCollectionSelection::AllEffective,
        ),
        Some(requested) if requested.moot.0 != moot_id => (
            Vec::new(),
            CapturedCollectionSelection::Collection {
                requested: requested.clone(),
                status: CapturedCollectionSelectionStatus::ForeignMoot,
            },
        ),
        Some(requested) => {
            let domain_requested = CollectionVersion {
                collection: CollectionRef {
                    moot_id: requested.moot.0,
                    collection_id: CollectionId(requested.collection.0),
                },
                frontier: requested.frontier.clone(),
                membership_commitment: requested.membership_commitment,
            };
            match pollster::block_on(
                open.moot
                    .authorized_collection(domain_requested.collection.collection_id, at_ms),
            )
            .map_err(|error| format!("materialize selected Gemot collection: {error}"))?
            {
                None => (
                    Vec::new(),
                    CapturedCollectionSelection::Collection {
                        requested: requested.clone(),
                        status: CapturedCollectionSelectionStatus::Unavailable,
                    },
                ),
                Some(view) if view.version != domain_requested => {
                    let current = PlaceCollectionVersion {
                        moot: PlaceId(view.version.collection.moot_id),
                        collection: PlaceCollectionId(view.version.collection.collection_id.0),
                        frontier: view.version.frontier,
                        membership_commitment: view.version.membership_commitment,
                    };
                    (
                        Vec::new(),
                        CapturedCollectionSelection::Collection {
                            requested: requested.clone(),
                            status: CapturedCollectionSelectionStatus::Stale { current },
                        },
                    )
                },
                Some(view) => {
                    let shares: BTreeSet<_> = view
                        .effective_selected
                        .iter()
                        .map(|reference| reference.share)
                        .collect();
                    let effective: Vec<_> = authorized_fauna
                        .iter()
                        .filter(|entry| shares.contains(&entry.op_hash))
                        .cloned()
                        .collect();
                    let status = CapturedCollectionSelectionStatus::Ready {
                        name: view.name,
                        effective_contributions: effective.len(),
                        pending_facts: view.pending.len(),
                    };
                    (
                        effective,
                        CapturedCollectionSelection::Collection {
                            requested: requested.clone(),
                            status,
                        },
                    )
                },
            }
        },
    };
    let captured_roster =
        crate::place::captured_collection::effective_roster(&moot_snapshot.roster, &captured_fauna);
    let captured = crate::place::captured_collection::remint(
        &captured_roster,
        &authority.authority,
        &open.capture_directory,
        &settings.capture_library,
    );

    let graph_projection = pollster::block_on(open.graph.projection_with_authority(&authority))
        .map_err(|error| format!("materialize Commons graph: {error}"))?;
    let chat_projection = pollster::block_on(open.chat.projection_with_authority(&authority))
        .map_err(|error| format!("materialize Commons chat: {error}"))?;

    let group_members = open
        .group
        .session
        .members()
        .map_err(|error| format!("materialize group membership: {error}"))?
        .len();
    let (parked, evicted) = open.parking()?;
    let shared = crate::place::projection::SharedGraph::from_projection(&graph_projection);
    Ok(OfflinePlaceSnapshot {
        personae_root: open.subject,
        graph_digest: projection_digest(b"turnstone.place.graph-digest.v1", &shared),
        chat_digest: projection_digest(
            b"turnstone.place.chat-digest.v1",
            &canonical_chat(&chat_projection),
        ),
        moot: MootCache {
            membership_epoch: moot_snapshot.membership.epoch,
            members: moot_snapshot.membership.members.len(),
            roster_members: moot_snapshot.roster.members.len(),
            delegated_certificates: moot_snapshot.delegated_certificates,
            standing_operations: moot_snapshot.standing_operations,
        },
        graph: GraphCache {
            nodes: graph_projection.graph.graph().node_count(),
            edges: graph_projection.graph.graph().edge_count(),
            pending_causality: graph_projection.pending.len(),
            pending_authority: graph_projection.pending_authority.len(),
            revoked_authority: graph_projection.revoked.len(),
        },
        chat: ChatCache {
            channels: chat_projection.channels.len(),
            messages: chat_projection.messages.len(),
            deleted_messages: chat_projection.deleted_messages.len(),
            pending_causality: chat_projection.pending.len(),
            pending_authority: chat_projection.pending_authority.len(),
            revoked_authority: chat_projection.revoked.len(),
        },
        group: GroupCache {
            members: group_members,
            epochs: open.group.session.epoch_count(),
            has_current_epoch: open.group.session.current_epoch().is_some(),
            pending_frames: open.group.pending,
            refused_frames: open.group.refused,
            parked_records: parked,
            readmitted_records: open.readmitted,
            evicted_records: evicted,
        },
        captured,
        captured_selection,
        collection_choices,
        shared,
        sync: open.lanes.as_ref().map(|lanes| {
            let (local_rendezvous, dialed_rendezvous) = lanes.rendezvous();
            crate::place::PlaceSyncSnapshot {
                lanes: lanes.sync_snapshot(),
                projection: lanes.projection_snapshot(),
                local_rendezvous,
                dialed_rendezvous,
            }
        }),
        permissions: Some(crate::place::PlacePermissionSnapshot {
            message_write,
            graph_write,
        }),
        standing: local_standing(
            &moot_snapshot.membership.members,
            &delegations,
            &moot_snapshot.governance.rules,
            moot_id,
            open.subject,
            at_ms,
        ),
        members: moot_snapshot
            .membership
            .members
            .iter()
            .map(|member| crate::place::PlaceMember {
                root: member.member,
                access: match member.access {
                    MootAccessLevel::Pull => crate::place::PlaceMemberAccess::Pull,
                    MootAccessLevel::Read => crate::place::PlaceMemberAccess::Read,
                    MootAccessLevel::Write => crate::place::PlaceMemberAccess::Write,
                    MootAccessLevel::Manage => crate::place::PlaceMemberAccess::Manage,
                },
            })
            .collect(),
    })
}
