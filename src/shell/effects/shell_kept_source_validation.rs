// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::panes::PaneId;
use crate::saved_sources::KeptSource;
use crate::swatch_pane::validation::{EvidenceResponse, EvidenceTarget, Response, Shapes};
use crate::trail_memory::{
    EvidenceRequest, TrailCommand, ValidationCommand as Command, ValidationRequest,
};

impl Shell {
    pub(in crate::shell) fn validate_kept_source(
        &mut self,
        pane: PaneId,
        source: KeptSource,
        shapes: Shapes,
    ) {
        if !self.app.admits_neighborhood_validation(pane, &source) {
            return;
        }
        if matches!(shapes, Shapes::Choose | Shapes::ChoosePack) {
            let pack = shapes == Shapes::ChoosePack;
            let dialog = rfd::FileDialog::new();
            let dialog = if pack {
                dialog
                    .set_title("Choose a SHACL shape pack")
                    .add_filter("Mere packs", &["pack"])
            } else {
                dialog
                    .set_title("Choose SHACL shapes")
                    .add_filter("Turtle shapes", &["ttl"])
            };
            if let Some(path) = dialog.pick_file() {
                self.act(Action::ValidateKeptSource {
                    pane,
                    source,
                    shapes: if pack {
                        Shapes::Pack(path)
                    } else {
                        Shapes::File(path)
                    },
                });
            }
            return;
        }
        let Some(capture) = self.app.admit_structured_validation(&source) else {
            return;
        };
        let Some(stored) = capture.stored.clone() else {
            return;
        };
        let request = uuid::Uuid::new_v4();
        let generation = self.app.source_capture_generation;
        let command = ValidationRequest {
            request,
            generation,
            pane,
            shapes,
            capture: capture.request.clone(),
            stored,
            directory: crate::trail_memory::memory_dir(&self.app.session_dir()),
            source: source.clone(),
        };
        if !self
            .renderers
            .gloss
            .get_mut(&pane)
            .is_some_and(|view| view.begin_validation(&source, request))
        {
            return;
        }
        if !self
            .trail_handle
            .command(TrailCommand::ValidateKeptSource(Box::new(
                Command::Validate(command),
            )))
        {
            self.accept_kept_source_validation(Response {
                generation,
                request,
                pane,
                source,
                result: Err("The kept source store is unavailable.".into()),
            });
        }
        self.request_redraw();
    }

    pub(in crate::shell) fn accept_kept_source_validation(&mut self, response: Response) {
        let admitted = self.app.source_capture_generation == response.generation
            && self
                .app
                .admits_neighborhood_validation(response.pane, &response.source);
        if let Some(view) = self.renderers.gloss.get_mut(&response.pane) {
            view.finish_validation(response, admitted);
            self.request_redraw();
        }
    }

    pub(in crate::shell) fn open_validation_evidence(
        &mut self,
        pane: PaneId,
        target: EvidenceTarget,
    ) {
        if !self
            .app
            .admits_neighborhood_validation(pane, &target.source)
        {
            return;
        }
        let Some(revision) = self
            .renderers
            .gloss
            .get(&pane)
            .and_then(|view| view.validation_evidence(&target))
            .map(|report| report.projection.revision().clone())
        else {
            return;
        };
        let generation = self.app.source_capture_generation;
        let request = EvidenceRequest {
            generation,
            pane,
            target: target.clone(),
            revision,
            directory: crate::trail_memory::memory_dir(&self.app.session_dir()),
        };
        if !self
            .trail_handle
            .command(TrailCommand::ValidateKeptSource(Box::new(
                Command::OpenEvidence(request),
            )))
        {
            self.accept_validation_evidence(EvidenceResponse {
                generation,
                pane,
                target,
                result: Err("The kept source store is unavailable.".into()),
            });
        }
    }

    pub(in crate::shell) fn accept_validation_evidence(&mut self, response: EvidenceResponse) {
        let EvidenceResponse {
            generation,
            pane,
            target,
            result,
        } = response;
        if self.app.source_capture_generation != generation
            || !self
                .app
                .admits_neighborhood_validation(pane, &target.source)
        {
            return;
        }
        let Some((origin, block)) = self
            .renderers
            .gloss
            .get(&pane)
            .and_then(|view| view.validation_evidence(&target))
            .and_then(|report| {
                let block = report
                    .blocks
                    .iter()
                    .find(|block| block.document_order == target.order)?;
                Some((report.origin.clone(), block.clone()))
            })
        else {
            return;
        };
        let result = result.and_then(|()| {
            self.reveal_structured_source(
                &target.source,
                &origin,
                &target.source.capture_hash,
                std::slice::from_ref(&block),
                target.order,
            )
        });
        if let Some(view) = self.renderers.gloss.get_mut(&pane) {
            view.record_evidence_open(target.order, result);
            self.request_redraw();
        }
    }
}
