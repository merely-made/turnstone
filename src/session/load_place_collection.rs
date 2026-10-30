// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Missing means the original all-effective default. An unreadable saved
/// choice must not silently broaden the search on restart.
pub(crate) fn load_place_collection(
    directory: &Path,
) -> Result<Option<crate::place::PlaceCollectionVersion>, String> {
    let bytes = match std::fs::read(directory.join(PLACE_COLLECTION_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("read saved collection choice: {error}")),
    };
    let saved: PlaceCollectionSidecar = serde_json::from_slice(&bytes)
        .map_err(|error| format!("decode saved collection choice: {error}"))?;
    if saved.version != 1 {
        return Err(format!(
            "unsupported collection choice version {}",
            saved.version
        ));
    }
    Ok(saved.selection)
}
