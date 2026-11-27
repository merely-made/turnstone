// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Lookup over the built-in table and the runtime table (D60).

use std::borrow::Cow;

use super::{
    BUILTIN_PANES, DefinitionRef, PaneContent, PaneDefinition, PaneKindId, PanePaletteEntry,
    runtime,
};

/// The definition of kind `id`: a built-in, else a runtime kind (D60).
pub fn pane_definition(id: &str) -> Option<DefinitionRef> {
    BUILTIN_PANES
        .iter()
        .find(|definition| definition.id == id)
        .map(DefinitionRef::Builtin)
        .or_else(|| runtime::runtime_pane(id))
}

/// The built-in palette entries, in order; their labels are static.
pub fn builtin_palette_entries() -> Vec<(&'static str, PaneKindId)> {
    let mut panes: Vec<_> = BUILTIN_PANES
        .iter()
        .filter_map(|definition| match &definition.palette {
            Some(PanePaletteEntry {
                order,
                label: Cow::Borrowed(label),
            }) => Some((*order, *label, definition.kind_id())),
            _ => None,
        })
        .collect();
    panes.sort_by_key(|(order, _, _)| *order);
    panes
        .into_iter()
        .map(|(_, label, kind)| (label, kind))
        .collect()
}

/// Every palette entry: the built-ins, then the runtime kinds (D60).
pub fn pane_palette_entries() -> Vec<(String, PaneKindId)> {
    let mut entries: Vec<_> = builtin_palette_entries()
        .into_iter()
        .map(|(label, kind)| (label.to_string(), kind))
        .collect();
    entries.extend(runtime::runtime_palette_entries());
    entries
}

/// Transitional projection into the legacy leaf payload. Registration is the
/// authority; A2 removes this adapter when leaves carry `PaneSpec` directly.
pub fn legacy_pane_content(id: &PaneKindId) -> Option<PaneContent> {
    pane_definition(id.as_str())?.legacy_content()
}

