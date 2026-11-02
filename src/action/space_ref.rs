// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Which window's frisket space a tree op targets: the primary tree or a live
/// lens's. Pane ids are unique across every space, so a pane-anchored op
/// resolves its own space ([`crate::app::App::space_of`]); only the
/// path-addressed divider drag names its tree explicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpaceRef {
    Primary,
    /// A lens window's space, by ordinal into `App::lenses`.
    Lens(usize),
}
