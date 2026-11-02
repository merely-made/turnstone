// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Retained answer card. Generation and durable jobs remain in the resident.
use crate::{
    contributed_surface::{SurfaceAdmissionError, SurfaceProvider},
    inference_service::{ClientSnapshot, Control, InferenceService, RetainedSourceOwner},
    panes::{PaneKindId, PaneSource, SerializedSource, SourceRef, SourceSchemaId},
};
use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, PointerClick, ResolvedSurfaceEvent,
    RetainedSurfaceSession, SurfaceEffect, SurfaceViewport, TextInput, clickable, el, lens,
    text_field_typed,
};
use distillery::inference_protocol::{RunStateV1, RunV1};
use genet_scripted_dom::NodeId;
use mere_surface_api::{
    ProviderId, SourceKindId, SurfaceAvailability, SurfaceDescriptor, SurfaceId, SurfaceSourceShape,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{Arc, Weak},
};

pub(crate) use crate::answer_source_contract::answer_source_value;
pub use crate::answer_source_contract::{AnswerSourceV2, PANE_KIND, SOURCE_SCHEMA, answer_source};
#[derive(Clone, Default)]
pub(crate) struct AnswerConnections(Rc<RefCell<BTreeMap<String, Weak<InferenceService>>>>);
#[derive(Clone, Default)]
struct Draft {
    question: TextInput,
    active: Option<String>,
}
pub struct AnswerProvider {
    kind: PaneKindId,
    schema: SourceSchemaId,
    descriptor: SurfaceDescriptor,
    owner: Arc<dyn RetainedSourceOwner>,
    wake: armillary::Wake,
    connections: AnswerConnections,
    bridge: crate::research::Bridge,
    drafts: RefCell<BTreeMap<String, std::rc::Weak<RefCell<Draft>>>>,
}

const CSS: &str = r#"
.answer-pane { box-sizing: border-box; padding: 12px; color: rgb(230,235,245); font-size: 14px; }
.answer-pane button { color: rgb(230,235,245); background-color: rgb(36,44,62); }
.answer-heading { font-size: 18px; margin-bottom: 8px; }
.answer-controls { display: flex; flex-wrap: wrap; gap: 8px; margin: 8px 0; }
.answer-button { padding: 6px 10px; border: 1px solid rgb(105,115,135); border-radius: 5px; }
.answer-body { white-space: pre-wrap; overflow-wrap: anywhere; margin: 12px 0; }
.answer-status { margin: 8px 0; }
.answer-source { padding: 10px; border: 1px solid rgb(100,110,135); margin: 10px 0; }
.answer-version { font-size: 12px; overflow-wrap: anywhere; }
.answer-history { margin: 12px 0; }
.answer-question { box-sizing: border-box; min-height: 32px; min-width: 160px; width: 100%; padding: 6px 8px; border: 1px solid rgb(105,115,135); border-radius: 5px; color: rgb(230,235,245); background-color: rgb(25,32,48); }
"#;

struct AnswerState {
    draft: Rc<RefCell<Draft>>,
    source: AnswerSourceV2,
    context: crate::research::Context,
    bridge: crate::research::Bridge,
    service: Arc<InferenceService>,
    snapshot: ClientSnapshot,
    question: TextInput,
    active: Option<String>,
    local_status: String,
    show_raw: bool,
    width: f32,
    height: f32,
}
type AnswerView = Box<dyn AnyView<AnswerState, (), GenetCtx, GenetElement>>;
type AnswerRunner = GenetAppRunner<AnswerState, fn(&AnswerState) -> AnswerView, AnswerView, ()>;

struct AnswerSession {
    descriptor: SurfaceDescriptor,
    runner: AnswerRunner,
}

#[cfg(test)]
#[path = "inference_surface/tests.rs"]
mod tests;

#[path = "inference_surface/answer_provider_new.rs"]
mod answer_provider_new;
#[path = "inference_surface/impl_surface_provider_for_answer_provider.rs"]
mod impl_surface_provider_for_answer_provider;
#[path = "inference_surface/row.rs"]
mod row;
use row::row;
#[path = "inference_surface/button.rs"]
mod button;
use button::button;
#[path = "inference_surface/state_name.rs"]
mod state_name;
use state_name::state_name;
#[path = "inference_surface/selected.rs"]
mod selected;
use selected::selected;
#[path = "inference_surface/ask.rs"]
mod ask;
use ask::ask;
#[path = "inference_surface/ask_with_evidence.rs"]
mod ask_with_evidence;
use ask_with_evidence::ask_with_evidence;
#[path = "inference_surface/ask_format.rs"]
mod ask_format;
use ask_format::ask_format;
#[path = "inference_surface/answer_view.rs"]
mod answer_view;
use answer_view::answer_view;
#[path = "inference_surface/answer_session_effects.rs"]
mod answer_session_effects;
#[path = "inference_surface/impl_retained_surface_session_for_answer.rs"]
mod impl_retained_surface_session_for_answer;
