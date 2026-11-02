// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A pane's placeholder display label from its `PaneContent`. Title-cased tag
/// (the tags are single lowercase words); slice D replaces the placeholder with
/// the pane's real content.
pub(super) fn pane_display_label(content: &PaneContent) -> String {
    crate::panes::pane_definition(content.kind_id().as_str())
        .map(|definition| definition.display_name.to_string())
        .unwrap_or_else(|| content.tag().to_string())
}
