// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn item_rank(item: &SkyTimelineItem) -> u8 {
    match item {
        SkyTimelineItem::LunarPhase(_) => 0,
        SkyTimelineItem::RiseSet(event) => match event.kind() {
            ConventionalRiseSetKind::Rise => 1,
            ConventionalRiseSetKind::Set => 2,
        },
        SkyTimelineItem::MeridianTransit(event) => match event.kind() {
            MeridianTransitKind::Upper => 3,
            MeridianTransitKind::Lower => 4,
        },
        SkyTimelineItem::Position(_) => 5,
    }
}
