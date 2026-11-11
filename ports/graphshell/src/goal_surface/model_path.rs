// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The opt-in model path of the goal surface (feature `goal-surface-model`).
//!
//! Eidetic's `ModelGoalAssessor` (S3, wrapping the lexical S1) drives the same
//! bounded Fleece traversal as the ordinary path. The model is a local folder
//! the person names (`GRAPHSHELL_GOAL_MODEL_DIR`); it is never downloaded, its
//! files are checked against pinned SHA-256 digests before anything is loaded,
//! and loading and every model run happen on one worker thread so the UI
//! thread only ever sends a request and polls for the answer. The worker owns
//! the embedder; the request carries a copy of the retained snapshot.

use super::{Budgets, GoalRunView, GoalSurface};
use eidetic_search::goal::GoalSettings;
use eidetic_search::goal_model::{EmbedRole, GoalEmbedder, ModelAssessRule, ModelGoalAssessor, ModelGoalSettings};
use esp::embed::goal_model::{EspGoalEmbedder, recommended_match_similarity_milli};
use fleece::retained_traversal::{
    RetainedLimits, RetainedSeed, RetainedTraversalReceipt, run_retained_traversal_with,
};
use fleece::traversal::TraversalPolicy;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    mpsc::{Receiver, Sender, channel},
};

/// Where the model folder is named.
pub const ENV_DIR: &str = "GRAPHSHELL_GOAL_MODEL_DIR";

/// The files a model folder must hold, with the SHA-256 each must have. They
/// are the all-MiniLM-L6-v2 checkpoint Eidetic calibrated; another model needs
/// its own calibration before it is trusted here.
pub const PINNED: [(&str, &str); 3] = [
    ("config.json", "953f9c0d463486b10a6871cc2fd59f223b2c70184f49815e7efbcab5d8908b41"),
    ("tokenizer.json", "be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037"),
    ("model.safetensors", "53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db"),
];

/// A wake-up for the UI thread, called by the worker after it sends an event.
pub type Waker = Arc<dyn Fn() + Send + Sync>;
pub type Embedder = Box<dyn GoalEmbedder + Send>;

/// What the worker learned when it loaded a model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loaded {
    pub label: String,
    /// `(file, sha256)` of each artifact, verified before loading.
    pub digests: Vec<(String, String)>,
    pub threshold_milli: i16,
    /// Whether the threshold is calibrated for this model, in a sentence.
    pub threshold_note: String,
    pub load_ms: u128,
}

/// Builds the embedder on the worker thread. The real one verifies and loads
/// a folder; tests supply a stub.
pub type Loader = Box<dyn FnOnce() -> Result<(Embedder, Loaded), String> + Send>;
pub type LoaderFor = Box<dyn Fn(&Path) -> Loader>;

