// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// `dial_holder` without the certificate: the same hello on the same
/// transport, presenting no delegation at all.
pub(super) fn dial_without_a_grant(
    lanes: &super::super::LiveLanes,
    ticket: &str,
    identity: &dyn IdentityProvider,
) -> Result<(), String> {
    let moot = lanes.moot;
    let derived = identity
        .derive_keypair(&super::super::transport_salt(moot))
        .unwrap();
    let visitor = InMemoryProvider::from_seed(derived.to_seed());
    let transport = Arc::clone(&lanes._transport);
    let hello = graphshell_host::admission::open_session(
        &visitor,
        notochord::NetworkId(moot),
        crate::place::projection_host::projection_profile(),
        notochord::TrafficClass::Interactive,
        [31; 32],
        &graphshell_host::network_carrier::projection_binding(transport.local_peer_id()),
        Vec::new(),
    )
    .unwrap();
    let runtime = lanes._runtime.as_ref().expect("lane runtime is present");
    runtime.block_on(async {
        let peer = transport
            .add_peer_ticket(ticket)
            .await
            .map_err(|error| error.to_string())?;
        match graphshell_host::network_carrier::dial_projection_session(
            &*transport,
            peer,
            &hello,
            &notochord::HandshakeLimits::default(),
        )
        .await
        {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(reason)) => Err(format!("{reason:?}")),
            Err(error) => Err(error.to_string()),
        }
    })
}
