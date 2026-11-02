// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

// Chromium profile storage expects ordinary Win32 paths. Rust canonicalization
// returns extended paths, which prevented cookie persistence in our native receipt.
pub(in super::super) fn browser_profile_path(
    path: std::path::PathBuf,
) -> Result<std::path::PathBuf, String> {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        let mut parts = path.components();
        match parts.next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::VerbatimDisk(drive) => {
                    let mut normal = std::path::PathBuf::from(format!("{}:\\", drive as char));
                    normal.extend(parts.filter(|part| !matches!(part, Component::RootDir)));
                    return Ok(normal);
                },
                Prefix::VerbatimUNC(server, share) => {
                    let mut normal = std::path::PathBuf::from(r"\\");
                    normal.push(server);
                    normal.push(share);
                    normal.extend(parts.filter(|part| !matches!(part, Component::RootDir)));
                    return Ok(normal);
                },
                Prefix::Verbatim(_) | Prefix::DeviceNS(_) => {
                    return Err("browser profiles require a drive or UNC path".into());
                },
                _ => {},
            },
            _ => {},
        }
    }
    Ok(path)
}
