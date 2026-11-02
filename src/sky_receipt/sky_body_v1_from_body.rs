// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyBodyV1 {
    pub(super) fn from_body(body: ApparentBody) -> Self {
        match body {
            ApparentBody::Sun => Self::Sun,
            ApparentBody::Moon => Self::Moon,
            ApparentBody::Mercury => Self::Mercury,
            ApparentBody::Venus => Self::Venus,
            ApparentBody::Mars => Self::Mars,
            ApparentBody::Jupiter => Self::Jupiter,
            ApparentBody::Saturn => Self::Saturn,
            ApparentBody::Uranus => Self::Uranus,
            ApparentBody::Neptune => Self::Neptune,
            ApparentBody::Pluto => Self::Pluto,
        }
    }

    pub(super) fn sort_rank(self) -> u8 {
        match self {
            Self::Sun => 0,
            Self::Moon => 1,
            Self::Mercury => 2,
            Self::Venus => 3,
            Self::Mars => 4,
            Self::Jupiter => 5,
            Self::Saturn => 6,
            Self::Uranus => 7,
            Self::Neptune => 8,
            Self::Pluto => 9,
        }
    }
}
