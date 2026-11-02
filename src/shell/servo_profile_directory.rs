// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A launch-selected name identifies Servo's process profile. An explicit
/// directory overrides its storage location, while keeping that visible name.
#[cfg(any(
    test,
    all(
        feature = "servo",
        any(windows, target_os = "macos", target_os = "linux")
    )
))]
pub(super) fn servo_profile_directory(
    data_root: &std::path::Path,
    name: &str,
    directory: Option<std::path::PathBuf>,
) -> Result<std::path::PathBuf, String> {
    let reserved = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let device_name = matches!(reserved.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (reserved.len() == 4
            && (reserved.starts_with("COM") || reserved.starts_with("LPT"))
            && matches!(reserved.as_bytes()[3], b'1'..=b'9'));
    if name.is_empty()
        || name.trim() != name
        || name.ends_with('.')
        || device_name
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
    {
        return Err("TURNSTONE_SERVO_PROFILE must be a nonempty profile name usable as one directory component".into());
    }
    match directory {
        Some(path) if path.is_absolute() => Ok(path),
        Some(_) => Err("TURNSTONE_SERVO_PROFILE_DIR must be an absolute directory".into()),
        None => Ok(data_root.join("servo").join("profiles").join(name)),
    }
}
