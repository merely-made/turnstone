// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Found a new place in `directory` and return its durable binding.
///
/// The founding counterpart to [`admit_invitation`]: it creates the Moot,
/// its constitution, its membership fold, this profile's own delegation, the
/// crypto group, and the default channel. Nothing here dials, writes
/// `place.json`, or claims app state; the worker does that once this returns.
///
/// The self-delegation is not ceremony. Without it the founder's own writes
/// project as pending against its own Moot, because authority is evaluated
/// through the converged delegation fold for everyone, the founder included.
pub fn found_place(
    directory: &Path,
    identity: &dyn IdentityProvider,
    name: &str,
    settings: &PlaceWorkerSettings,
) -> Result<PlaceBindingV1, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a place needs a name".to_string());
    }
    let moot_id = random_id()?;
    let binding = PlaceBindingV1::new(
        PlaceId(moot_id),
        crate::place::SharedContainerId(random_id()?),
        crate::place::ChatSpaceId(random_id()?),
        DEFAULT_CHANNEL,
    )
    .map_err(|error| format!("place binding: {error}"))?;

    let now_ms = settings.authority_clock.now_ms();
    let root = identity.master_public_key().to_bytes();
    let founding = identity
        .derive_keypair(&founding_salt(moot_id))
        .map_err(|error| format!("derive place founding key: {error}"))?;
    let founder_id = founding.public_key().to_bytes();
    let rules = place_rules(founder_id, root, moot_id, now_ms, None);

    let stores = place_store_dir(directory);
    std::fs::create_dir_all(&stores).map_err(|error| format!("create place store: {error}"))?;
    let moot = pollster::block_on(MootFile::open(
        stores.join("gemot"),
        MootId(moot_id),
        founder_id,
        settings.retention.clone(),
    ))
    .map_err(|error| format!("open Gemot store: {error}"))?;
    pollster::block_on(moot.found(founding.to_seed(), None, None, rules.clone(), now_ms))
        .map_err(|error| format!("found Gemot constitution: {error}"))?;
    pollster::block_on(moot.membership_store().author_for_identity(
        identity,
        MootMembershipAction::Create {
            initial_members: vec![MootMember {
                member: root,
                access: MootAccessLevel::Manage,
            }],
        },
    ))
    .map_err(|error| format!("create Gemot membership: {error}"))?;
    pollster::block_on(moot.delegation_store().author_issue(
        &founder_signing_key(identity, moot_id)?,
        &rules,
        place_delegation(identity, moot_id, root, now_ms, now_ms, None)?,
    ))
    .map_err(|error| format!("issue founder delegation: {error}"))?;
    drop(moot);

    found_place_group(directory, identity, moot_id)?;
    let group = load_group_session(directory, identity, moot_id)?;
    let keyring = DataKeyring::from_bytes(
        &group
            .data_keyring_state()
            .map_err(|error| format!("read group data epochs: {error}"))?,
    )
    .map_err(|error| format!("decode group data epochs: {error}"))?;
    let chat_backend = RedbBackend::open(stores.join("commons-chat.redb"))
        .map_err(|error| format!("open Commons chat cache: {error}"))?;
    let mut chat = ChatReplica::for_identity(chat_backend, binding.chat.0, identity, keyring)
        .map_err(|error| format!("bind Commons chat writer: {error}"))?;
    // The place's name lives where peers converge on it, as the default
    // channel's title, rather than in a local sidecar only the founder reads.
    pollster::block_on(
        chat.author(commons::chat::ChatEvent::Channel(commons::chat::Channel {
            id: binding.default_channel.clone(),
            title: name.to_string(),
        })),
    )
    .map_err(|error| format!("open the default channel: {error}"))?;
    drop(chat);
    Ok(binding)
}
