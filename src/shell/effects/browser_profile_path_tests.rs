// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::browser_profile_path;
use std::path::PathBuf;

#[test]
fn canonical_drive_and_unc_paths_keep_the_resolved_location() {
    for (input, expected) in [
        (
            r"\\?\C:\profile with spaces\node",
            r"C:\profile with spaces\node",
        ),
        (r"\\?\UNC\server\share\node", r"\\server\share\node"),
        (r"C:\ordinary\node", r"C:\ordinary\node"),
    ] {
        assert_eq!(
            browser_profile_path(PathBuf::from(input)).unwrap(),
            PathBuf::from(expected)
        );
    }
}

#[test]
fn unsupported_device_paths_are_refused() {
    assert!(browser_profile_path(PathBuf::from(r"\\?\Volume{fixture}\node")).is_err());
    assert!(browser_profile_path(PathBuf::from(r"\\.\device\node")).is_err());
}
