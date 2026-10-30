// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn window_keyboard_case(event: PeltInput) {
    let mut pool = Pool::new();
    let primary = Calls::default();
    let lens = Calls::default();
    pool.insert(key(7), hosted(&primary));
    pool.insert(key(8), hosted(&lens));
    pool.set_placements([
        (SurfaceId(10), placed(key(7), 0, 0., false)),
        (SurfaceId(11), placed(key(8), 1, 0., true)),
    ]);
    primary.lock().unwrap().clear();
    lens.lock().unwrap().clear();
    focused_input(&mut pool, 1, SurfaceId(10), event);
    assert!(
        !primary
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.starts_with("keyboard")),
        "titlebar focus must not mutate other window"
    );
    assert_eq!(
        lens.lock()
            .unwrap()
            .iter()
            .filter(|s| s.starts_with("keyboard"))
            .count(),
        1,
        "originating focused lens receives once"
    );
}
