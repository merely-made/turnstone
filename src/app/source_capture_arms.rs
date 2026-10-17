// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Explicit capture admission and correlated persistence acknowledgement.
use super::App;
use crate::action::{Effect, StoredSourceDocument};
use crate::observe::AppEvent;
use crate::source_capture::{self, CaptureAttempt, CapturePhase, CaptureRequest, SourceIdentity};

impl App {
    pub(super) fn await_source_capture_revalidation(&mut self, session: crate::panes::SessionId) {
        self.source_capture_generation = uuid::Uuid::new_v4();
        let requests: Vec<_> = self
            .source_captures
            .values()
            .filter(|attempt| {
                attempt.request.session == session
                    && matches!(
                        attempt.phase,
                        CapturePhase::Saved
                            | CapturePhase::Depositing
                            | CapturePhase::Deposited
                            | CapturePhase::PendingSave
                    )
            })
            .map(|attempt| attempt.request.id)
            .collect();
        for id in requests {
            let attempt = self.source_captures.get_mut(&id).unwrap();
            attempt.phase = CapturePhase::Recoverable;
            attempt.recovered = true;
            let error = "fresh session recovery must verify this capture before restoring Saved status or retrying unfinished work".to_string();
            attempt.error = Some(error.clone());
            self.capture_progress(id, "awaiting-revalidation", Some(error));
        }
    }

    pub(super) fn begin_source_capture(&mut self, node: uuid::Uuid) -> Vec<Effect> {
        let Some(graph) = self.graph_runtimes.graph_containing_member(node) else {
            return vec![];
        };
        let runtime = self.graph_runtimes.get(graph).unwrap();
        let Some(session) = runtime
            .session
            .filter(|session| *session == self.session_id)
        else {
            return vec![];
        };
        let url = runtime
            .canvas
            .graph()
            .get_node_by_id(node)
            .unwrap()
            .1
            .url()
            .to_owned();
        let request = CaptureRequest {
            id: uuid::Uuid::new_v4(),
            session,
            graph,
            node,
            source: self
                .content
                .source_capture_candidate(node, &url)
                .map(SourceIdentity::of),
            url,
        };
        self.source_captures.insert(
            request.id,
            CaptureAttempt {
                request: request.clone(),
                stored: None,
                phase: CapturePhase::Depositing,
                error: None,
                recovered: false,
            },
        );
        self.capture_progress(request.id, "depositing", None);
        vec![Effect::CaptureSourceDocument { request }]
    }

    /// The URL is not sufficient to authorize a late response or a retry.
    pub(crate) fn source_capture_owner(&self, request: &CaptureRequest) -> Result<(), String> {
        // Once exact evidence is attached, retry completes that historical
        // save. Current address/source admission applies only before attachment.
        if let Some(stored) = self
            .source_captures
            .get(&request.id)
            .and_then(|attempt| attempt.stored.as_ref())
            && self
                .graph_runtimes
                .get(request.graph)
                .is_some_and(|runtime| {
                    source_capture::attached(runtime.canvas.graph().facets(), request).as_ref()
                        == Some(stored)
                })
        {
            return self.saved_capture_owner(request, Some(stored));
        }
        if self.session_id != request.session {
            return Err("capture belongs to another session".into());
        }
        let runtime = self
            .graph_runtimes
            .get(request.graph)
            .ok_or("capture graph is no longer admitted")?;
        if runtime.session != Some(request.session) {
            return Err("capture graph session changed".into());
        }
        let directory = crate::session::session_dir(&self.data_root, request.session);
        runtime
            .persistence
            .placement_for(&directory)
            .map_err(|error| error.to_string())?;
        if !crate::browse::still_current(&runtime.canvas, request.node, &request.url) {
            return Err("capture member or address changed".into());
        }
        Ok(())
    }

    pub(super) fn retry_source_capture(&mut self, id: uuid::Uuid) -> Vec<Effect> {
        let Some(attempt) = self.source_captures.get(&id) else {
            return vec![];
        };
        if !matches!(
            attempt.phase,
            CapturePhase::Failed | CapturePhase::Recoverable
        ) {
            return vec![];
        }
        let request = attempt.request.clone();
        if let Err(error) = self.source_capture_owner(&request) {
            self.capture_failed(id, error);
            return vec![Effect::Redraw];
        }
        let attempt = self.source_captures.get_mut(&id).unwrap();
        attempt.phase = CapturePhase::Depositing;
        attempt.recovered = true;
        attempt.error = None;
        self.capture_progress(id, "verifying-or-retrying", None);
        vec![Effect::CaptureSourceDocument { request }]
    }

