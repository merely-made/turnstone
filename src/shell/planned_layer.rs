// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// One compositor source in plan order. A retained scene is rasterized by
/// Turnstone; an imported surface has already supplied a host-device view.
pub(super) enum PlannedLayer {
    Scene(PlannedScene),
    Imported(CompositeLayer),
}
