// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn inference_owner(dir: &Path) -> Result<String, String> {
    let session = dir
        .parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .and_then(|n| uuid::Uuid::parse_str(n).ok())
        .ok_or("The active browsing session has no retained-text owner.")?;
    Ok(format!("turnstone/{session}"))
}