    pub(super) fn source_capture_deposited(
        &mut self,
        request: CaptureRequest,
        result: Result<StoredSourceDocument, String>,
    ) -> Vec<Effect> {
        let Some(attempt) = self.source_captures.get(&request.id) else {
            return vec![];
        };
        if attempt.request != request || attempt.phase != CapturePhase::Depositing {
            return vec![];
        }
        let recovered = attempt.recovered;
        let stored = match result {
            Ok(stored) => stored,
            Err(error) => {
                self.capture_failed(request.id, error);
                return vec![Effect::Redraw];
            },
        };
        let attempt = self.source_captures.get_mut(&request.id).unwrap();
        attempt.stored = Some(stored.clone());
        attempt.phase = CapturePhase::Deposited;
        self.capture_progress(request.id, "deposited", None);
        if let Err(error) = self.source_capture_owner(&request) {
            self.capture_failed(request.id, error);
            return vec![Effect::Redraw];
        }
        // A fresh acquisition at the same address cannot complete an old request.
        // Recovered deposits have no live response and are verified by the actor.
        let current = self
            .content
            .source_capture_candidate(request.node, &request.url)
            .map(SourceIdentity::of);
        if !recovered && request.source != current {
            self.capture_failed(
                request.id,
                "source response changed before capture completion".into(),
            );
            return vec![Effect::Redraw];
        }
        let facets = self
            .graph_runtimes
            .get_mut(request.graph)
            .unwrap()
            .canvas
            .facets_mut();
        if let Err(error) =
            source_capture::attach_reference(facets, request.node, &request, &stored)
        {
            self.capture_failed(request.id, error);
            return vec![Effect::Redraw];
        }
        self.source_captures.get_mut(&request.id).unwrap().phase = CapturePhase::PendingSave;
        self.capture_progress(request.id, "pending-save", None);
        vec![
            Effect::PersistSourceCapture {
                request: request.id,
            },
            Effect::Redraw,
        ]
    }

    /// Capture's bounded required writes: owning canonical facets and graph.
    /// View/preferences/feed sidecars neither acknowledge nor gate this save.
    pub(crate) fn persist_source_capture(&mut self, id: uuid::Uuid) -> Result<(), String> {
        let attempt = self
            .source_captures
            .get(&id)
            .ok_or("unknown capture request")?;
        if attempt.phase != CapturePhase::PendingSave {
            return Err("capture is not pending a save".into());
        }
        let request = &attempt.request;
        self.source_capture_owner(request)?;
        let runtime = self.graph_runtimes.get(request.graph).unwrap();
        if source_capture::attached(runtime.canvas.graph().facets(), request) != attempt.stored {
            return Err("capture attachment no longer matches its deposit".into());
        }
        let directory = crate::session::session_dir(&self.data_root, request.session);
        runtime
            .persistence
            .save_with(
                &directory,
                || pandect::save_node_facets(&directory, runtime.canvas.graph().facets()),
                |placement| {
                    pandect::session_graph_store::save_profiled(
                        &directory.join(pandect::session_graph_store::GRAPH_FILE),
                        runtime.canvas.graph(),
                        placement,
                    )
                },
            )
            .map_err(|error| error.to_string())
    }

    pub(super) fn source_capture_saved(
        &mut self,
        id: uuid::Uuid,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        let Some(attempt) = self.source_captures.get(&id) else {
            return vec![];
        };
        if attempt.phase != CapturePhase::PendingSave {
            return vec![];
        }
        if let Err(error) = result
            .and_then(|()| self.saved_capture_owner(&attempt.request, attempt.stored.as_ref()))
        {
            self.capture_failed(id, error);
            return vec![Effect::Redraw];
        }
        let attempt = self.source_captures.get_mut(&id).unwrap();
        attempt.phase = CapturePhase::Saved;
        let stored = attempt.stored.as_ref().unwrap();
        self.events.push(AppEvent::SourceDocumentCaptured {
            node: attempt.request.node,
            url: attempt.request.url.clone(),
            raw_manifest: stored.raw_manifest.clone(),
            annotation_manifest: stored.annotation_manifest.clone(),
        });
        self.capture_progress(id, "saved", None);
        vec![Effect::Redraw]
    }

