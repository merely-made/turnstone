// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn fixture_surface(id: Uuid) -> bool {
    EXAMPLES.iter().any(|example| example.id == id)
}
