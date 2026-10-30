// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub fn record_current_session(data_root: &Path, id: SessionId) {
    let path = data_root.join(CURRENT_SESSION_FILE);
    if let Err(err) = std::fs::write(&path, id.as_uuid().to_string()) {
        tracing::warn!(%err, "failed to record the current session");
    }
}
