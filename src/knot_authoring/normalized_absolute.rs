// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn normalized_absolute(path: &std::path::Path) -> Result<PathBuf, String> {
    let absolute = std::path::absolute(path)
        .map_err(|error| format!("could not resolve {}: {error}", path.display()))?;
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {},
            std::path::Component::ParentDir => {
                normalized.pop();
            },
            component => normalized.push(component.as_os_str()),
        }
    }
    Ok(normalized)
}
