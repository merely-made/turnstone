// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn fact_kind_label(kind: SkyFactKind) -> &'static str {
    match kind {
        SkyFactKind::Position => "position",
        SkyFactKind::LunarPhase(LunarPhase::NewMoon) => "new moon",
        SkyFactKind::LunarPhase(LunarPhase::FirstQuarter) => "first quarter",
        SkyFactKind::LunarPhase(LunarPhase::FullMoon) => "full moon",
        SkyFactKind::LunarPhase(LunarPhase::LastQuarter) => "last quarter",
        SkyFactKind::Rise => "rise",
        SkyFactKind::Set => "set",
        SkyFactKind::UpperTransit => "upper transit",
        SkyFactKind::LowerTransit => "lower transit",
    }
}
