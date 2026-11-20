// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::collections::BTreeSet;

/// The application-wide engine setting projected from Pandect.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EngineEnableSet(BTreeSet<String>);

impl EngineEnableSet {
    pub(crate) fn from_disabled(disabled: &[String]) -> Self {
        Self(disabled.iter().cloned().collect())
    }

    pub(crate) fn is_enabled(&self, engine: &str) -> bool {
        // These routes belong to the host, including the sessions that
        // present already-kept sources and clips.
        matches!(
            engine,
            crate::shell::routing::ENGINE_GRAPHSHELL_INTERNAL
                | crate::shell::routing::ENGINE_LINKED_DATA_INGEST
                | inker::routing::ENGINE_EXTERNAL_PROTOCOL
                | crate::saved_sources::ENGINE_ID
                | crate::retained_clip::ENGINE_ID
        ) || !self.0.contains(engine)
    }
}
