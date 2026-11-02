// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Author one fact locally, then push it onto the live lane.
///
/// Preflight first: Turnstone refuses its own unauthorized command rather
/// than authoring an operation that every peer would then filter out of its
/// projection. That is the local half of the two authority paths — the other
/// being that received operations are stored whatever their verdict, so a
/// later re-evaluation can still reverse it.
///
/// Publishing is separate from authoring on purpose. Authoring stores the
/// operation, which is what makes it survive; publishing is what makes it
/// arrive. A place with no live lanes authors happily and syncs when it next
/// joins.
pub(super) fn author_into_place(
    open: &mut OpenPlace,
    binding: &PlaceBindingV1,
    identity: &dyn IdentityProvider,
    command: &PlaceCommand,
    settings: &PlaceWorkerSettings,
) -> Result<(), String> {
    let at_ms = settings.authority_clock.now_ms();
    let subject = identity.master_public_key().to_bytes();
    let needed = match command {
        PlaceCommand::SendMessage { .. } => commons::chat::chat_write_capability(binding.chat.0),
        PlaceCommand::ShareNode { .. } => commons::commons_write_capability(binding.root.0),
        // Governance, not content: its own preflight is holding Manage.
        PlaceCommand::RevokeMember { member } => {
            return revoke_member(open, identity, *member, at_ms);
        },
    };
    let moot_snapshot = pollster::block_on(open.moot.snapshot())
        .map_err(|error| format!("materialize Gemot: {error}"))?;
    let delegations = pollster::block_on(open.moot.delegations())
        .map_err(|error| format!("materialize Gemot delegations: {error}"))?;
    let authority = GemotAuthorityView {
        authority: MootAuthority {
            delegations: &delegations,
            rules: &moot_snapshot.governance.rules,
            moot_id: binding.moot.0,
            now_ms: at_ms,
        },
    };
    if !matches!(
        commons::CommonsAuthority::classify(
            &authority,
            servitor::Subject(subject),
            &needed,
            servitor::Mode::Write,
        ),
        commons::AuthorityState::Effective
    ) {
        const REFUSED: &str = "this profile holds no effective capability to author here";
        let standing = local_standing(
            &moot_snapshot.membership.members,
            &delegations,
            &moot_snapshot.governance.rules,
            binding.moot.0,
            subject,
            at_ms,
        );
        return Err(match standing.refusal() {
            Some(reason) => format!("{REFUSED}: {reason}"),
            None => REFUSED.to_string(),
        });
    }
    // Seal to the newest epoch the lane already holds, not the last nudge's.
    open.follow_group_keys(identity)?;

    match command {
        PlaceCommand::SendMessage { channel, body } => {
            if body.trim().is_empty() {
                return Err("a message needs a body".to_string());
            }
            let operation = pollster::block_on(open.chat.author(
                commons::chat::ChatEvent::Message(commons::chat::Message {
                    channel: channel.clone(),
                    body: body.clone(),
                    sent_at_ms: settings.authority_clock.now_ms(),
                    reply_to: None,
                }),
            ))
            .map_err(|error| format!("author message: {error}"))?;
            if let Some(lanes) = &open.lanes {
                lanes.publish_chat(operation)?;
            }
        },
        PlaceCommand::ShareNode { address } => {
            if address.trim().is_empty() {
                return Err("a shared node needs an address".to_string());
            }
            // Address as identity, so sharing the same page twice converges on
            // one node instead of accumulating duplicates of the same thing.
            //
            // A Knot document held in this profile's vault is the one address
            // that cannot be shared as it stands: `file:///...` and
            // `knot://vault/...` both name a document only this mere can
            // reach. Rewritten here rather than at the app because this is
            // where the sharer's Personae root and its vault root are both
            // known, and because a rewrite that depended on app state would
            // be one more thing that could be stale when the node is authored.
            let address = settings
                .knot_root
                .as_deref()
                .filter(|_| crate::knot_authoring::is_knot_address(address))
                .and_then(|root| crate::knot_authoring::place_held_rewrite(address, root, &subject))
                .unwrap_or_else(|| address.clone());
            let operation = pollster::block_on(open.graph.edit(move |log| {
                log.insert_node(
                    &chartulary::Author::new("turnstone"),
                    chartulary::Container::new(address.clone()).with_address(address),
                );
            }))
            .map_err(|error| format!("author shared node: {error}"))?;
            if let Some(lanes) = &open.lanes {
                lanes.publish_graph(operation)?;
            }
        },
        PlaceCommand::RevokeMember { .. } => unreachable!("returned above"),
    }
    Ok(())
}
