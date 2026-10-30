// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Author retained graph and chat content on the host as the founder.
pub(super) fn author_host_content(host: &Path, founder: &InMemoryProvider, b: &PlaceBindingV1) {
    let moot = b.moot.0;
    let stores = place_store_dir(host);
    // The founder's own writes must be Effective on the joiner, and a root
    // grant alone covers nothing: MootDelegations::covers walks
    // certificates only, so the founder delegates to itself.
    let rules = place_rules(founder.master_public_key().to_bytes(), moot);
    let moot_file = pollster::block_on(gemot::moot::MootFile::open_existing(
        stores.join("gemot"),
        gemot::moot::MootId(moot),
        settings().retention,
    ))
    .unwrap();
    pollster::block_on(moot_file.delegation_store().author_issue(
        &founder_signing_key(founder, moot),
        &rules,
        place_delegation(founder, moot, founder.master_public_key().to_bytes()),
    ))
    .unwrap();
    drop(moot_file);

    let group = load_group_session(host, founder, moot).unwrap();
    let keyring = DataKeyring::from_bytes(&group.data_keyring_state().unwrap()).unwrap();
    let keys = GroupKeys::new(keyring);

    let graph_backend = RedbBackend::open(stores.join(GRAPH_STORE)).unwrap();
    let mut graph =
        EncryptedReplica::for_identity(graph_backend, b.root.0, founder, keys.clone()).unwrap();
    for index in 0..2 {
        // Address as identity, matching what ShareNode authors: the
        // fixture must produce what the product path produces, or the
        // projection is proved against something nobody writes.
        pollster::block_on(graph.edit(move |log| {
            let address = format!("shared-{index}");
            log.insert_node(
                &Author::new("turnstone"),
                Container::new(address.clone()).with_address(address),
            );
        }))
        .unwrap();
    }
    drop(graph);

    let chat_backend = RedbBackend::open(stores.join("commons-chat.redb")).unwrap();
    let mut chat = ChatReplica::for_identity(chat_backend, b.chat.0, founder, keys).unwrap();
    pollster::block_on(chat.author(ChatEvent::Channel(Channel {
        id: "hall".into(),
        title: "Hall".into(),
    })))
    .unwrap();
    for index in 0..2 {
        pollster::block_on(chat.author(ChatEvent::Message(Message {
            channel: "hall".into(),
            body: format!("retained {index}"),
            sent_at_ms: index as u64,
            reply_to: None,
        })))
        .unwrap();
    }
}
