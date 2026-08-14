//! Gate-ring arms: install, uninstall, run a denizen.
//!
//! Host-only by ring, so no grant can ever cover them: a component confirming
//! its own review would be self-escalation.

use uuid::Uuid;

use crate::action::{Action, Effect};
use crate::observe::AppEvent;
use crate::surface::FocusTarget;
use crate::ui::OmnibarState;

use super::App;

impl App {
    /// Run a resident because a watch woke it, rather than because somebody
    /// asked. The same lane either way: a behavior is a denizen whose run was
    /// triggered, and giving it a second path would give it a second set of
    /// rules.
    pub(crate) fn run_denizen_for_cascade(
        &mut self,
        member: Uuid,
        trigger: &crate::behaviors::TriggerContext,
    ) -> Vec<Effect> {
        self.run_denizen_with(member, trigger)
    }

    pub(super) fn run_denizen(&mut self, member: Uuid) -> Vec<Effect> {
        // Invoked by hand: the context is empty rather than absent, so a body
        // asking what woke it always gets an answer.
        self.run_denizen_with(member, &crate::behaviors::TriggerContext::default())
    }

    fn run_denizen_with(
        &mut self,
        member: Uuid,
        trigger: &crate::behaviors::TriggerContext,
    ) -> Vec<Effect> {
        let Some((subject, label)) = self
            .denizens
            .residents
            .get(&member)
            .map(|r| (r.subject, r.label.clone()))
        else {
            return vec![Effect::Redraw];
        };
        let facet = |id: &str| {
            self.graph_runtimes
                .facets()
                .get(&member, &chartulary::FacetId::new(id))
                .and_then(|v| v.as_str().map(str::to_string))
        };
        // Which lane runs this resident is a property of what it IS
        // (a script's source facet, or a component's file pointer),
        // never of what it may DO — that is the grant's business.
        let component_file = facet(crate::denizen::COMPONENT_FACET);
        let source = facet(crate::denizen::SCENARIO_SOURCE_FACET);
        if let Some(file) = component_file {
            // The wasm lane: emissions are ring-gated inside the run,
            // and what comes back is already authorized.
            #[cfg(not(feature = "wasm"))]
            {
                let _ = file;
                tracing::warn!(%label, "component run refused: built without the wasm feature");
                self.events.push(AppEvent::DenizenRefused(
                    "this build carries no component runtime".to_string(),
                ));
                return vec![Effect::Redraw];
            }
            #[cfg(feature = "wasm")]
            {
                let path = crate::denizen::component_path(&self.session_dir(), &file);
                let run = match crate::component::run(
                    &path,
                    &self.denizens.authority,
                    subject,
                    "run",
                    "",
                ) {
                    Ok(run) => run,
                    Err(err) => {
                        tracing::warn!(%err, %label, "component run failed");
                        self.events.push(AppEvent::DenizenRefused(err));
                        return vec![Effect::Redraw];
                    }
                };
                for line in &run.logs {
                    tracing::info!(%label, "{line}");
                }
                for refusal in &run.refusals {
                    tracing::info!(%label, "component emission refused: {refusal}");
                }
                return self.lower_denizen_actions(subject, label, run.actions);
            }
        }
        let Some(source) = source else {
            return vec![Effect::Redraw];
        };
        // Evaluate the body (read-only against app truth; mutation
        // only ever leaves as typed Actions). The runnable lane is the
        // piccolo feature; a runtime-free build refuses honestly.
        #[cfg(not(feature = "piccolo"))]
        let actions: Vec<Action> = {
            let _ = (&source, &subject, trigger);
            tracing::warn!(%label, "denizen run refused: built without the piccolo feature");
            self.events.push(AppEvent::DenizenRefused(
                "this build carries no script runtime".to_string(),
            ));
            return vec![Effect::Redraw];
        };
        #[cfg(feature = "piccolo")]
        let actions = match crate::script::run(
            self,
            &source,
            // B2: what this run may do derives from the denizen's
            // grant (the participant node), never a blanket flag.
            crate::script::capabilities_from_grant(&self.denizens.authority, subject),
            crate::denizen::RUN_BUDGET,
            trigger,
        ) {
            Ok(actions) => actions,
            Err(err) => {
                tracing::warn!(%err, %label, "denizen run failed");
                self.events.push(AppEvent::DenizenRefused(err));
                return vec![Effect::Redraw];
            }
        };
        self.lower_denizen_actions(subject, label, actions)
    }

    pub(super) fn install_denizen(&mut self, path: String) -> Vec<Effect> {
        match crate::denizen::stage_install(std::path::Path::new(&path)) {
            Ok(pending) => {
                self.events
                    .push(AppEvent::DenizenStaged(pending.label.clone()));
                self.pending_install = Some(pending);
                // Surface the review: the palette opens on the actions
                // lane, whose top rows are the Confirm (carrying the
                // ASK) and Cancel.
                self.omnibar = OmnibarState {
                    open: true,
                    text: ">".to_string(),
                    ..OmnibarState::default()
                };
                let target = self.fallback_shell_context();
                self.shell.begin_omnibar(target);
                self.focus = FocusTarget::Chrome;
                self.recompute_omnibar_suggestions();
                vec![Effect::Redraw]
            }
            Err(err) => {
                tracing::warn!(%err, %path, "denizen install refused at staging");
                self.events.push(AppEvent::DenizenRefused(err));
                vec![Effect::Redraw]
            }
        }
    }

    pub(super) fn uninstall_denizen(&mut self, member: Uuid) -> Vec<Effect> {
        // Revocation, the mirror of install: the user's delegations to
        // this denizen are revoked (cascading to anything it delegated
        // onward), and it stops residing. The node and its world are
        // untouched — revoking authority destroys nothing.
        let Some(resident) = self.denizens.residents.remove(&member) else {
            return vec![Effect::Redraw];
        };
        let revoked = self.denizens.authority.revoke_root_grants(resident.subject);
        // A watch outliving its body would wake nothing, forever. Residency,
        // authority, and standing subscriptions end together.
        self.watches.remove_subject(resident.subject);
        session_runtime::remove_denizen_binding(self.graph_runtimes.facets_mut(), member);
        let hex = resident.subject.to_hex();
        // The certificates go with the residency: a later adopt must
        // not resurrect the authority we just revoked.
        let path = crate::denizen::certs_path(&self.session_dir(), &hex);
        if path.is_file()
            && let Err(err) = std::fs::remove_file(&path)
        {
            tracing::warn!(%err, path = ?path, "failed to remove revoked certificates");
        }
        tracing::info!(label = %resident.label, revoked, "denizen uninstalled");
        self.events
            .push(AppEvent::DenizenUninstalled(resident.label.clone()));
        vec![Effect::SaveSession, Effect::Redraw]
    }

    pub(super) fn confirm_install_denizen(&mut self) -> Vec<Effect> {
        let Some(pending) = self.pending_install.take() else {
            return vec![Effect::Redraw];
        };
        let label = pending.label.clone();
        let member = crate::denizen::install(self, pending);
        self.events.push(AppEvent::DenizenInstalled(label));
        let _ = member;
        self.omnibar = OmnibarState::default();
        self.focus = FocusTarget::Graph(self.default_graph_pane());
        vec![Effect::SaveSession, Effect::Redraw]
    }
}
