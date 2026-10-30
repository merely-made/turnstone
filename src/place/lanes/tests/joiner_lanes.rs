// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The joiner's own live bind: admitted by the product path, then bound
/// exactly as the worker binds it, so the test can hold the lanes.
pub(super) fn joiner_lanes(
    founder: &ServedFounder,
    joiner: &InMemoryProvider,
) -> super::super::LiveLanes {
    crate::place::worker::admit_invitation(&founder.guest, &founder.invite, joiner, &settings())
        .expect("the joiner is admitted");
    let (open, _) =
        open_cached_place(&founder.guest, &founder.binding, joiner, &settings()).unwrap();
    super::super::join_live(
        &open,
        &founder.binding,
        joiner,
        std::slice::from_ref(&founder.ticket),
        // A joiner holds no vault of its own in this proof, so it serves
        // nothing and its own status says so.
        None,
        None,
    )
    .expect("the joiner binds its lanes")
}
