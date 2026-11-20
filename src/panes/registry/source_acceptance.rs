// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PaneSourceShape {
    pub fn accepts(self, source: &PaneSource) -> bool {
        match source {
            PaneSource::FromContext(selector) => self.contextual.contains(selector),
            PaneSource::Fixed(source) => self.fixed.iter().any(|kind| match (kind, source) {
                (FixedSourceKind::Graph, SourceRef::Graph(_))
                | (FixedSourceKind::Forme, SourceRef::Forme { .. })
                | (FixedSourceKind::Member, SourceRef::Member { .. })
                | (FixedSourceKind::Settings, SourceRef::Settings(_))
                | (FixedSourceKind::Session, SourceRef::Session(_))
                | (FixedSourceKind::SessionSet, SourceRef::SessionSet)
                | (FixedSourceKind::Application, SourceRef::Application) => true,
                (FixedSourceKind::External(expected), SourceRef::External { schema, .. }) => {
                    schema.as_str() == *expected
                },
                _ => false,
            }),
        }
    }
}
