// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::collections::HashMap;

use super::{
    ContextBinding, PaneContext, PaneId, PaneSource, PaneSpec, SourceRef, SourceSelector, SpaceId,
};

/// Runtime index for resolving focus-following pane sources without storing a
/// mutable graph id on every follower.
#[derive(Clone, Debug, Default)]
pub struct ContextIndex {
    application: PaneContext,
    published: HashMap<PaneId, PaneContext>,
    space_of: HashMap<PaneId, SpaceId>,
    focus_history: HashMap<SpaceId, Vec<PaneId>>,
}

impl ContextIndex {
    pub fn set_application(&mut self, context: PaneContext) {
        self.application = context;
    }

    pub fn place(&mut self, pane: PaneId, space: SpaceId) {
        self.space_of.insert(pane, space);
    }

    pub fn publish(&mut self, pane: PaneId, context: PaneContext) {
        self.published.insert(pane, context);
    }

    pub fn focus(&mut self, pane: PaneId) {
        let Some(space) = self.space_of.get(&pane).cloned() else {
            return;
        };
        let history = self.focus_history.entry(space).or_default();
        history.retain(|candidate| *candidate != pane);
        history.push(pane);
    }

    pub fn resolve_source(&self, pane: &PaneSpec) -> Option<SourceRef> {
        match &pane.source {
            PaneSource::Fixed(source) => Some(source.clone()),
            PaneSource::FromContext(selector) => self
                .resolve_context(pane.id, &pane.context, *selector)
                .and_then(|context| context.source(*selector)),
        }
    }

    pub fn resolve_context(
        &self,
        requester: PaneId,
        binding: &ContextBinding,
        selector: SourceSelector,
    ) -> Option<PaneContext> {
        let candidate = match binding {
            ContextBinding::Own => self.published.get(&requester).copied(),
            ContextBinding::Follow(pane) if *pane != requester => self.published.get(pane).copied(),
            ContextBinding::Follow(_) => None,
            ContextBinding::FocusedInOwnSpace => {
                let space = self.space_of.get(&requester)?;
                self.focus_history
                    .get(space)?
                    .iter()
                    .rev()
                    .find_map(|pane| {
                        (*pane != requester)
                            .then(|| self.published.get(pane).copied())
                            .flatten()
                            .filter(|context| context.supplies(selector))
                    })
            }
            ContextBinding::Application => Some(self.application),
        }?;
        candidate.supplies(selector).then_some(candidate)
    }
}
