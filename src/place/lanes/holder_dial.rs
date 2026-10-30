// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Prepare a dial to a holder: spend the grant's hop and sign the hello.
///
/// Everything authority-bearing happens here, on the caller's thread, where
/// the identity provider is. What crosses to a visiting thread afterwards is
/// a signed hello and a transport handle, never a key.
pub(crate) fn holder_dial(
    lanes: &LiveLanes,
    holder_ticket: &str,
    grant: &[u8],
    identity: &dyn IdentityProvider,
    nonce: [u8; 32],
) -> Result<HolderDial, String> {
    let moot = lanes.moot;
    let grant = crate::place::projection_host::decode_grant(grant)?;
    let parent = &grant.certificate;
    if parent.subject != identity.master_public_key().to_bytes() {
        return Err("projection grant names another subject".to_string());
    }

    // The place-transport key this dial will actually arrive as. A provider
    // over the derived seed, because `open_session` takes the subject from
    // whatever provider signs the hello.
    let derived = identity
        .derive_keypair(&transport_salt(moot))
        .map_err(|error| format!("derive place transport identity: {error}"))?;
    let visitor = identity::InMemoryProvider::from_seed(derived.to_seed());
    let subject = visitor.master_public_key().to_bytes();

    // The leaf copies the grant's window, so a bounded grant bounds the leaf.
    let leaf = SignedDelegationCertificate::issue(
        &ProviderRef(identity),
        DelegationCertificate::new(
            DelegationParent::Certificate(parent.id()),
            parent.subject,
            subject,
            projection_scope(moot),
            parent.not_before_ms,
            parent.not_before_ms,
            parent.expires_at_ms,
            0,
            *blake3::hash(&[&parent.nonce[..], &subject[..]].concat()).as_bytes(),
        ),
    )
    .map_err(|error| format!("issue the place-transport leaf: {error}"))?;

    let runtime = lanes._runtime.as_ref().expect("lane runtime is present");
    let transport = Arc::clone(&lanes._transport);
    let local_peer = transport.local_peer_id();
    let hello = open_session(
        &visitor,
        NetworkId(moot),
        projection_profile(),
        TrafficClass::Interactive,
        nonce,
        &projection_binding(local_peer),
        vec![grant, leaf],
    )
    .map_err(|error| format!("sign the projection hello: {error}"))?;

    Ok(HolderDial {
        transport,
        handle: runtime.handle().clone(),
        hello,
        ticket: holder_ticket.to_string(),
    })
}
