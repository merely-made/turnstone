// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Pane kinds defined after startup (mod projects MP7c, D60): a table beside `BUILTIN_PANES`
//! of owned definitions, so every consumer of `pane_definition` reads a runtime kind exactly as
//! it reads a built-in.
//!
//! The table is per thread: the shell publishes kinds and reads them on the UI thread, and a
//! test's table is its own.

use std::{cell::RefCell, ops::Deref, sync::Arc};

use super::{PaneDefinition, PaneKindId};

/// A definition read from either table; it derefs to the definition.
#[derive(Clone, Debug)]
pub enum DefinitionRef {
    Builtin(&'static PaneDefinition),
    Runtime(Arc<PaneDefinition>),
}

impl Deref for DefinitionRef {
    type Target = PaneDefinition;
    fn deref(&self) -> &PaneDefinition {
        match self {
            Self::Builtin(definition) => definition,
            Self::Runtime(definition) => definition,
        }
    }
}

/// Where runtime kinds sort in the palette, after the built-ins (the last is 85).
const RUNTIME_ORDER: u16 = 100;

thread_local! {
    static RUNTIME: RefCell<Vec<Arc<PaneDefinition>>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn runtime_pane(id: &str) -> Option<DefinitionRef> {
    RUNTIME.with_borrow(|table| {
        table
            .iter()
            .find(|definition| definition.id == id)
            .cloned()
            .map(DefinitionRef::Runtime)
    })
}

/// Add a runtime kind, or replace the one with the same id.
pub fn define_runtime_pane(definition: PaneDefinition) {
    RUNTIME.with_borrow_mut(|table| {
        let definition = Arc::new(definition);
        match table.iter().position(|d| d.id == definition.id) {
            Some(at) => table[at] = definition,
            None => table.push(definition),
        }
    });
}

/// Remove a runtime kind; whether it was there.
pub fn remove_runtime_pane(id: &str) -> bool {
    RUNTIME.with_borrow_mut(|table| {
        let before = table.len();
        table.retain(|definition| definition.id != id);
        table.len() != before
    })
}

/// The ids of the runtime kinds, in the order they were added.
pub fn runtime_pane_ids() -> Vec<String> {
    RUNTIME.with_borrow(|table| table.iter().map(|d| d.id.to_string()).collect())
}

/// The runtime kinds' palette entries, by label.
pub fn runtime_palette_entries() -> Vec<(String, PaneKindId)> {
    let mut entries: Vec<_> = RUNTIME.with_borrow(|table| {
        table
            .iter()
            .filter_map(|definition| {
                let entry = definition.palette.as_ref()?;
                Some((entry.order, entry.label.to_string(), definition.kind_id()))
            })
            .collect()
    });
    entries.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    entries
        .into_iter()
        .map(|(_, label, kind)| (label, kind))
        .collect()
}

impl PaneDefinition {
    /// A contributed pane kind defined at runtime: opening it opens `source` on the provider
    /// registered under `id`. Many panes may differ by source.
    pub fn runtime_contributed(
        id: String,
        display_name: String,
        palette_label: String,
        source_shape: super::PaneSourceShape,
        config_schema: &'static str,
        view_schema: &'static str,
        source: super::PaneSource,
    ) -> Self {
        use std::borrow::Cow;

        use super::{
            PaneCapabilities, PaneMultiplicity, PanePaletteEntry,
            PaneRenderer, PlacementPolicy, PublishedContext,
            ViewPersistencePolicy,
        };
        const NONE: PublishedContext = PublishedContext {
            graph: false,
            forme: false,
            member: false,
            session: false,
        };
        Self {
            id: Cow::Owned(id),
            display_name: Cow::Owned(display_name),
            source_shape,
            multiplicity: PaneMultiplicity::PerSpaceAndSource,
            capabilities: PaneCapabilities {
                pointer: true,
                keyboard: true,
                composed_sections: false,
                mixed_surfaces: false,
                publishes: NONE,
            },
            default_placement: PlacementPolicy::BesideFocused,
            config_schema,
            view_schema,
            view_persistence: ViewPersistencePolicy::Blueprint,
            renderer: PaneRenderer::Contributed,
            palette: Some(PanePaletteEntry {
                order: RUNTIME_ORDER,
                label: Cow::Owned(palette_label),
            }),
            content_factory: None,
            default_source: Some(source),
        }
    }
}
