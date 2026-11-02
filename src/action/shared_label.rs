// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The catalogue's label for an action offered under a shared id.
pub(super) fn shared_label(action: &Action) -> Option<&'static str> {
    shared_command_id(action).and_then(cambium::catalogue::label)
}
