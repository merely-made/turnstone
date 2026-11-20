// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Native reader for the exact JSON-LD block associated with a finding.

use std::cell::RefCell;
use std::rc::Rc;

use cambium::{DomHandle, GenetAppRunner, RetainedSurfaceSession, RunnerSurfaceSession, el};
use fleece::EmbeddedJsonLdBlock;
use mere_surface_api::{
    ProviderId, SourceKindId, SurfaceAvailability, SurfaceDescriptor, SurfaceId,
    SurfaceSourceShape, SurfaceUnavailableReason,
};
use uuid::Uuid;

use super::Shell;
use crate::contributed_surface::{SurfaceAdmissionError, SurfaceProvider};
use crate::panes::{
    PaneKindId, PaneSource, SerializedSource, SessionId, SourceRef, SourceSchemaId,
};
use crate::saved_sources::KeptSource;

pub(crate) const PANE_KIND: &str = "turnstone.structured-source";
pub(crate) const LABEL: &str = "Source block";
pub(crate) const SOURCE_SCHEMA: &str = "turnstone.structured-source.v1";

const CSS: &str = "
.source-block { box-sizing: border-box; padding: 16px; color: var(--tabard-color-text, #e0e6ee); }
.source-block h1 { font-size: 16px; margin: 0 0 8px; }
.source-block p { margin: 4px 0; overflow-wrap: anywhere; }
.source-block pre {
  box-sizing: border-box; padding: 12px; white-space: pre-wrap;
  overflow-wrap: anywhere; font-family: monospace;
  background: var(--tabard-color-surface-2, #232833);
}
";

#[derive(Clone, Default)]
pub(super) struct Reader {
    current: Rc<RefCell<Option<OpenBlock>>>,
}

impl Reader {
    fn source(session: SessionId) -> PaneSource {
        PaneSource::Fixed(SourceRef::External {
            schema: SourceSchemaId::new(SOURCE_SCHEMA),
            payload: SerializedSource {
                version: 1,
                payload: serde_json::Value::String(session.as_uuid().to_string()),
            },
        })
    }

    fn block_for(&self, source: &PaneSource) -> Option<Rc<Block>> {
        let current = self.current.borrow();
        let open = current.as_ref()?;
        (source == &Self::source(open.admission.key.session)).then(|| Rc::clone(&open.block))
    }
}

struct OpenBlock {
    admission: KeptSource,
    generation: Uuid,
    block: Rc<Block>,
}

#[derive(Debug)]
struct Block {
    source: String,
    capture: String,
    order: u64,
    element_id: Option<String>,
    dom_text: String,
}

impl Block {
    fn at(
        source: &str,
        capture: &str,
        blocks: &[EmbeddedJsonLdBlock],
        order: u64,
    ) -> Result<Self, String> {
        let mut matching = blocks.iter().filter(|block| block.document_order == order);
        let block = matching.next().ok_or_else(|| {
            "The finding's JSON-LD block is missing from this capture".to_string()
        })?;
        if matching.next().is_some() {
            return Err("The finding's JSON-LD block order is ambiguous".into());
        }
        Ok(Self {
            source: source.into(),
            capture: capture.into(),
            order,
            element_id: block.element_id.clone(),
            dom_text: block.dom_text.clone(),
        })
    }
}

pub(super) struct Provider {
    reader: Reader,
    kind: PaneKindId,
    schema: SourceSchemaId,
    descriptor: SurfaceDescriptor,
}

impl Provider {
    pub(super) fn new(reader: Reader) -> Self {
        Self {
            reader,
            kind: PaneKindId::new(PANE_KIND),
            schema: SourceSchemaId::new(SOURCE_SCHEMA),
            descriptor: SurfaceDescriptor {
                provider_id: ProviderId::from("turnstone"),
                surface_id: SurfaceId::from(SOURCE_SCHEMA),
                label: LABEL.into(),
                accepted_source: SurfaceSourceShape::One(SourceKindId::from(SOURCE_SCHEMA)),
            },
        }
    }
}

impl SurfaceProvider for Provider {
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
        let block = self
            .reader
            .block_for(source)
            .ok_or(SurfaceAdmissionError::Unavailable {
                reason: SurfaceUnavailableReason::Other(
                    "Open a finding to see its source block".into(),
                ),
            })?;
        let runner = GenetAppRunner::new(dom, view, block);
        Ok(Box::new(RunnerSurfaceSession::new(
            self.descriptor.clone(),
            runner,
            |_: &Rc<Block>| SurfaceAvailability::Available,
            |_, _| {},
            |_: ()| Vec::new(),
        )))
    }
}

fn view(
    block: &Rc<Block>,
) -> impl cambium::View<Rc<Block>, (), cambium::GenetCtx, Element = cambium::GenetElement> + use<> {
    let heading = format!("JSON-LD block at DOM position {}", block.order);
    let source = format!("Source: {}", block.source);
    let capture = format!("Capture: {}", block.capture);
    let element_id = block
        .element_id
        .as_ref()
        .filter(|id| !id.is_empty())
        .map(|id| el("p", format!("Element ID: {id}")));
    el(
        "article",
        (
            el("h1", heading),
            el("p", source),
            el("p", capture),
            element_id,
            el("pre", block.dom_text.clone()).attr("class", "source-block-text"),
        ),
    )
    .attr("class", "source-block")
}

impl Shell {
    /// The caller supplies blocks resolved against the evidence's exact capture.
    pub(super) fn reveal_structured_source(
        &mut self,
        admission: &KeptSource,
        source: &str,
        capture: &str,
        blocks: &[EmbeddedJsonLdBlock],
        order: u64,
    ) -> Result<(), String> {
        self.reconcile_structured_source();
        if capture != admission.capture_hash
            || self.app.admit_structured_validation(admission).is_none()
        {
            return Err("The finding's source is no longer attached".into());
        }
        let block = Rc::new(Block::at(source, capture, blocks, order)?);
        *self.structured_source.current.borrow_mut() = Some(OpenBlock {
            admission: admission.clone(),
            generation: self.app.source_capture_generation,
            block,
        });

        // A new block starts at the top of the reader even in a reused pane.
        self.evict_structured_source_panes();
        let kind = PaneKindId::new(PANE_KIND);
        self.act(crate::action::Action::SummonContributedPane {
            kind: kind.clone(),
            source: Reader::source(admission.key.session),
        });
        if self
            .app
            .active_pane
            .and_then(|pane| self.pane_content(pane))
            .is_some_and(|content| content.kind_id() == kind)
        {
            Ok(())
        } else {
            self.clear_structured_source();
            Err("Could not open the source block reader".into())
        }
    }

    pub(super) fn reconcile_structured_source(&mut self) {
        let expired = self
            .structured_source
            .current
            .borrow()
            .as_ref()
            .is_some_and(|open| {
                open.generation != self.app.source_capture_generation
                    || self
                        .app
                        .admit_structured_validation(&open.admission)
                        .is_none()
            });
        if expired {
            self.clear_structured_source();
            self.request_redraw();
        }
    }

    pub(super) fn clear_structured_source(&mut self) {
        self.structured_source.current.borrow_mut().take();
        self.evict_structured_source_panes();
    }

    fn evict_structured_source_panes(&mut self) {
        let surface = SurfaceId::from(SOURCE_SCHEMA);
        let panes: Vec<_> = self
            .renderers
            .contributed
            .iter()
            .filter(|(_, pane)| pane.descriptor().surface_id == surface)
            .map(|(id, _)| id)
            .collect();
        for pane in panes {
            self.evict_pane_renderer(pane);
        }
    }
}

#[cfg(test)]
mod tests;
