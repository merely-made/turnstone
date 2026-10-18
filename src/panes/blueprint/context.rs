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
    admission: uuid::Uuid,
    application: PaneContext,
    published: HashMap<PaneId, PaneContext>,
    space_of: HashMap<PaneId, SpaceId>,
    focus_history: HashMap<SpaceId, Vec<PaneId>>,
}

impl ContextIndex {
    /// Process-only admission for captured context controls. Returning through
    /// A/B/A never restores an old control; unchanged frame publication does.
    pub fn admission(&self) -> uuid::Uuid {
        self.admission
    }

    pub(crate) fn invalidate(&mut self) {
        self.admission = uuid::Uuid::new_v4();
    }

    pub fn set_application(&mut self, context: PaneContext) {
        if self.application != context {
            self.invalidate();
        }
        self.application = context;
    }

    pub fn place(&mut self, pane: PaneId, space: SpaceId) {
        if self.space_of.get(&pane) != Some(&space) {
            self.invalidate();
        }
        self.space_of.insert(pane, space);
    }

    pub fn publish(&mut self, pane: PaneId, context: PaneContext) {
        if self.published.get(&pane) != Some(&context) {
            self.invalidate();
        }
        self.published.insert(pane, context);
    }

    pub fn focus(&mut self, pane: PaneId) {
        let Some(space) = self.space_of.get(&pane).cloned() else {
            return;
        };
        if self.focus_history.get(&space).and_then(|h| h.last()) != Some(&pane) {
            self.invalidate();
        }
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
            },
            ContextBinding::Application => Some(self.application),
        }?;
        candidate.supplies(selector).then_some(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captured_context_never_revives_after_aba_but_repaint_keeps_it() {
        let mut index = ContextIndex::default();
        let pane = PaneId(7);
        let a = PaneContext {
            member: Some(uuid::Uuid::new_v4()),
            ..Default::default()
        };
        let b = PaneContext {
            member: Some(uuid::Uuid::new_v4()),
            ..Default::default()
        };
        index.publish(pane, a);
        let offered = index.admission();
        index.publish(pane, a);
        assert_eq!(index.admission(), offered);
        index.publish(pane, b);
        index.publish(pane, a);
        assert_ne!(index.admission(), offered);
        let current = index.admission();
        index.invalidate();
        assert_ne!(index.admission(), current);
    }
}
