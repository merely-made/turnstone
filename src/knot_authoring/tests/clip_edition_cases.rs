// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use gemot_site::address::KnotAddress;
use gemot_site::clip::{clip_address, open_clip, republish};
use gemot_site::editions::{self, EditionWriter, SigningKey};

use super::*;

fn public(key: &SigningKey) -> [u8; 32] {
    *key.verifying_key().as_bytes()
}

#[test]
fn a_clip_becomes_an_edition_that_opens_by_address_and_republishes_as_a_fork() {
    let author = SigningKey::from_bytes(&[4; 32]);
    let (operation, payload) = clip_edition(
        "https://example.test/essay",
        Some("Essay"),
        Some("p:nth-of-type(2)"),
        "A clipped passage.\n",
        &mut EditionWriter::new(author.clone()),
        11,
    )
    .unwrap();
    let chain = editions::verify_edition_chain(
        &public(&author),
        std::slice::from_ref(&operation),
        editions::EditionLimits::default(),
    )
    .unwrap();
    let edition = chain.head();
    assert_eq!(edition.body.kind, editions::EditionKind::Clip);
    assert_eq!(edition.body.provenance.source, "turnstone");
    assert_eq!(edition.body.provenance.created_ms, 11);

    let link = clip_address(public(&author), edition.id).to_string();
    let address = KnotAddress::parse(&link).unwrap();
    let frame = open_clip(&address, std::slice::from_ref(&operation), &payload).unwrap();
    assert_eq!(frame.payload.source_url, "https://example.test/essay");

    let me = SigningKey::from_bytes(&[5; 32]);
    let (fork_op, _) = republish(
        &frame,
        &mut EditionWriter::new(me.clone()),
        None,
        "turnstone",
        12,
    )
    .unwrap();
    let fork_chain = editions::verify_edition_chain(
        &public(&me),
        std::slice::from_ref(&fork_op),
        editions::EditionLimits::default(),
    )
    .unwrap();
    assert_eq!(fork_chain.fork_source(), Some(edition.id));
}

#[test]
fn old_place_held_addresses_still_parse_and_pins_are_dropped() {
    let root = [0x5a; 32];
    let plain = place_held_address(&root, "notes/my field note.knot");
    assert_eq!(
        parse_place_held_address(&plain),
        Some((root, "notes/my field note.knot".to_string()))
    );
    let pinned = format!("{plain}@{}", "ab".repeat(32));
    assert_eq!(
        parse_place_held_address(&pinned),
        Some((root, "notes/my field note.knot".to_string()))
    );
    assert_eq!(parse_place_held_address("knot://vault/field-note"), None);
    assert_eq!(
        parse_place_held_address(&format!("knot://{}.moot/p", "5a".repeat(32))),
        None
    );
}
