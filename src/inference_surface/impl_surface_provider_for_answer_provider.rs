// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SurfaceProvider for AnswerProvider {
    fn pane_kind(&self) -> &PaneKindId {
        &self.kind
    }
    fn source_schema(&self) -> &SourceSchemaId {
        &self.schema
    }
    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }
    fn stylesheet(&self) -> &str {
        CSS
    }
    fn admit(
        &self,
        source: &PaneSource,
        dom: DomHandle,
    ) -> Result<Box<dyn RetainedSurfaceSession>, SurfaceAdmissionError> {
        let invalid = |message: String| SurfaceAdmissionError::InvalidPayload {
            schema: self.schema.clone(),
            message,
        };
        let PaneSource::Fixed(SourceRef::External { schema, payload }) = source else {
            return Err(invalid("Answers need a browsing-session source.".into()));
        };
        if schema != &self.schema || payload.version != 2 {
            return Err(invalid(
                "Open Ask saved sources to choose an explicit saved-source scope.".into(),
            ));
        }
        let source: AnswerSourceV2 =
            serde_json::from_value(payload.payload.clone()).map_err(|e| invalid(e.to_string()))?;
        let context = self.bridge.context().map_err(invalid)?;
        if *context.session.as_uuid() != source.session {
            return Err(invalid(
                "Answers belong to another browsing session.".into(),
            ));
        }
        let namespace = crate::research::namespace(context.session);
        let connection_key = format!("{namespace}/{}", context.generation);
        let existing = self
            .connections
            .0
            .borrow()
            .get(&connection_key)
            .and_then(Weak::upgrade);
        let service = match existing {
            Some(service) => service,
            None => {
                let service = InferenceService::start(
                    namespace.clone(),
                    self.owner.clone(),
                    self.wake.clone(),
                )
                .map_err(invalid)?;
                self.connections
                    .0
                    .borrow_mut()
                    .insert(connection_key, Arc::downgrade(&service));
                service
            },
        };
        let draft_key = format!("{namespace}/{}/{}", context.generation, source.view);
        let draft = self
            .drafts
            .borrow()
            .get(&draft_key)
            .and_then(std::rc::Weak::upgrade)
            .unwrap_or_else(|| Rc::new(RefCell::new(Draft::default())));
        self.drafts
            .borrow_mut()
            .insert(draft_key, Rc::downgrade(&draft));
        let previous = draft.borrow().clone();
        let state = AnswerState {
            snapshot: service.snapshot(),
            source,
            context,
            bridge: self.bridge.clone(),
            service,
            draft,
            question: previous.question,
            active: previous.active,
            local_status: String::new(),
            show_raw: false,
            width: 0.,
            height: 0.,
        };
        let runner = GenetAppRunner::new(dom, answer_view as fn(&AnswerState) -> AnswerView, state);
        Ok(Box::new(AnswerSession {
            descriptor: self.descriptor.clone(),
            runner,
        }))
    }
}
