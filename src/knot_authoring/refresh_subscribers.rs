// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn refresh_subscribers(
    retained: &mut RetainedEndpointSession,
    session: &ProjectionSession,
    bindings: &mut BTreeMap<String, DocumentBinding>,
    subscribers: &BTreeMap<u64, Subscriber>,
    wake: &Wake,
    visiting: bool,
) {
    let addresses = subscribers
        .values()
        .map(|subscriber| subscriber.address.clone())
        .collect::<BTreeSet<_>>();
    for address in addresses {
        match resolve_binding(retained, session, &address) {
            Ok(binding) => {
                bindings.insert(address.clone(), binding.clone());
                for subscriber in subscribers
                    .values()
                    .filter(|subscriber| subscriber.address == address)
                {
                    send_event(subscriber, HubEvent::Remote(binding.clone()), wake);
                }
            },
            Err(error) => {
                bindings.remove(&address);
                for subscriber in subscribers
                    .values()
                    .filter(|subscriber| subscriber.address == address)
                {
                    send_event(subscriber, lost(visiting, error.clone()), wake);
                }
            },
        }
    }
}
