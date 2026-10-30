// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn broadcast(subscribers: &BTreeMap<u64, Subscriber>, event: HubEvent, wake: &Wake) {
    for subscriber in subscribers.values() {
        send_event(subscriber, event.clone(), wake);
    }
}