pub fn dir_from_env() -> Option<PathBuf> {
    std::env::var_os(ENV_DIR).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn sha256(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let (mut hasher, mut buffer) = (Sha256::new(), [0_u8; 64 * 1024]);
    loop {
        let n = file.read(&mut buffer).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Check every pinned file before anything is loaded.
pub fn verify(dir: &Path) -> Result<Vec<(String, String)>, String> {
    PINNED
        .iter()
        .map(|(name, pinned)| {
            let found = sha256(&dir.join(name))?;
            if found == *pinned {
                Ok(((*name).to_owned(), found))
            } else {
                Err(format!("{name} is not the pinned all-MiniLM-L6-v2 file (sha256 {})", &found[..12]))
            }
        })
        .collect()
}

/// Wrap a ready embedder with the threshold the model's calibration gives.
pub fn calibrated(embedder: Embedder, label: String, digests: Vec<(String, String)>, load_ms: u128) -> (Embedder, Loaded) {
    let generic = ModelGoalSettings::default().match_similarity_milli;
    let space = embedder.envelope().space;
    let (threshold_milli, threshold_note) = match recommended_match_similarity_milli(&space) {
        Some(t) => (t, format!("calibrated for this model; the generic default {} is wrong for it", super::score(generic))),
        None => (
            generic,
            "the generic default: this model has no calibration, and cosine scales differ per model, so treat matches as a placeholder".into(),
        ),
    };
    (embedder, Loaded { label, digests, threshold_milli, threshold_note, load_ms })
}

/// Verify and load the MiniLM folder on the portable CPU backend.
pub fn real_loader(dir: &Path) -> Loader {
    let dir = dir.to_owned();
    Box::new(move || {
        let digests = verify(&dir)?;
        let started = std::time::Instant::now();
        let provider = esp::embed::bert::load_cpu(&dir).map_err(|e| format!("cannot load the model: {e}"))?;
        let embedder = EspGoalEmbedder::new(provider)?;
        let label = dir.file_name().map_or_else(|| "model".into(), |n| n.to_string_lossy().into_owned());
        Ok(calibrated(Box::new(embedder), label, digests, started.elapsed().as_millis()))
    })
}

/// A goal run to be done by the model, without the retained snapshot.
#[derive(Clone)]
pub struct Request {
    pub seeds: Vec<RetainedSeed>,
    pub goal: String,
    pub policy: TraversalPolicy,
    pub limits: RetainedLimits,
    pub budgets: Budgets,
}

struct Job {
    request: Request,
    pages: super::RetainedPages,
    rule: ModelAssessRule,
    loaded: Loaded,
}

pub struct Done {
    pub run: GoalRunView,
    pub receipt: RetainedTraversalReceipt,
}

enum Event {
    Loaded(Loaded),
    LoadFailed(String),
    Ran(Result<Done, String>),
}

struct Borrowed<'a>(&'a mut (dyn GoalEmbedder + Send));
impl GoalEmbedder for Borrowed<'_> {
    fn envelope(&self) -> eidetic::models::embedding::EmbeddingEnvelopeV1 {
        self.0.envelope()
    }
    fn embed(&mut self, texts: &[&str], role: EmbedRole) -> Result<Vec<Vec<f32>>, String> {
        self.0.embed(texts, role)
    }
}

/// Run one request with `embedder`. Synchronous: the worker calls it, and so
/// can a test.
pub fn run_request(
    request: Request,
    pages: &super::RetainedPages,
    rule: ModelAssessRule,
    loaded: &Loaded,
    embedder: &mut (dyn GoalEmbedder + Send),
) -> Result<Done, String> {
    let settings = ModelGoalSettings {
        assess_rule: rule,
        match_similarity_milli: loaded.threshold_milli,
        ..ModelGoalSettings::default()
    };
    let assessor = ModelGoalAssessor::new(&request.goal, GoalSettings::default(), settings, Borrowed(embedder))?;
    let mut provider = pages.provider();
    let (report, _) = run_retained_traversal_with(
        &mut provider,
        request.seeds,
        &request.goal,
        assessor,
        request.policy,
        request.limits,
        || false,
    );
    let mut run = GoalRunView::build(&report, pages, request.budgets);
    if let Some(model) = run.model.as_mut() {
        model.threshold_note = loaded.threshold_note.clone();
    }
    Ok(Done { run, receipt: report.receipt() })
}

struct Worker {
    jobs: Sender<Job>,
    events: Receiver<Event>,
}

impl Worker {
    fn spawn(load: Loader, wake: Option<Waker>) -> Self {
        let (jobs, job_rx) = channel::<Job>();
        let (event_tx, events) = channel::<Event>();
        let tell = move |event: Event| {
            let _ = event_tx.send(event);
            if let Some(wake) = &wake {
                wake();
            }
        };
        std::thread::spawn(move || match load() {
            Err(why) => tell(Event::LoadFailed(why)),
            Ok((mut embedder, loaded)) => {
                tell(Event::Loaded(loaded));
                for job in job_rx {
                    let done = run_request(job.request, &job.pages, job.rule, &job.loaded, embedder.as_mut());
                    tell(Event::Ran(done));
                }
            },
        });
        Self { jobs, events }
    }
}

/// Where the model is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelState {
    /// Nothing has been asked of it yet.
    Idle,
    Loading,
    Ready(Loaded),
    Failed(String),
}

/// The toggle's state and the worker behind it. View state only.
pub struct ModelPanel {
    /// The configured model folder; `None` disables the toggle.
    pub dir: Option<PathBuf>,
    pub enabled: bool,
    pub rule: ModelAssessRule,
    pub state: ModelState,
    pub running: bool,
    pub waker: Option<Waker>,
    loader: LoaderFor,
    worker: Option<Worker>,
    waiting: Option<Request>,
    in_flight: Option<Request>,
}

impl Default for ModelPanel {
    fn default() -> Self {
        Self {
            dir: None,
            enabled: false,
            rule: ModelAssessRule::Or,
            state: ModelState::Idle,
            running: false,
            waker: None,
            loader: Box::new(real_loader),
            worker: None,
            waiting: None,
            in_flight: None,
        }
    }
}

impl ModelPanel {
    /// Whether no request is waiting for the model to load.
    pub fn waiting_is_empty(&self) -> bool {
        self.waiting.is_none()
    }

