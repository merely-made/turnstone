// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A composable pane's name for its palette rows ("Gloss", "Overmap"): the
/// pane's own tag, title-cased. Derived rather than tabled, so a pane that
/// gains a composition names itself.
pub(super) fn pane_label(content: &PaneContent) -> String {
    crate::panes::pane_definition(content.kind_id().as_str())
        .map(|definition| definition.display_name.to_string())
        .unwrap_or_else(|| content.tag().to_string())
}
