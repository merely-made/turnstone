// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn routing_refused(app: &mut App) {
    app.refuse_behavior("automatic participant run refused: loaded session/runtime binding changed or is unavailable".into());
}