    /// Replace how a folder becomes an embedder (tests use a stub).
    pub fn set_loader(&mut self, loader: LoaderFor) {
        self.loader = loader;
    }

    /// Why the toggle is disabled, or `None` when it can be used.
    pub fn disabled_reason(&self) -> Option<String> {
        self.dir.is_none().then(|| {
            format!("No model is configured. Set {ENV_DIR} to a folder holding config.json, tokenizer.json and model.safetensors.")
        })
    }

    /// One sentence on the model's state, for the status line.
    pub fn status(&self) -> String {
        if let Some(reason) = self.disabled_reason() {
            return reason;
        }
        match (&self.state, self.enabled) {
            (ModelState::Idle, false) => "Off. Find uses the ordinary lexical path.".into(),
            (ModelState::Idle | ModelState::Loading, true) => "Loading the model in the background; Find waits for it.".into(),
            (ModelState::Ready(l), true) => format!(
                "On: {} ready (loaded in {} ms, checksums verified). Match threshold {}: {}.",
                l.label, l.load_ms, super::score(l.threshold_milli), l.threshold_note
            ),
            (ModelState::Ready(l), false) => format!("Off. {} stays loaded.", l.label),
            (ModelState::Failed(why), _) => format!("Model unavailable: {why}. Find uses the ordinary lexical path."),
            (ModelState::Loading, false) => "Off. The model is still loading.".into(),
        }
    }

    fn load(&mut self) {
        let Some(dir) = self.dir.clone() else { return };
        self.state = ModelState::Loading;
        self.worker = Some(Worker::spawn((self.loader)(&dir), self.waker.clone()));
    }

    /// Flip the toggle; the first time on starts the background load.
    pub fn toggle(&mut self) {
        if self.dir.is_none() {
            return;
        }
        self.enabled = !self.enabled;
        if self.enabled && self.worker.is_none() {
            self.load();
        }
    }
}

impl GoalSurface {
    /// Hand a request to the model, or say why the ordinary path runs.
    pub(super) fn run_with_model(&mut self, request: Request) {
        match self.model.state.clone() {
            ModelState::Ready(loaded) => self.submit(request, &loaded),
            ModelState::Idle | ModelState::Loading => {
                self.model.waiting = Some(request);
                self.notice = Some("Loading the model; the run starts when it is ready.".into());
            },
            ModelState::Failed(why) => self.fall_back(request, &why),
        }
    }

    fn submit(&mut self, request: Request, loaded: &Loaded) {
        let panel = &mut self.model;
        let job = Job { request: request.clone(), pages: (*self.pages).clone(), rule: panel.rule, loaded: loaded.clone() };
        match panel.worker.as_ref().map(|w| w.jobs.send(job)) {
            Some(Ok(())) => {
                panel.running = true;
                panel.in_flight = Some(request);
                self.notice = Some("Running with the model in the background.".into());
            },
            _ => {
                panel.state = ModelState::Failed("the model worker stopped".into());
                self.fall_back(request, "the model worker stopped");
            },
        }
    }

    fn fall_back(&mut self, request: Request, why: &str) {
        self.notice = None;
        self.run_lexical(request.seeds, &request.goal, request.policy, request.limits, request.budgets);
        if self.notice.is_none() {
            self.notice = Some(format!("The model is unavailable ({why}); this run used the ordinary lexical path."));
        }
    }

    /// Take whatever the worker has finished. A host calls this when woken.
    /// Returns whether anything changed.
    pub fn poll_model(&mut self) -> bool {
        let mut changed = false;
        while let Some(event) = self.model.worker.as_ref().and_then(|w| w.events.try_recv().ok()) {
            changed = true;
            match event {
                Event::Loaded(loaded) => {
                    self.model.state = ModelState::Ready(loaded.clone());
                    if let Some(request) = self.model.waiting.take() {
                        self.submit(request, &loaded);
                    }
                },
                Event::LoadFailed(why) => {
                    self.model.state = ModelState::Failed(why.clone());
                    self.model.worker = None;
                    if let Some(request) = self.model.waiting.take() {
                        self.fall_back(request, &why);
                    }
                },
                Event::Ran(done) => {
                    self.model.running = false;
                    let request = self.model.in_flight.take();
                    match (done, request) {
                        (Ok(done), _) => {
                            self.run = Some(done.run);
                            self.receipt = Some(done.receipt);
                            self.notice = None;
                        },
                        (Err(why), Some(request)) => self.fall_back(request, &why),
                        (Err(why), None) => self.notice = Some(format!("The model run failed: {why}")),
                    }
                },
            }
        }
        changed
    }
}
