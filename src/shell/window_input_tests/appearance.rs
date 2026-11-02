// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn appearance(n: u64, x: f32) -> Surface {
    Surface {
        id: SurfaceId(n),
        kind: SurfaceKind::Content(uuid::Uuid::from_u128(7)),
        rect: Rect::new(x, 0., 100., 100.),
    }
}
