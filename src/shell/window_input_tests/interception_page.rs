// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn interception_page(f: &mut CallerCaptureFixture, index: usize) -> Calls {
    let lifetime: Calls = Default::default();
    f.contents.insert(
        f.nodes[index],
        PeltContent::from_surface(
            Box::new(InterceptionPage {
                page: Page {
                    calls: Default::default(),
                    pointers: f.pointers[index].clone(),
                    acquisitions: Default::default(),
                    title: None,
                    fail_offset: None,
                },
                lifetime: lifetime.clone(),
            }),
            (100, 100),
            route(),
        ),
    );
    f.sync(true);
    lifetime
}
