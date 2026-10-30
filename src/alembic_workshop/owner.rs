// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn owner(host: &str, workspace: Uuid, session: Uuid) -> Owner {
    Owner {
        host: host.into(),
        workspace: workspace.to_string(),
        session: session.to_string(),
    }
}
