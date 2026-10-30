// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn reload_subscriber(
    retained: &mut RetainedEndpointSession,
    session: Option<&ProjectionSession>,
    bindings: &mut BTreeMap<String, DocumentBinding>,
    subscribers: &BTreeMap<u64, Subscriber>,
    registration: u64,
    wake: &Wake,
) {
    let Some(subscriber) = subscribers.get(&registration) else {
        return;
    };
    let Some(session) = session else {
        send_event(
            subscriber,
            HubEvent::Rejected("Knot projection is not mounted".into()),
            wake,
        );
        return;
    };
    match resolve_binding(retained, session, &subscriber.address) {
        Ok(binding) => {
            bindings.insert(subscriber.address.clone(), binding.clone());
            send_event(subscriber, HubEvent::Reloaded(binding), wake);
        },
        Err(error) => send_event(subscriber, HubEvent::Revoked(error), wake),
    }
}
