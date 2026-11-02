// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A registry row for a shared verb, labelled by the catalogue.
pub(super) fn shared(action: Action) -> (&'static str, Action) {
    let label = shared_label(&action).expect("a shared verb has a catalogue id and label");
    (label, action)
}
