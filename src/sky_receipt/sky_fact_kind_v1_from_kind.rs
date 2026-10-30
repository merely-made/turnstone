// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyFactKindV1 {
    pub(super) fn from_kind(kind: SkyFactKind) -> Self {
        match kind {
            SkyFactKind::Position => Self::Position,
            SkyFactKind::LunarPhase(LunarPhase::NewMoon) => Self::NewMoon,
            SkyFactKind::LunarPhase(LunarPhase::FirstQuarter) => Self::FirstQuarter,
            SkyFactKind::LunarPhase(LunarPhase::FullMoon) => Self::FullMoon,
            SkyFactKind::LunarPhase(LunarPhase::LastQuarter) => Self::LastQuarter,
            SkyFactKind::Rise => Self::Rise,
            SkyFactKind::Set => Self::Set,
            SkyFactKind::UpperTransit => Self::UpperTransit,
            SkyFactKind::LowerTransit => Self::LowerTransit,
        }
    }

    pub(super) fn sort_rank(self) -> u8 {
        match self {
            Self::NewMoon | Self::FirstQuarter | Self::FullMoon | Self::LastQuarter => 0,
            Self::Rise => 1,
            Self::Set => 2,
            Self::UpperTransit => 3,
            Self::LowerTransit => 4,
            Self::Position => 5,
        }
    }
}
