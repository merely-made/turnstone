// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Workspace-owned source rules for Graphshell's single Inspector.
//! They select an existing access; they never copy document or graph content.
use mere::kernel::graph::Graph;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum InspectorSource {
    #[default]
    Active,
    Map,
    Read,
    Pinned(Uuid),
    Paused,
}

impl InspectorSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "Following active view",
            Self::Map => "Following Map",
            Self::Read => "Following Read",
            Self::Pinned(_) => "Pinned",
            Self::Paused => "Paused",
        }
    }

    pub fn resolve(
        self,
        graph: &Graph,
        map: Option<Uuid>,
        reading: Option<Uuid>,
        read_active: bool,
    ) -> Option<Uuid> {
        let member = match self {
            Self::Active if read_active => reading,
            Self::Active | Self::Map => map,
            Self::Read => reading,
            Self::Pinned(member) => Some(member),
            Self::Paused => None,
        }?;
        graph.get_node_by_id(member).map(|_| member)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mere::kernel::graph::apply::add_node;

    #[test]
    fn follows_the_requested_projection_and_never_substitutes_for_an_unavailable_source() {
        let mut graph = Graph::new();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        for id in [a, b] {
            add_node(
                &mut graph,
                Some(id),
                "https://same.test/".into(),
                Default::default(),
            );
        }
        assert_eq!(
            InspectorSource::Active.resolve(&graph, Some(a), Some(b), true),
            Some(b)
        );
        assert_eq!(
            InspectorSource::Map.resolve(&graph, Some(a), Some(b), true),
            Some(a)
        );
        assert_eq!(
            InspectorSource::Read.resolve(&graph, Some(a), None, false),
            None
        );
        assert_eq!(
            InspectorSource::Pinned(a).resolve(&graph, Some(b), Some(b), false),
            Some(a)
        );
        assert_eq!(
            InspectorSource::Pinned(Uuid::from_u128(3)).resolve(&graph, Some(a), Some(b), false),
            None
        );
        assert_eq!(
            InspectorSource::Paused.resolve(&graph, Some(a), Some(b), false),
            None
        );
        assert_eq!(graph.node_count(), 2);
    }
}
