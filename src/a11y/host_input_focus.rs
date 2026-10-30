// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn host_input_focus(app: &App) -> Option<accesskit::NodeId> {
    app.omnibar
        .open
        .then(|| node_id_for_path("turnstone/chrome/omnibar"))
}
