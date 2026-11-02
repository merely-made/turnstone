// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
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
            if attempt.request != request
                || !matches!(
                    attempt.phase,
                    CapturePhase::Saved | CapturePhase::Recoverable
                )
            {
                return vec![];
            }
            // Only this opening's actor inventory can retire the transient
            // live admission and authorize verification of persisted evidence.
            attempt.source_admission = None;
            attempt.phase = CapturePhase::Recoverable;
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
                source_admission: None,
                admitted: true,
                ordinal: self.next_source_capture_ordinal(),
            },
        );
        self.capture_progress(id, "recoverable", None);
        vec![Effect::Redraw]
    }

    pub(super) fn source_capture_inventory_admitted(&self, request: &CaptureRequest) -> bool {
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

    pub(super) fn saved_capture_owner(
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
        if request.keep
            && !crate::surface_controls::read(
                runtime.canvas.graph(),
                request.node,
                crate::surface_controls::Control::Keep,
            )
        {
            return Err("saved capture requires its owning member's Keep state".into());
        }
        if stored.is_none()
            || source_capture::attached(runtime.canvas.graph().facets(), request).as_ref() != stored
        {
            return Err("saved capture attachment is incomplete or changed".into());
        }
        Ok(())
    }

    pub(in super::super) fn revalidate_source_capture(
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
                    source_admission: None,
                    admitted: true,
                    ordinal: self.next_source_capture_ordinal(),
                },
            );
            self.capture_progress(id, "revalidated-saved", None);
            vec![Effect::Redraw]
        } else {
            self.recover_source_capture(request, Some(stored))
        }
    }

    pub(super) fn capture_failed(&mut self, id: uuid::Uuid, error: String) {
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

    pub(super) fn capture_progress(&mut self, id: uuid::Uuid, stage: &str, detail: Option<String>) {
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
