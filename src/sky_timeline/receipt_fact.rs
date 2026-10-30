// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn receipt_fact(item: &SkyTimelineItem) -> SkyReceiptFact {
    let (kind, calculation_model) = match item {
        SkyTimelineItem::Position(_) => (SkyFactKind::Position, None),
        SkyTimelineItem::LunarPhase(event) => (SkyFactKind::LunarPhase(event.phase()), None),
        SkyTimelineItem::RiseSet(event) => match event.kind() {
            ConventionalRiseSetKind::Rise => (SkyFactKind::Rise, Some(event.circumstance_model())),
            ConventionalRiseSetKind::Set => (SkyFactKind::Set, Some(event.circumstance_model())),
        },
        SkyTimelineItem::MeridianTransit(event) => match event.kind() {
            MeridianTransitKind::Upper => (SkyFactKind::UpperTransit, Some(event.transit_model())),
            MeridianTransitKind::Lower => (SkyFactKind::LowerTransit, Some(event.transit_model())),
        },
    };
    SkyReceiptFact {
        body: item.body(),
        kind,
        span: item.span(),
        calculation_model,
    }
}