    pub(crate) fn recover_source_capture(
        &mut self,
        request: CaptureRequest,
        stored: Option<StoredSourceDocument>,
    ) -> Vec<Effect> {
        if !self.source_capture_inventory_admitted(&request) {
            return vec![];
        }
        let id = request.id;
        if let Some(attempt) = self.source_captures.get_mut(&id) {
            // Recovery inventory can arrive after a new live request has begun.
            // It may revoke a prior Saved qualification, never replace a live
            // operation or reuse its id for different request facts.
            if attempt.request != request || attempt.phase != CapturePhase::Saved {
                return vec![];
            }
            attempt.phase = CapturePhase::Recoverable;
            attempt.recovered = true;
            // Keep the original admitted artifact identities even if the fresh
            // receipt is incomplete or damaged. Explicit retry must ask the
            // actor to verify them; inventory cannot substitute new evidence.
            let error = "fresh session recovery could not confirm this capture's saved state; original evidence requires verification and save acknowledgement".to_string();
            attempt.error = Some(error.clone());
            self.capture_progress(id, "recoverable", Some(error));
            return vec![Effect::Redraw];
        }
        self.source_captures.insert(
            id,
            CaptureAttempt {
                request,
                stored,
                phase: CapturePhase::Recoverable,
                error: Some(
                    "reopened deposit requires local verification and save acknowledgement".into(),
                ),
                recovered: true,
            },
        );
        self.capture_progress(id, "recoverable", None);
        vec![Effect::Redraw]
    }

    fn source_capture_inventory_admitted(&self, request: &CaptureRequest) -> bool {
        request.session == self.session_id
            && self.source_captures.get(&request.id).is_none_or(|attempt| {
                attempt.request == *request
                    && !matches!(
                        attempt.phase,
                        CapturePhase::Depositing
                            | CapturePhase::Deposited
                            | CapturePhase::PendingSave
                    )
            })
    }

    fn saved_capture_owner(
        &self,
        request: &CaptureRequest,
        stored: Option<&StoredSourceDocument>,
    ) -> Result<(), String> {
        if request.session != self.session_id {
            return Err("saved capture belongs to another session".into());
        }
        let runtime = self
            .graph_runtimes
            .get(request.graph)
            .ok_or("saved capture graph is not admitted")?;
        if runtime.session != Some(request.session)
            || runtime
                .canvas
                .graph()
                .get_node_by_id(request.node)
                .is_none()
        {
            return Err("saved capture member is not admitted by its owning graph".into());
        }
        runtime
            .persistence
            .placement_for(&crate::session::session_dir(
                &self.data_root,
                request.session,
            ))
            .map_err(|error| error.to_string())?;
        if stored.is_none()
            || source_capture::attached(runtime.canvas.graph().facets(), request).as_ref() != stored
        {
            return Err("saved capture attachment is incomplete or changed".into());
        }
        Ok(())
    }

    pub(super) fn revalidate_source_capture(
        &mut self,
        request: CaptureRequest,
        stored: StoredSourceDocument,
    ) -> Vec<Effect> {
        if !self.source_capture_inventory_admitted(&request) {
            return vec![];
        }
        // A saved receipt is a claim from the current actor after local integrity
        // checks. Its owning live graph must still carry that exact attachment.
        let valid = self.saved_capture_owner(&request, Some(&stored)).is_ok();
        let id = request.id;
        if valid && request.session == self.session_id {
            self.source_captures.insert(
                id,
                CaptureAttempt {
                    request,
                    stored: Some(stored),
                    phase: CapturePhase::Saved,
                    error: None,
                    recovered: true,
                },
            );
            self.capture_progress(id, "revalidated-saved", None);
            vec![Effect::Redraw]
        } else {
            self.recover_source_capture(request, Some(stored))
        }
    }

    fn capture_failed(&mut self, id: uuid::Uuid, error: String) {
        let attempt = self.source_captures.get_mut(&id).unwrap();
        attempt.phase = CapturePhase::Failed;
        attempt.error = Some(error.clone());
        self.events.push(AppEvent::SourceDocumentCaptureFailed {
            node: attempt.request.node,
            url: attempt.request.url.clone(),
            error: error.clone(),
        });
        self.capture_progress(id, "failed-recoverable", Some(error));
    }

    fn capture_progress(&mut self, id: uuid::Uuid, stage: &str, detail: Option<String>) {
        let attempt = &self.source_captures[&id];
        self.events.push(AppEvent::SourceDocumentCaptureProgress {
            request: id,
            node: attempt.request.node,
            session: attempt.request.session,
            stage: stage.into(),
            detail,
            stored: attempt.stored.clone(),
        });
    }
}
