// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn overlap_surface_pool() -> (
    Pool,
    [Arc<Mutex<Vec<inker::PointerEvent>>>; 2],
    Vec<Surface>,
) {
    let mut pool = Pool::new();
    let events = [
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
    ];
    let plan = vec![
        Surface {
            id: SurfaceId(10),
            kind: SurfaceKind::Content(uuid::Uuid::from_u128(7)),
            rect: Rect::new(0., 0., 100., 100.),
        },
        Surface {
            id: SurfaceId(11),
            kind: SurfaceKind::Content(uuid::Uuid::from_u128(8)),
            rect: Rect::new(20., 0., 100., 100.),
        },
    ];
    for (node, pointers) in [(7, &events[0]), (8, &events[1])] {
        pool.insert(
            key(node),
            PeltContent::from_surface(
                Box::new(Page {
                    calls: Default::default(),
                    pointers: pointers.clone(),
                    acquisitions: Default::default(),
                    title: None,
                    fail_offset: None,
                }),
                (100, 100),
                route(),
            ),
        );
    }
    pool.set_placements([
        (SurfaceId(10), placed(key(7), 0, 0., false)),
        (SurfaceId(11), placed(key(8), 0, 20., false)),
    ]);
    (pool, events, plan)
}
