// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// A malformed or unsupported public place binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceBindingError {
    UnsupportedVersion(u16),
    EmptyDefaultChannel,
    DefaultChannelTooLong(usize),
    DefaultChannelHasControl,
}
