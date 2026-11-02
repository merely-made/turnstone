// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::retained_clip::owner_worker::Pending;

pub(super) struct RetainStatus {
    pub(super) request: uuid::Uuid,
    pub(super) choice: crate::app::research_note_arms::NoteChoice,
    pub(super) result: Option<Result<RetainSuccess, String>>,
}
impl RetainStatus {
    pub(super) fn record(
        &mut self,
        request: uuid::Uuid,
        result: Result<RetainSuccess, String>,
    ) -> bool {
        if self.request != request || self.result.is_some() {
            return false;
        }
        self.result = Some(result);
        true
    }
    fn matches_scope(
        &self,
        session: crate::panes::SessionId,
        graph: crate::panes::GraphId,
        generation: uuid::Uuid,
        selected: Option<eidetic::ManifestId>,
    ) -> bool {
        let Some(Ok(success)) = &self.result else {
            return false;
        };
        self.choice.session == session
            && self.choice.graph == graph
            && self.choice.generation == generation
            && selected == Some(success.manifest)
    }
}
pub(super) struct RetainSuccess {
    manifest: eidetic::ManifestId,
    root: std::path::PathBuf,
    identity: Arc<crate::identity::RootIdentity>,
    bound: Arc<crate::custody_binding::BoundSealer>,
    epoch: u64,
}
pub(super) struct PendingClipRetain {
    pub(super) choice: crate::app::research_note_arms::NoteChoice,
    pub(super) selection: crate::producer_retention::ProducerSelection,
    pub(super) root: std::path::PathBuf,
    pub(super) identity: Arc<crate::identity::RootIdentity>,
    pub(super) bound: Arc<crate::custody_binding::BoundSealer>,
    pub(super) epoch: u64,
    pub(super) response: Pending<Result<eidetic::ManifestId, String>>,
}
impl Shell {
    pub(super) fn latest_retention_error(&self) -> Option<&str> {
        self.latest_clip_retention
            .as_ref()?
            .result
            .as_ref()?
            .as_ref()
            .err()
            .map(String::as_str)
    }
    pub(super) fn latest_retention_ready(&self) -> bool {
        let Some(status) = &self.latest_clip_retention else {
            return false;
        };
        let Some(Ok(success)) = &status.result else {
            return false;
        };
        status.matches_scope(
            self.app.session_id,
            self.app.graph_runtimes.active_graph(),
            self.app.source_capture_generation,
            selected_graph_clip(&self.app),
        ) && self.app.clip_destination().ok().as_ref() == Some(&success.root)
            && self
                .app
                .identity
                .as_ref()
                .is_some_and(|identity| Arc::ptr_eq(identity, &success.identity))
            && self
                .custody_binding
                .as_ref()
                .is_some_and(|binding| Arc::ptr_eq(&binding.bound, &success.bound))
            && success.bound.epoch() == Some(success.epoch)
    }
    pub(super) fn drain_clip_retains(&mut self) {
        let ready: Vec<_> = self
            .pending_clip_retains
            .iter()
            .filter_map(|(id, pending)| match pending.response.try_recv() {
                Ok(response) => Some((*id, response.into_inner())),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some((*id, Err("Retained clip owner stopped".into())))
                },
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            })
            .collect();
        for (id, result) in ready {
            let Some(pending) = self.pending_clip_retains.remove(&id) else {
                continue;
            };
            let current = self.app.note_choice(pending.choice.member).as_ref()
                == Some(&pending.choice)
                && self.app.browser_command_target().map(|(member, _)| member)
                    == Some(pending.choice.member)
                && self.app.clip_destination().ok().as_ref() == Some(&pending.root)
                && self
                    .app
                    .identity
                    .as_ref()
                    .is_some_and(|identity| Arc::ptr_eq(identity, &pending.identity))
                && self
                    .custody_binding
                    .as_ref()
                    .is_some_and(|binding| Arc::ptr_eq(&binding.bound, &pending.bound))
                && pending.bound.epoch() == Some(pending.epoch)
                && self
                    .app
                    .producer_retention_action()
                    .is_some_and(|(_, action)| {
                        matches!(action,
                    crate::action::Action::RetainProducerSelection { choice, selection }
                    if choice == pending.choice && selection == pending.selection)
                    });
            let latest = self
                .latest_clip_retention
                .as_ref()
                .is_some_and(|status| status.request == id);
            if !latest {
                continue;
            }
            if !current {
                if let Some(status) = self.latest_clip_retention.as_mut() {
                    status.record(
                        id,
                        Err("Retention completed after its source or destination changed".into()),
                    );
                }
                continue;
            }
            match result {
                Ok(manifest) => {
                    let success = RetainSuccess {
                        manifest,
                        root: pending.root,
                        identity: pending.identity,
                        bound: pending.bound,
                        epoch: pending.epoch,
                    };
                    self.act(crate::action::Action::OpenAddress(
                        crate::retained_clip::address(manifest),
                    ));
                    self.run_effects(vec![crate::action::Effect::SaveSession]);
                    if let Some(status) = self
                        .latest_clip_retention
                        .as_mut()
                        .filter(|status| status.request == id)
                    {
                        status.record(id, Ok(success));
                    }
                },
                Err(error) => {
                    if let Some(status) = self
                        .latest_clip_retention
                        .as_mut()
                        .filter(|status| status.request == id)
                    {
                        status.record(id, Err(error.clone()));
                    }
                    self.app.note_retained_clip_refusal(error);
                },
            }
        }
    }
}

