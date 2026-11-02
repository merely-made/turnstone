// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn update(state: &Mutex<ClientSnapshot>, wake: &Wake, mut next: ClientSnapshot) {
    let mut current = state.lock().unwrap();
    next.revision = current.revision;
    if *current != next {
        next.revision += 1;
        *current = next;
        drop(current);
        wake();
    }
}
