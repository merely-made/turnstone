// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The participant node's address: subject-derived, so the same pack is the same
/// node identity-wise across installs.
pub fn denizen_url(subject: Subject) -> String {
    format!("mere://denizen/{}", &subject.to_hex()[..16])
}
