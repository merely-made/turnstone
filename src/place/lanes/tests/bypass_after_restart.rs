// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Stop a member's worker the way a restart does, then act as a member
/// that bypasses its own worker: reopen its stores by hand, optionally
/// author a node its worker would refuse, and dial the founder's door with
/// the grant it still holds. Returns the bind, held so the node can cross,
/// and the door's refusal.
pub(super) fn bypass_after_restart(
    founder: &ServedFounder,
    worker: &armillary::ActorHandle<PlaceWorkerCommand>,
    member: &InMemoryProvider,
    clock: &crate::place::worker::PlaceWorkerSettings,
    forge: Option<&str>,
    nonce: [u8; 32],
) -> (
    crate::place::worker::OpenPlace,
    super::super::LiveLanes,
    String,
) {
    release(worker);
    // Release does not wait for the sync actors' store clones the way a
    // process exit would release them, so the reopen waits instead.
    let released = Instant::now();
    let deadline = released + Duration::from_secs(30);
    let (mut open, _) = loop {
        match open_cached_place(&founder.guest, &founder.binding, member, clock) {
            Ok(opened) => {
                eprintln!(
                    "restart: stores reopened {} ms after release",
                    released.elapsed().as_millis()
                );
                break opened;
            },
            Err(error) if error.contains("already open") && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            },
            Err(error) => panic!("reopen the member's stores: {error}"),
        }
    };
    let lanes = super::super::join_live(
        &open,
        &founder.binding,
        member,
        std::slice::from_ref(&founder.ticket),
        None,
        None,
    )
    .expect("the member binds its lanes");
    if let Some(address) = forge {
        let address = address.to_string();
        let operation = pollster::block_on(open.graph.edit(move |log| {
            log.insert_node(
                &Author::new("turnstone"),
                Container::new(address.clone()).with_address(address),
            );
        }))
        .unwrap();
        lanes.publish_graph(operation).unwrap();
    }
    let grant = crate::place::rendezvous::load_projection_grant(&founder.guest)
        .expect("the member still holds its grant");
    let refusal = match super::super::dial_holder(&lanes, &founder.ticket, &grant, member, nonce) {
        Ok(_) => panic!("the door admitted a withdrawn grant"),
        Err(error) => error,
    };
    (open, lanes, refusal)
}
