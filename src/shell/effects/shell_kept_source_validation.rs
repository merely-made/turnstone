// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::panes::PaneId;
use crate::saved_sources::KeptSource;
use crate::swatch_pane::validation::{Response, Shapes};
use crate::trail_memory::{TrailCommand, ValidationRequest};

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
        if shapes == Shapes::Choose {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Choose SHACL shapes")
                .add_filter("Turtle shapes", &["ttl"])
                .pick_file()
            {
                self.act(Action::ValidateKeptSource {
                    pane,
                    source,
                    shapes: Shapes::File(path),
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
            .command(TrailCommand::ValidateKeptSource(Box::new(command)))
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
        source: KeptSource,
        order: u64,
    ) {
        if !self.app.admits_neighborhood_validation(pane, &source) {
            return;
        }
        let Some((origin, block)) = self
            .renderers
            .gloss
            .get(&pane)
            .and_then(|view| view.validation_evidence(&source, order))
            .and_then(|report| {
                let block = report
                    .blocks
                    .iter()
                    .find(|block| block.document_order == order)?;
                Some((report.origin.clone(), block.clone()))
            })
        else {
            return;
        };
        let result = self.reveal_structured_source(
            &source,
            &origin,
            &source.capture_hash,
            std::slice::from_ref(&block),
            order,
        );
        if let Some(view) = self.renderers.gloss.get_mut(&pane) {
            view.record_evidence_open(order, result);
            self.request_redraw();
        }
    }
}
