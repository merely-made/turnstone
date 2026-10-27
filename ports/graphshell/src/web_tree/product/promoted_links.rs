// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) enum WriteKind {
    Freeze { owner: Uuid },
    Comment { owner: Uuid, expected: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) struct PendingWrite {
    pub(in super::super) correlation: graphshell::promoted_links::WriteCorrelation,
    pub(in super::super) kind: WriteKind,
}

pub(in super::super) struct Completion {
    pub(in super::super) pending: PendingWrite,
    pub(in super::super) result: Result<(), String>,
}

impl SavedProduct {
    pub(in super::super) fn freeze_link(
        &mut self,
        key: &scenograph::link::LinkKey,
    ) -> Result<(Uuid, graphshell::promoted_links::WriteCorrelation), String> {
        if self.selection_locked() {
            return Err("Wait for the current save".into());
        }
        let scope = self.retained_scope().ok_or("Source unavailable")?;
        let correlation = graphshell::promoted_links::WriteCorrelation {
            scope,
            operation: Uuid::new_v4(),
            attempt: 0,
        };
        let app = self.app.as_mut().ok_or("Source unavailable")?;
        let source = app.host.link_source();
        let owner = graphshell::promoted_links::freeze(app, key, &source, Uuid::new_v4())?;
        self.pending_operation = Some(PendingOperation::Promoted(PendingWrite {
            correlation: correlation.clone(),
            kind: WriteKind::Freeze { owner },
        }));
        self.operation_success = Some("Link frozen for review".into());
        self.persist_operation();
        Ok((owner, correlation))
    }
    pub(in super::super) fn promoted_links(&self) -> Result<Vec<Uuid>, String> {
        self.source_graph().map(graphshell::promoted_links::list)
    }
    pub(in super::super) fn inspect_promoted_link(
        &self,
        owner: Uuid,
    ) -> Result<graphshell::promoted_links::PromotedLinkInspection, String> {
        let app = self.app.as_ref().ok_or("Wait for the current save")?;
        graphshell::promoted_links::inspect(app.host.graph(), owner, &app.host.link_source())
    }
    pub(in super::super) fn comment_promoted_link(
        &mut self,
        owner: Uuid,
        text: &str,
    ) -> Result<graphshell::promoted_links::WriteCorrelation, String> {
        if self.selection_locked() {
            return Err("Wait for the current save".into());
        }
        let scope = self.retained_scope().ok_or("Source unavailable")?;
        let correlation = graphshell::promoted_links::WriteCorrelation {
            scope,
            operation: Uuid::new_v4(),
            attempt: 0,
        };
        let app = self.app.as_mut().ok_or("Source unavailable")?;
        graphshell::promoted_links::comment(
            app,
            owner,
            text,
            Uuid::new_v4(),
            js_sys::Date::now() as u64,
        )?;
        self.pending_operation = Some(PendingOperation::Promoted(PendingWrite {
            correlation: correlation.clone(),
            kind: WriteKind::Comment {
                owner,
                expected: text.to_owned(),
            },
        }));
        self.operation_success = Some("Review comment saved".into());
        self.persist_operation();
        Ok(correlation)
    }

    pub(in super::super) fn take_promoted_completion(&mut self) -> Option<Completion> {
        self.promoted_completed.pop_front()
    }

    pub(in super::super) fn has_promoted_operation(&self, operation: Uuid) -> bool {
        matches!(
            &self.pending_operation,
            Some(PendingOperation::Promoted(pending))
                if pending.correlation.operation == operation
        )
    }

    pub(super) fn complete_promoted(&mut self, pending: PendingWrite, result: &Result<(), String>) {
        if self
            .pending_operation
            .as_ref()
            .and_then(|operation| match operation {
                PendingOperation::Promoted(pending) => Some(pending),
                PendingOperation::Query(_) => None,
            })
            .is_none_or(|current| current.correlation != pending.correlation)
        {
            return;
        }
        if result.is_ok() {
            self.pending_operation = None;
        }
        self.promoted_completed.push_back(Completion {
            pending,
            result: result.clone(),
        });
    }

    pub(in super::super) fn can_withdraw_link_assertion(
        &self,
        target: &cambium::LinkMemberTarget,
    ) -> bool {
        !self.selection_locked()
            && self
                .app
                .as_ref()
                .is_some_and(|app| graphshell::promoted_links::can_withdraw(app, target))
    }
    pub(in super::super) fn withdraw_link_assertion(
        &mut self,
        target: &cambium::LinkMemberTarget,
    ) -> Result<(), String> {
        if self.selection_locked() {
            return Err("Wait for the current save".into());
        }
        let app = self.app.as_mut().ok_or("Source unavailable")?;
        graphshell::promoted_links::withdraw(app, target, js_sys::Date::now() as u64)?;
        self.operation_success = Some("Personal assertion withdrawn".into());
        self.persist_operation();
        Ok(())
    }
}
