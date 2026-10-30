// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn seed_profile(
    directory: &Path,
    identity: &RootIdentity,
    binding: &PlaceBindingV1,
    facts: usize,
) {
    let founder = founder_for(binding);
    let founder_id = founder.master_public_key().to_bytes();
    let settings = settings();
    let stores = place_store_dir(directory);
    std::fs::create_dir_all(&stores).unwrap();
    let rules = place_rules(founder_id, binding.moot.0);
    let moot = pollster::block_on(MootFile::open(
        stores.join("gemot"),
        MootId(binding.moot.0),
        founder_id,
        settings.retention.clone(),
    ))
    .unwrap();
    pollster::block_on(moot.found(
        founder.master_keypair().to_seed(),
        None,
        None,
        rules.clone(),
        1,
    ))
    .unwrap();
    // Without this the profile holds no capability and every fact it
    // authors below projects as pending, which is the correct verdict for
    // an unadmitted writer but not the fixture these tests need.
    pollster::block_on(moot.delegation_store().author_issue(
        &founder_signing_key(&founder, binding.moot.0),
        &rules,
        place_delegation(
            &founder,
            binding.moot.0,
            identity.master_public_key().to_bytes(),
        ),
    ))
    .unwrap();
    drop(moot);

    prepare_group_identity(directory, identity, binding.moot.0).unwrap();
    found_place_group(directory, identity, binding.moot.0).unwrap();
    let group = load_group_session(directory, identity, binding.moot.0).unwrap();
    let keyring = DataKeyring::from_bytes(&group.data_keyring_state().unwrap()).unwrap();
    let keys = GroupKeys::new(keyring);

    let graph_backend = RedbBackend::open(stores.join(GRAPH_STORE)).unwrap();
    let mut graph =
        EncryptedReplica::for_identity(graph_backend, binding.root.0, identity, keys.clone())
            .unwrap();
    for index in 0..facts {
        pollster::block_on(graph.edit(|log| {
            log.insert_node(
                &Author::new("turnstone"),
                Container::new(format!("node-{index}")),
            );
        }))
        .unwrap();
    }
    drop(graph);

    let chat_backend = RedbBackend::open(stores.join("commons-chat.redb")).unwrap();
    let mut chat = ChatReplica::for_identity(chat_backend, binding.chat.0, identity, keys).unwrap();
    pollster::block_on(chat.author(ChatEvent::Channel(Channel {
        id: "hall".into(),
        title: "Hall".into(),
    })))
    .unwrap();
    for index in 0..facts {
        pollster::block_on(chat.author(ChatEvent::Message(Message {
            channel: "hall".into(),
            body: format!("message {index}"),
            sent_at_ms: index as u64,
            reply_to: None,
        })))
        .unwrap();
    }
}
