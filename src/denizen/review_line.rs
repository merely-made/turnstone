// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The review line the palette shows on the Confirm row — the ASK, visible
/// before any grant exists.
pub fn review_line(pending: &PendingInstall) -> String {
    let rings = pending
        .rings
        .iter()
        .map(|r| r.name())
        .collect::<Vec<_>>()
        .join(", ");
    // One row, no clipping: the label, the lane, and the RINGS this install
    // would grant. `own world` stands for the `scenario/` scope every
    // resident gets over its own nested graph.
    // The watch is part of the ask: a reviewer is owed *when this runs* beside
    // *what it may touch*, before either is granted.
    let wakes = match &pending.watch_url {
        Some(url) => match url.strip_prefix("every/") {
            // A schedule reads as a period, not as a path: "wakes on: every
            // hour" is the sentence a reviewer is being asked to grant.
            Some(period) => format!(" — wakes on: every {period}"),
            None => format!(" — wakes on: {url}"),
        },
        None => String::new(),
    };
    let deadband = pending.deadband.map_or_else(String::new, |band| {
        format!(
            " — deadband: change >= {}, interval >= {} ms",
            band.minimum_change(),
            band.minimum_interval_ms()
        )
    });
    format!(
        "Install {} ({}) — grants: {}, own world{}{} — Confirm",
        pending.label,
        pending.body.noun(),
        rings,
        wakes,
        deadband
    )
}
