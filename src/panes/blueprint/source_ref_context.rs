// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SourceRef {
    pub fn context(&self) -> PaneContext {
        match self {
            Self::Graph(graph) => PaneContext::graph(*graph),
            Self::Forme { graph, forme } => PaneContext {
                graph: Some(*graph),
                forme: Some(*forme),
                ..PaneContext::default()
            },
            Self::Member { graph, member } => PaneContext {
                graph: Some(*graph),
                member: Some(*member),
                ..PaneContext::default()
            },
            Self::Session(session) => PaneContext {
                session: Some(*session),
                ..PaneContext::default()
            },
            Self::Settings(_) | Self::SessionSet | Self::Application | Self::External { .. } => {
                PaneContext::default()
            },
        }
    }
}
