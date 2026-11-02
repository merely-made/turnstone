// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// A malformed serialized place identifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceIdError {
    pub(super) kind: &'static str,
    pub(super) reason: &'static str,
}
