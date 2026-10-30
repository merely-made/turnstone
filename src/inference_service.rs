// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's decoder-free client for the resident inference route.
use armillary::Wake;
use chirograph::{
    CarrierRequestBody, CarrierResponseBody, IntentInvocation, IntentResult, PortableCardV1,
    ProjectionRequest,
};
use distillery::inference_protocol::{self as wire, InferenceRunsV1, RunV1};
use eidetic::models::recall::{PassageSelector, RecallSourceId, RetainedSourceText};
use graphshell_endpoint::door::{
    app_admission::{AppId, AppRouteId},
    app_client::AppBrokerClient,
};
use sceno::InstanceId;
use scenotime::{Revision, SceneEpoch};
use std::{
    sync::{Arc, Mutex, mpsc},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use wire::recall::{
    ASK_PUBLISHED_INTENT, MAX_PUBLISHED_CORPUS_BYTES, PUBLISH_CORPUS_INTENT,
    PublishCorpusRequestV1, PublishedAskRequestV1,
};

pub(crate) use crate::inference_contract::{OpenSource, RetainedSourceOwner};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ClientSnapshot {
    pub revision: u64,
    pub status: String,
    pub ready: bool,
    pub model: Option<String>,
    pub runs: Vec<RunV1>,
    pub choices: Vec<crate::research::Choice>,
    pub source: Option<OpenSource>,
}
#[derive(Clone, Copy)]
pub(crate) enum Control {
    Pause,
    Resume,
    Cancel,
}

enum Command {
    Ask {
        run: String,
        question: String,
        chosen: Vec<crate::research::Target>,
        answer_format: wire::recall::AnswerFormat,
    },
    Control {
        run: String,
        control: Control,
    },
    Source {
        run: String,
        handle: u32,
        claim_link: Option<(usize, usize)>,
    },
    Refresh,
}

pub(crate) struct InferenceService {
    commands: mpsc::SyncSender<Command>,
    snapshot: Arc<Mutex<ClientSnapshot>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    owner: String,
}

struct Connection {
    client: AppBrokerClient,
    request: ProjectionRequest,
    published: Option<eidetic::Hash>,
}

#[cfg(test)]
#[path = "inference_service/tests.rs"]
mod tests;

#[path = "inference_service/control_intent.rs"]
mod control_intent;
#[path = "inference_service/impl_drop_for_inference_service.rs"]
mod impl_drop_for_inference_service;
#[path = "inference_service/inference_service_start.rs"]
mod inference_service_start;
#[path = "inference_service/update.rs"]
mod update;
use update::update;
#[path = "inference_service/worker.rs"]
#[cfg(feature = "inference")]
mod worker;
#[cfg(feature = "inference")]
use worker::worker;
#[path = "inference_service/publication.rs"]
mod publication;
use publication::publication;
#[path = "inference_service/intent.rs"]
mod intent;
use intent::intent;
#[path = "inference_service/read_runs.rs"]
mod read_runs;
use read_runs::read_runs;
#[path = "inference_service/resolve_source.rs"]
mod resolve_source;
use resolve_source::resolve_source;
#[path = "inference_service/resolve_claim_source.rs"]
mod resolve_claim_source;
use resolve_claim_source::resolve_claim_source;
