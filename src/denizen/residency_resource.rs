// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// The governed space a participant's capabilities name: its own residency. Stable
/// across a fork (which carries the same subject and world), and unique per
/// participant, so one helper's certificate can never be read as another's.
pub fn residency_resource(subject_hex: &str) -> Vec<u8> {
    format!("denizen:{subject_hex}").into_bytes()
}
