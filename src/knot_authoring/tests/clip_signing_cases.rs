// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use gemot_site::address::KnotAddress;
use gemot_site::clip::open_clip;
use gemot_site::editions;
use identity::{IdentityProvider, InMemoryProvider, KeyTier, UnassociatedKeypair};
use zeroize::Zeroizing;

use super::*;
use crate::identity::RootIdentity;

/// An unassociated key held the way custody would hand it out.
struct Custodied(UnassociatedKeypair);

impl ClipKeySource for Custodied {
    fn clip_seed(&self, tier: KeyTier) -> Result<Zeroizing<[u8; 32]>, String> {
        match tier {
            KeyTier::Unassociated => Ok(Zeroizing::new(self.0.to_seed())),
            KeyTier::Associated => Err("no persona here".into()),
        }
    }
}

fn clip(signing: &ClipSigning) -> Result<SignedClip, String> {
    signing.sign(
        "https://example.test/essay",
        Some("Essay"),
        None,
        "A clipped passage.\n",
        21,
    )
}

#[test]
fn the_default_tier_signs_with_an_unassociated_key_linked_to_no_persona() {
    let persona = InMemoryProvider::from_seed([8; 32]);
    let key = UnassociatedKeypair::generate();
    let public = key.public_key().to_bytes();
    let signing = ClipSigning::new(Arc::new(Custodied(key)));
    assert_eq!(signing.tier(), KeyTier::Unassociated);

    let signed = clip(&signing).unwrap();
    assert_eq!(
        (signed.tier, signed.publisher),
        (KeyTier::Unassociated, public)
    );
    let known = [
        persona.master_public_key().to_bytes(),
        persona
            .derived_keypair(b"anon")
            .unwrap()
            .public_key()
            .to_bytes(),
    ];
    assert!(!known.contains(&signed.publisher));

    // It verifies from the address key and opens by its address.
    let chain = editions::verify_edition_chain(
        &signed.publisher,
        std::slice::from_ref(&signed.operation),
        editions::EditionLimits::default(),
    )
    .unwrap();
    assert_eq!(chain.head().body.kind, editions::EditionKind::Clip);
    let address = KnotAddress::parse(&signed.address).unwrap();
    let frame = open_clip(
        &address,
        std::slice::from_ref(&signed.operation),
        &signed.payload,
    )
    .unwrap();
    assert_eq!(frame.payload.source_url, "https://example.test/essay");

    // Only a recorded clip counts as issued.
    assert!(signing.issued().is_empty());
    signing.record(Ok(signed));
    assert_eq!(signing.issued().len(), 1);
}

#[test]
fn the_associated_tier_signs_with_the_persona_clip_key_and_never_by_default() {
    let persona = InMemoryProvider::from_seed([9; 32]);
    let expected = persona
        .derived_keypair(b"turnstone.clip-edition.v1")
        .unwrap()
        .public_key()
        .to_bytes();
    let root = Arc::new(RootIdentity::Local(persona));

    let associated = ClipSigning::with_tier(root.clone(), KeyTier::Associated);
    let signed = clip(&associated).unwrap();
    assert_eq!(
        (signed.tier, signed.publisher),
        (KeyTier::Associated, expected)
    );

    // Unassociated is the default, and a persona-only source does not fake it.
    let by_default = ClipSigning::new(root);
    let error = clip(&by_default).unwrap_err();
    assert!(error.contains("unassociated"), "{error}");
    by_default.record(Err(error));
    assert!(by_default.issued().is_empty());
    assert!(by_default.last_error().is_some());
}
