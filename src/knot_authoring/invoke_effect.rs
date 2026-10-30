// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn invoke_effect(
    retained: &mut RetainedEndpointSession,
    session: Option<&ProjectionSession>,
    bindings: &mut BTreeMap<String, DocumentBinding>,
    subscribers: &BTreeMap<u64, Subscriber>,
    registration: u64,
    kind: KnotEffectKind,
    confirmed: bool,
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
            HubEvent::Rejected("Knot document is no longer effect-capable".into()),
            wake,
        );
        return;
    };
    let action = match kind {
        KnotEffectKind::Resolve => binding.resolve_action.as_ref(),
        KnotEffectKind::Run => binding.run_action.as_ref(),
    };
    let Some(action) = action else {
        send_event(
            subscriber,
            HubEvent::Rejected(format!(
                "{} is disabled for this document",
                match kind {
                    KnotEffectKind::Resolve => "Resolve",
                    KnotEffectKind::Run => "Run",
                }
            )),
            wake,
        );
        return;
    };
    let result = retained.invoke(
        session,
        binding.target,
        action,
        &KnotEffectV1 {
            base_token: binding.editable.base_token,
            confirmed,
        },
    );
    match result {
        Ok(result) => {
            send_event(
                subscriber,
                HubEvent::Intent(ProjectionIntentReceipt {
                    target: binding.target,
                    intent: action.intent.0.clone(),
                    result: result.clone(),
                }),
                wake,
            );
            match result {
                IntentResult::Accepted => match retained.wait_for_change() {
                    Ok(_) => refresh_subscribers(
                        retained,
                        session,
                        bindings,
                        subscribers,
                        wake,
                        visiting,
                    ),
                    Err(error) => send_event(subscriber, HubEvent::Rejected(error), wake),
                },
                IntentResult::Stale { .. } => send_event(subscriber, HubEvent::Stale, wake),
                IntentResult::Rejected { reason } => {
                    send_event(subscriber, HubEvent::Rejected(reason), wake)
                },
            }
        },
        Err(error) => send_event(subscriber, HubEvent::Rejected(error), wake),
    }
}
