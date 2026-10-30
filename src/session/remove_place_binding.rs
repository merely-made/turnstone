// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Remove the local place binding after the worker has released its handles.
/// Retained place stores and shared membership records are left untouched.
pub fn remove_place_binding(session_dir: &Path) -> Result<bool, PlaceSidecarError> {
    // Remove contact metadata first. A failure retains the binding and remains
    // visible; a subsequent binding-removal failure still permits offline use.
    match std::fs::remove_file(session_dir.join(crate::place::rendezvous::RENDEZVOUS_FILE)) {
        Ok(()) => {},
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
        Err(error) => return Err(error.into()),
    }
    match std::fs::remove_file(place_binding_path(session_dir)) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
