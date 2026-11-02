// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// A bounded native Micron request.  Values are deliberately omitted from
/// debug output so tracing an effect cannot disclose a masked form field.
#[derive(Clone, PartialEq, Eq)]
pub struct MicronSubmission {
    pub target: String,
    pub values: std::collections::BTreeMap<String, String>,
}
