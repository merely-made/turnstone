// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn pane_selector_family(content: Option<&crate::panes::PaneContent>) -> &'static str {
    use crate::panes::PaneContent;
    match content {
        Some(PaneContent::Roster) => "roster",
        Some(PaneContent::Trail) => "trail",
        Some(PaneContent::Workbench) => "workbench",
        Some(PaneContent::Inspector) => "inspector",
        Some(PaneContent::Gloss(_)) => "gloss",
        Some(PaneContent::Overmap(_)) => "overmap",
        Some(PaneContent::Registered(kind)) => match kind.as_str() {
            crate::panes::kind::TRANSCRIPT => "transcript",
            crate::panes::kind::SETTINGS => "settings",
            crate::panes::kind::ARRANGE => "arrange",
            crate::panes::kind::PUBLISHING => "publishing",
            crate::panes::kind::SHARED_KNOT => "shared-knot",
            crate::panes::kind::DEVICE_RECEIPTS => "device-receipts",
            crate::panes::kind::FROZEN_PROJECTION => "frozen-projection",
            _ => "contributed",
        },
        _ => "pane",
    }
}
