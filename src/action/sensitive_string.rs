// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// A short-lived secret whose debug representation never contains the value.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct SensitiveString(pub(super) String);
