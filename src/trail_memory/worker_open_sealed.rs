// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;

pub(super) fn run(
    directory: &Path,
    limits: &Result<crate::research_storage::StorageLimits, String>,
) -> (
    Option<FjallStore>,
    Option<crate::research_storage::SourceReadError>,
) {
    let result = limits
        .as_ref()
        .map_err(|e| crate::research_storage::SourceReadError::from(e.clone()))
        .and_then(|limits| {
            FjallStore::open_with_limits(sealed_memory_dir(directory), limits.owner()).map_err(
                |error| crate::research_storage::SourceReadError::open("Sealed session", error),
            )
        });
    match result {
        Ok(store) => (Some(store), None),
        Err(error) => (None, Some(error)),
    }
}
