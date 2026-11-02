// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The place worker writes its local choice before acknowledging it. Atomic
/// replacement keeps a failed write from losing the previously saved scope.
pub(crate) fn save_place_collection(
    directory: &Path,
    selection: Option<&crate::place::PlaceCollectionVersion>,
) -> Result<(), String> {
    use std::io::Write;
    let saved = PlaceCollectionSidecar {
        version: 1,
        selection: selection.cloned(),
    };
    let bytes = serde_json::to_vec_pretty(&saved)
        .map_err(|error| format!("encode collection choice: {error}"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)
        .map_err(|error| format!("stage collection choice: {error}"))?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| format!("write collection choice: {error}"))?;
    temporary
        .persist(directory.join(PLACE_COLLECTION_FILE))
        .map_err(|error| format!("replace collection choice: {}", error.error))?;
    Ok(())
}
