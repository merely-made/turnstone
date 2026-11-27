// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The viewer pane kinds follow the project list (mod projects MP7c; D60, D63): an installed
//! viewer gets a runtime pane definition and its own provider, an uninstalled or no longer
//! viewer loses both, and a retitle renames the definition.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::{Handles, ModViewerProvider, SOURCE_SCHEMA, kind_id, mod_of_kind, source};
use crate::contributed_surface::SurfaceProviderRegistry;
use crate::mod_projects::ModProjectHost;
use crate::panes::{
    FixedSourceKind, PaneDefinition, PaneKindId, PaneSourceShape, define_runtime_pane,
    pane_definition, remove_runtime_pane, runtime_pane_ids,
};

const ACCEPTED: &[FixedSourceKind] = &[FixedSourceKind::External(SOURCE_SCHEMA)];

/// What one sync changed.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Synced {
    pub changed: bool,
    /// Kinds that lost their provider: the shell drops their panes' open sessions.
    pub removed: Vec<PaneKindId>,
    /// Kinds that gained one: a pane of such a kind that was a placeholder draws afresh.
    pub added: Vec<PaneKindId>,
}

/// The runtime definition of viewer `mod_id`: a contributed pane kind titled `title`.
pub(crate) fn definition(mod_id: &str, title: &str) -> PaneDefinition {
    PaneDefinition::runtime_contributed(
        kind_id(mod_id),
        title.to_owned(),
        format!("Open {title} pane"),
        PaneSourceShape::fixed(ACCEPTED),
        "turnstone.mod-viewer.config",
        "turnstone.mod-viewer.view",
        source(mod_id),
    )
}

/// Make the viewer kinds exactly `viewers` (mod id and title, as the project list has them).
pub(crate) fn sync_kinds(
    registry: &mut SurfaceProviderRegistry,
    viewers: &[(String, String)],
    mods: &Arc<ModProjectHost>,
    handles: &Handles,
) -> Synced {
    let want: BTreeMap<&str, &str> = viewers
        .iter()
        .map(|(id, title)| (id.as_str(), title.as_str()))
        .collect();
    let mut synced = Synced::default();
    for id in runtime_pane_ids() {
        if mod_of_kind(&id).is_some_and(|mod_id| !want.contains_key(mod_id)) {
            let kind = PaneKindId::new(id.clone());
            registry.unregister(&kind);
            remove_runtime_pane(&id);
            synced.removed.push(kind);
            synced.changed = true;
        }
    }
    for (mod_id, title) in want {
        let kind = PaneKindId::new(kind_id(mod_id));
        let current = pane_definition(kind.as_str());
        if current.is_some_and(|d| d.display_name == title) && registry.contains_pane_kind(&kind) {
            continue;
        }
        if !registry.unregister(&kind) {
            synced.added.push(kind.clone());
        }
        let provider = ModViewerProvider::new(mods.clone(), handles.clone(), mod_id, title);
        match registry.register_provider(provider) {
            Ok(()) => define_runtime_pane(definition(mod_id, title)),
            Err(error) => {
                tracing::warn!(%error, mod_id, "a viewer pane kind could not be registered");
                remove_runtime_pane(kind.as_str());
            },
        }
        synced.changed = true;
    }
    synced
}
