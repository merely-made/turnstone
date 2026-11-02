// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// The default ring profile a staged pack arrives with. Control rings
/// (navigate / panes / dispatch) are what a helper needs to be useful; the
/// session ring (fork / close / delete / recover) is destructive, so it is
/// never preselected — a pack that wants it must be granted it deliberately.
/// Host-only is not a profile choice at all: no grant can cover it.
pub fn default_rings() -> Vec<crate::ring::Ring> {
    use crate::ring::Ring;
    vec![Ring::Navigate, Ring::Panes, Ring::Dispatch]
}
