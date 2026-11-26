// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A mod on a live Livery page (graphshell mod-projects MP5b; D14, D15, D20,
//! D22 to D25): the host keeps the `PageScript` and the page it is bound to;
//! the shell lends the live session for each turn.

use document_host::mod_loader::InstallRecord;
use document_host::{PageScript, PageSurface, Quota, TurnOutcome};
use genet_documents::engines::LiveryDocumentSession;
use genet_scripted_dom::ScriptedDom;
use inker::session_engine::{DocumentSession, HostMutation, HostMutationReport};
use netrender::Scene;

use super::{ModProjectHost, doc, outcome_words};

/// The page a project is attached to: the node, and what identifies the content
/// it showed. Either changing means the page was navigated or closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageBinding {
    pub node: uuid::Uuid,
    /// The node's content admission (a new load is a new admission).
    pub admission: Option<uuid::Uuid>,
    /// The controller's `(instance_id, generation)` for the retained session.
    pub session: (u64, u64),
}

/// A project's page attachment.
pub(super) struct PageAttach {
    pub script: PageScript,
    pub binding: PageBinding,
}

/// The shell's lent session as a [`PageSurface`], counting what the engine
/// restyled for the batches it took.
pub struct LivePage<'a> {
    pub session: &'a mut dyn DocumentSession<Scene>,
    pub restyled: usize,
}

impl PageSurface for LivePage<'_> {
    fn dom(&self) -> &ScriptedDom {
        self.session
            .as_any_ref()
            .downcast_ref::<LiveryDocumentSession>()
            .map_or_else(
                || unreachable!("a page binding is made only on a Livery session"),
                |session| session.document().dom(),
            )
    }

    fn apply(&mut self, mutations: &[HostMutation]) -> Result<HostMutationReport, String> {
        let report = self
            .session
            .apply_host_mutations(mutations)
            .map_err(|e| format!("{e:?}"))?;
        self.restyled += report.restyled_elements;
        Ok(report)
    }
}

/// The page's DOM, when `session` is a Livery session.
#[must_use]
pub fn livery_dom(session: &dyn DocumentSession<Scene>) -> Option<&ScriptedDom> {
    session
        .as_any_ref()
        .downcast_ref::<LiveryDocumentSession>()
        .map(|session| session.document().dom())
}

impl ModProjectHost {
    /// Where a project is attached, when it is attached to a page.
    pub fn page_binding(&self, id: &str) -> Option<PageBinding> {
        super::lock(&self.state)
            .projects
            .get(id)
            .and_then(|p| p.page.as_ref().map(|page| page.binding))
    }

    /// Every page-attached project and its page.
    pub fn page_bindings(&self) -> Vec<(String, PageBinding)> {
        super::lock(&self.state)
            .projects
            .iter()
            .filter_map(|(id, p)| p.page.as_ref().map(|page| (id.clone(), page.binding)))
            .collect()
    }

    /// Attach the checked component to a copy of `page`, bound to `binding` (D14,
    /// D15). Refused, naming why, when the copy does not match the page.
    pub fn attach_page(
        &self,
        id: &str,
        binding: PageBinding,
        page: &ScriptedDom,
    ) -> Result<(), String> {
        self.with(id, |p, state| {
            if p.script.is_some() || p.page.is_some() {
                return Err(format!("{id} is already attached; reload or detach it"));
            }
            self.check_in(id, p, state)?;
            let record: InstallRecord = self
                .runtime
                .record(id)
                .ok_or_else(|| format!("{id} is not admitted"))?;
            let attached = PageScript::attach_pinned(
                &p.component,
                &record,
                page,
                &p.grant(),
                Quota::default(),
                None,
                Vec::new(),
            );
            match attached {
                Ok(script) => {
                    p.keep_logs(script.guest_logs());
                    p.page = Some(PageAttach { script, binding });
                    p.detached = None;
                    p.failure = None;
                    Ok(())
                },
                Err(why) => {
                    let why = format!("attach to page: {why}");
                    p.last_trap = Some(why.clone());
                    p.failure = Some(why.clone());
                    Self::note_in(state, format!("{id}: {why}"));
                    Err(format!("{id}: {why}"))
                },
            }
        })
    }

    /// Deliver one event to the page mod through the lent page. A trap detaches it
    /// (D20) and the page keeps what the last applied turn made it. The line
    /// ends with the engine's restyled count for the turn.
    pub fn turn_page(
        &self,
        id: &str,
        kind: &str,
        payload: &str,
        session: &mut dyn DocumentSession<Scene>,
    ) -> Result<String, String> {
        self.with(id, |p, _| {
            let attach = p
                .page
                .as_mut()
                .ok_or_else(|| format!("{id} is not attached to a page"))?;
            let mut page = LivePage { session, restyled: 0 };
            match attach.script.deliver_event(&mut page, kind, payload) {
                Ok(outcome) => {
                    let restyled = page.restyled;
                    let line = format!(
                        "turn {kind}: {}, page restyled {restyled}",
                        outcome_words(&outcome)
                    );
                    let logs = attach.script.guest_logs().to_vec();
                    p.keep_logs(&logs);
                    p.last_turn = Some(line.clone());
                    if matches!(outcome, TurnOutcome::PageMoved) {
                        p.last_turn = Some(format!("turn {kind}: refused, the page moved"));
                    }
                    Ok(line)
                },
                Err(e) => {
                    let why = format!("turn '{kind}' trapped: {e:#}");
                    p.page = None;
                    p.detached = Some(why.clone());
                    p.last_trap = Some(why.clone());
                    p.last_turn = Some(format!("turn {kind}: trapped, detached"));
                    Err(format!("{id}: {why}"))
                },
            }
        })
    }

    /// Detach a page mod because its page went away or changed (D25); the row
    /// says why. The page keeps the last applied turn.
    pub fn detach_page(&self, id: &str, why: &str) {
        let _ = self.with(id, |p, _| {
            if let Some(attach) = p.page.take() {
                let _ = attach.script.detach();
                p.detached = Some(format!("detached: {why}"));
            }
            Ok(())
        });
    }

    /// The page copy as an outline, for the row (capped).
    pub(super) fn page_outline(attach: &PageAttach) -> Vec<String> {
        let mut lines = doc::outline(attach.script.dom());
        lines.truncate(24);
        lines
    }
}
