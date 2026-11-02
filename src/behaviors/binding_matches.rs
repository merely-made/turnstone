// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn binding_matches(app: &App, origin: crate::host_journal::RuntimeOrigin) -> bool {
    app.behavior_execution_origin() == Some(origin)
}
