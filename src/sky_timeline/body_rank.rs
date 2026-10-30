// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn body_rank(body: ApparentBody) -> u8 {
    match body {
        ApparentBody::Sun => 0,
        ApparentBody::Moon => 1,
        ApparentBody::Mercury => 2,
        ApparentBody::Venus => 3,
        ApparentBody::Mars => 4,
        ApparentBody::Jupiter => 5,
        ApparentBody::Saturn => 6,
        ApparentBody::Uranus => 7,
        ApparentBody::Neptune => 8,
        ApparentBody::Pluto => 9,
    }
}
