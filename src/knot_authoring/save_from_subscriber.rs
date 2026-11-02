// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
pub(super) fn save_from_subscriber(
    retained: &mut RetainedEndpointSession,
    session: Option<&ProjectionSession>,
    bindings: &mut BTreeMap<String, DocumentBinding>,
    subscribers: &BTreeMap<u64, Subscriber>,
    registration: u64,
    base_token: Vec<u8>,
    source: String,
    wake: &Wake,
    visiting: bool,
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
    let Some(binding) = bindings.get(&subscriber.address).cloned() else {
        send_event(
            subscriber,
            HubEvent::Rejected("Knot document is no longer writable".into()),
            wake,
        );
        return;
    };
    let result = retained.invoke(
        session,
        binding.target,
        &binding.save_action,
        &SaveTextV1 {
            base_token,
            source: source.clone(),
        },
    );
    match result {
        Ok(IntentResult::Accepted) => {
            let refresh = retained
                .wait_for_change()
                .and_then(|_| resolve_binding(retained, session, &subscriber.address));
            match refresh {
                Ok(binding) => {
                    bindings.insert(subscriber.address.clone(), binding.clone());
                    send_event(subscriber, HubEvent::Saved { source, binding }, wake);
                    refresh_subscribers(retained, session, bindings, subscribers, wake, visiting);
                },
                Err(error) => send_event(subscriber, HubEvent::Rejected(error), wake),
            }
        },
        Ok(IntentResult::Stale { .. }) => send_event(subscriber, HubEvent::Stale, wake),
        Ok(IntentResult::Rejected { reason }) => {
            send_event(subscriber, HubEvent::Rejected(reason), wake)
        },
        Err(error) => send_event(subscriber, HubEvent::Rejected(error), wake),
    }
}
