// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PaneContext {
    pub fn graph(graph: GraphId) -> Self {
        Self {
            graph: Some(graph),
            ..Self::default()
        }
    }

    pub fn source(self, selector: SourceSelector) -> Option<SourceRef> {
        match selector {
            SourceSelector::Graph => self.graph.map(SourceRef::Graph),
            SourceSelector::Forme => Some(SourceRef::Forme {
                graph: self.graph?,
                forme: self.forme?,
            }),
            SourceSelector::Member => Some(SourceRef::Member {
                graph: self.graph?,
                member: self.member?,
            }),
            SourceSelector::Session => self.session.map(SourceRef::Session),
        }
    }

    pub(super) fn supplies(self, selector: SourceSelector) -> bool {
        self.source(selector).is_some()
    }
}
