// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn node_transform_translation<Id>(snapshot: &LiverySnapshot<Id>, node: Id) -> (f32, f32)
where
    Id: Copy + Eq + Hash,
{
    let (Some(style), Some(fragment)) = (snapshot.styles.get(node), snapshot.fragments.get(node))
    else {
        return (0.0, 0.0);
    };
    // Turnstone's host placements are translate-only. Livery owns the full
    // transform paint matrix; this query only mirrors its translation terms so
    // host hit-testing and overlays land on those painted boxes.
    style
        .transform
        .to_matrix(16.0, (fragment.width, fragment.height))
        .map_or((0.0, 0.0), |matrix| (matrix.e, matrix.f))
}
