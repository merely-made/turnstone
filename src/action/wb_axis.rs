// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// A workbench split axis, in the app's own vocabulary (this module stays
/// free of the tile-contract crate; `app` maps it onto pelt's `SplitAxis` at
/// the platen call). `Row` lays the new cell left/right of the target,
/// `Column` above/below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WbAxis {
    Row,
    Column,
}
