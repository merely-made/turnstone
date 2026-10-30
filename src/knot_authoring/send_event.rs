// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn send_event(subscriber: &Subscriber, event: HubEvent, wake: &Wake) {
    if subscriber.events.send(event).is_ok() {
        wake();
    }
}