// Retention opens its result in the graph. A previously focused document can
// remain the browser command target until the user opens the new graph member
// in the Workbench, so this barrier checks that graph selection instead.
fn selected_graph_clip(app: &crate::app::App) -> Option<eidetic::ManifestId> {
    let pane = app
        .focused_graph_pane()
        .unwrap_or_else(|| app.default_graph_pane());
    let graph = app.graph_for_pane(pane)?;
    if graph != app.graph_runtimes.active_graph() {
        return None;
    }
    let member = app.graph_pane_focused_member(pane)?;
    let (_, node) = app
        .graph_runtimes
        .canvas(graph)?
        .graph()
        .get_node_by_id(member)?;
    match crate::retained_clip::request(node.url()).ok()? {
        Some(crate::retained_clip::ClipRequest::Read(manifest)) => Some(manifest),
        _ => None,
    }
}

pub(super) struct PendingClipImport {
    root: std::path::PathBuf,
    session: crate::panes::SessionId,
    graph: crate::panes::GraphId,
    generation: uuid::Uuid,
    identity: Arc<crate::identity::RootIdentity>,
    bound: Arc<crate::custody_binding::BoundSealer>,
    epoch: u64,
    response: Pending<Result<eidetic::ManifestId, String>>,
}
impl Shell {
    pub(super) fn request_clip_import(&mut self, path: std::path::PathBuf) {
        let result = (|| {
            let root = self.app.clip_destination()?;
            let identity = self.app.identity.clone().ok_or(crate::identity::PENDING)?;
            let binding = self
                .custody_binding
                .as_ref()
                .ok_or(crate::identity::PENDING)?;
            if !Arc::ptr_eq(&identity, &binding.root) {
                return Err(crate::identity::PENDING.into());
            }
            let bound = binding.bound.clone();
            let claim = bound.claim().ok_or(crate::identity::PENDING)?;
            let epoch = claim.epoch();
            let limits = self.app.research.storage_limits()?;
            let access = crate::research::ClipAccess::from_bound(root.clone(), bound.clone());
            let response = self.clip_owner.import(
                root.clone(),
                path,
                crate::retained_clip::limits()?,
                crate::denizen::now_ms(),
                access,
                epoch,
                limits,
            )?;
            Ok(PendingClipImport {
                root,
                session: self.app.session_id,
                graph: self.app.graph_runtimes.active_graph(),
                generation: self.app.source_capture_generation,
                identity,
                bound,
                epoch,
                response,
            })
        })();
        match result {
            Ok(pending) => {
                self.pending_clip_imports
                    .insert(uuid::Uuid::new_v4(), pending);
            },
            Err(error) => self.app.note_retained_clip_refusal(error),
        }
    }
    pub(super) fn drain_clip_imports(&mut self) {
        let ready: Vec<_> = self
            .pending_clip_imports
            .iter()
            .filter_map(|(id, pending)| match pending.response.try_recv() {
                Ok(response) => Some((*id, response.into_inner())),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some((*id, Err("Retained clip owner stopped".into())))
                },
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            })
            .collect();
        for (id, result) in ready {
            let Some(pending) = self.pending_clip_imports.remove(&id) else {
                continue;
            };
            let current = self.app.session_id == pending.session
                && self.app.graph_runtimes.active_graph() == pending.graph
                && self.app.source_capture_generation == pending.generation
                && self.app.clip_destination().ok().as_ref() == Some(&pending.root)
                && self
                    .app
                    .identity
                    .as_ref()
                    .is_some_and(|identity| Arc::ptr_eq(identity, &pending.identity))
                && self
                    .custody_binding
                    .as_ref()
                    .is_some_and(|binding| Arc::ptr_eq(&binding.bound, &pending.bound))
                && pending.bound.epoch() == Some(pending.epoch);
            if !current {
                continue;
            }
            match result {
                Ok(id) => {
                    self.act(crate::action::Action::OpenAddress(
                        crate::retained_clip::address(id),
                    ));
                    self.run_effects(vec![crate::action::Effect::SaveSession]);
                },
                Err(error) => self.app.note_retained_clip_refusal(error),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retention_wait_tracks_new_graph_clip_while_old_document_keeps_focus() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::app::App::test_stub_at(root.path().to_owned());
        app.update(crate::action::Action::OpenAddress(
            "mere://knot/previous".into(),
        ));
        let old = app.graph_runtimes.focused_member().unwrap();
        let choice = crate::app::research_note_arms::NoteChoice {
            generation: app.source_capture_generation,
            context_admission: uuid::Uuid::new_v4(),
            session: app.session_id,
            graph: app.graph_runtimes.active_graph(),
            member: old,
            address: "mere://knot/previous".into(),
        };
        app.focus = crate::surface::FocusTarget::Content {
            node: old,
            appearance: crate::surface::SurfaceId(1),
        };
        let manifest = eidetic::ManifestId::of_blob(b"new retained clip");
        app.update(crate::action::Action::OpenAddress(
            crate::retained_clip::address(manifest),
        ));
        let identity = app.identity.clone().unwrap();
        let bound = crate::custody_binding::BoundSealer::bind(
            identity.clone(),
            Arc::new(std::sync::atomic::AtomicU64::new(1)),
        )
        .unwrap();
        let status = RetainStatus {
            request: uuid::Uuid::new_v4(),
            choice,
            result: Some(Ok(RetainSuccess {
                manifest,
                root: root.path().to_owned(),
                identity,
                bound,
                epoch: 1,
            })),
        };
        assert!(
            app.clip_export_target().is_err(),
            "old document still owns browser commands"
        );
        assert!(status.matches_scope(
            app.session_id,
            app.graph_runtimes.active_graph(),
            app.source_capture_generation,
            selected_graph_clip(&app)
        ));
        assert!(!status.matches_scope(
            app.session_id,
            crate::panes::GraphId::new(),
            app.source_capture_generation,
            selected_graph_clip(&app)
        ));
        app.update(crate::action::Action::OpenAddress(
            crate::retained_clip::address(eidetic::ManifestId::of_blob(b"another clip")),
        ));
        assert!(!status.matches_scope(
            app.session_id,
            app.graph_runtimes.active_graph(),
            app.source_capture_generation,
            selected_graph_clip(&app)
        ));
        app.update(crate::action::Action::OpenAddress(
            "mere://knot/other".into(),
        ));
        assert_eq!(selected_graph_clip(&app), None);
    }
    #[test]
    fn retention_wait_requires_this_request_success_and_matching_current_target() {
        let root = tempfile::tempdir().unwrap();
        let app = crate::app::App::test_stub_at(root.path().to_owned());
        let identity = app.identity.clone().unwrap();
        let bound = crate::custody_binding::BoundSealer::bind(
            identity.clone(),
            Arc::new(std::sync::atomic::AtomicU64::new(1)),
        )
        .unwrap();
        let choice = crate::app::research_note_arms::NoteChoice {
            generation: app.source_capture_generation,
            context_admission: uuid::Uuid::new_v4(),
            session: app.session_id,
            graph: app.graph_runtimes.active_graph(),
            member: uuid::Uuid::new_v4(),
            address: "mere://knot/test".into(),
        };
        let request = uuid::Uuid::new_v4();
        let manifest = eidetic::ManifestId::of_blob(b"retained result");
        let mut status = RetainStatus {
            request,
            choice: choice.clone(),
            result: None,
        };
        let matches = |status: &RetainStatus, selected| {
            status.matches_scope(choice.session, choice.graph, choice.generation, selected)
        };
        assert!(!matches(&status, Some(manifest)));
        assert!(!status.record(uuid::Uuid::new_v4(), Err("old failure".into())));
        assert!(status.result.is_none());
        let success = RetainSuccess {
            manifest,
            root: root.path().to_owned(),
            identity,
            bound,
            epoch: 1,
        };
        assert!(status.record(request, Ok(success)));
        assert!(matches(&status, Some(manifest)));
        assert!(!matches(
            &status,
            Some(eidetic::ManifestId::of_blob(b"old clip"))
        ));
        assert!(!status.matches_scope(
            crate::panes::SessionId::new(),
            choice.graph,
            choice.generation,
            Some(manifest)
        ));
        assert!(!status.matches_scope(
            choice.session,
            crate::panes::GraphId::new(),
            choice.generation,
            Some(manifest)
        ));
        assert!(!status.matches_scope(
            choice.session,
            choice.graph,
            uuid::Uuid::new_v4(),
            Some(manifest)
        ));
        assert!(!status.record(request, Err("later failure".into())));
        let mut failed = RetainStatus {
            request,
            choice,
            result: None,
        };
        assert!(failed.record(request, Err("refused".into())));
        assert!(!failed.matches_scope(
            app.session_id,
            app.graph_runtimes.active_graph(),
            app.source_capture_generation,
            Some(manifest)
        ));
    }
}
