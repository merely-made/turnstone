// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
#[test]
fn content_pool_host_contributed_custody_checks_placement_and_provider() {
    let surface = crate::surface::Surface {
        id: crate::surface::SurfaceId(8),
        kind: crate::surface::SurfaceKind::Pane(crate::panes::PaneId(9)),
        rect: crate::surface::Rect::new(10., 20., 100., 80.),
    };
    let admitted = ContributedPointer {
        surface,
        generation: 4,
        window: 0,
    };
    assert!(admitted.is_current(Some(surface), Some(4), 0));
    assert!(
        !admitted.is_current(Some(surface), Some(4), 1),
        "another native window cannot inherit gesture"
    );
    assert!(
        !admitted.is_current(None, Some(4), 0),
        "hidden/closed placement retires gesture"
    );
    assert!(
        !admitted.is_current(Some(surface), Some(5), 0),
        "replacement provider retires gesture"
    );
    assert!(
        !admitted.is_current(
            Some(crate::surface::Surface {
                rect: crate::surface::Rect::new(11., 20., 100., 80.),
                ..surface
            }),
            Some(4),
            0
        ),
        "moved placement retires gesture"
    );
    assert!(
        !admitted.is_current(
            Some(crate::surface::Surface {
                id: crate::surface::SurfaceId(10),
                ..surface
            }),
            Some(4),
            0
        ),
        "another appearance cannot inherit gesture"
    );
}
