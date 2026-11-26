// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Mods on live pages (graphshell mod-projects MP5b; D14, D22 to D25): attach to
//! the focused Livery page, lend its session for each turn, and detach when the
//! page navigates or closes.

use super::*;
use crate::action::ModProjectAction as Step;
use crate::mod_projects::{ModProjectHost, PageBinding, livery_dom};

impl Shell {
    /// What identifies the content `node` shows now: its admission and the
    /// retained session's identity.
    fn page_identity(&self, node: uuid::Uuid) -> Option<(Option<uuid::Uuid>, (u64, u64))> {
        let session = self.content_sessions.get(&node)?.session_identity()?;
        Some((
            self.content_sessions.admission(node),
            (session.instance_id, session.generation),
        ))
    }

    /// Detach every page mod whose page was navigated or closed (D25); the row
    /// says why.
    pub(super) fn reconcile_mod_pages(&mut self) {
        let Some(mods) = self.app.mods.clone() else {
            return;
        };
        for (id, binding) in mods.page_bindings() {
            let why = match self.page_identity(binding.node) {
                None => Some("its page was closed or is reloading"),
                Some(now) if now != (binding.admission, binding.session) => {
                    Some("its page was replaced")
                },
                Some(_) => None,
            };
            if let Some(why) = why {
                mods.detach_page(&id, why);
                self.app.record_mod_project_event("detach", id, format!("ok: detached, {why}"));
            }
        }
    }

    /// Run a mod step that needs the live page.
    pub(super) fn run_mod_page_step(&mut self, step: Step) {
        let Some(mods) = self.app.mods.clone() else {
            return;
        };
        self.reconcile_mod_pages();
        let (name, id, result) = match step {
            Step::AttachPage(id) => {
                let result = self.attach_mod_page(&mods, &id);
                ("attach-page", id, result)
            },
            Step::Turn { id, kind, payload } => {
                let result = self.turn_mod_page(&mods, &id, &kind, &payload);
                ("turn", id, result)
            },
            _ => return,
        };
        let effects = self.app.finish_mod_project(name, id, result);
        self.run_effects(effects);
    }

    fn attach_mod_page(&mut self, mods: &ModProjectHost, id: &str) -> Result<String, String> {
        let node = drive::focused_node(&self.app)
            .ok_or_else(|| "no page is focused; focus a live page first".to_string())?;
        let session = self
            .content_sessions
            .session(&node)
            .ok_or_else(|| "the focused view has no live page; toggle live content".to_string())?;
        let dom = livery_dom(session)
            .ok_or_else(|| "the focused page is not a Livery page".to_string())?;
        let (admission, session_id) = self
            .page_identity(node)
            .ok_or_else(|| "the focused page has no retained session".to_string())?;
        let binding = PageBinding { node, admission, session: session_id };
        mods.attach_page(id, binding, dom)?;
        Ok("ok: attached to the focused page".into())
    }

    fn turn_mod_page(
        &mut self,
        mods: &ModProjectHost,
        id: &str,
        kind: &str,
        payload: &str,
    ) -> Result<String, String> {
        let binding = mods
            .page_binding(id)
            .ok_or_else(|| format!("{id} is not attached to a page"))?;
        let session = self
            .content_sessions
            .session_mut(&binding.node)
            .ok_or_else(|| format!("{id}: its page is gone"))?;
        mods.turn_page(id, kind, payload, session)
    }
}
