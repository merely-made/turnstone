// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn touch_pool() -> (Pool, Arc<Mutex<Vec<inker::PointerEvent>>>, Vec<Surface>) {
    let pointers = Arc::new(Mutex::new(Vec::new()));
    let mut pool = Pool::new();
    pool.insert(
        key(7),
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
    pool.set_placements([
        (SurfaceId(10), placed(key(7), 0, 0., false)),
        (SurfaceId(11), placed(key(7), 0, 200., false)),
    ]);
    (
        pool,
        pointers,
        vec![appearance(10, 0.), appearance(11, 200.)],
    )
}
