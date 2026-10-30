// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl InferenceService {
    pub fn start(
        owner: String,
        sources: Arc<dyn RetainedSourceOwner>,
        wake: Wake,
    ) -> Result<Arc<Self>, String> {
        #[cfg(not(feature = "inference"))]
        {
            let _ = (owner, sources, wake);
            return Err("Answers require a build with the inference feature".into());
        }
        #[cfg(feature = "inference")]
        {
            let (commands, rx) = mpsc::sync_channel(32);
            let snapshot = Arc::new(Mutex::new(ClientSnapshot {
                status: "Connecting to resident inference…".into(),
                ..Default::default()
            }));
            let state = snapshot.clone();
            let namespace = owner.clone();
            let generation = sources.generation();
            let worker = std::thread::Builder::new()
                .name("turnstone-inference".into())
                .spawn(move || worker(namespace, generation, sources, rx, state, wake))
                .map_err(|e| e.to_string())?;
            Ok(Arc::new(Self {
                commands,
                snapshot,
                worker: Mutex::new(Some(worker)),
                owner,
            }))
        }
    }

    #[cfg(test)]
    pub(crate) fn inert_for_surface_test(owner: String) -> Arc<Self> {
        let (commands, _) = mpsc::sync_channel(1);
        Arc::new(Self {
            commands,
            snapshot: Arc::new(Mutex::new(ClientSnapshot::default())),
            worker: Mutex::new(None),
            owner,
        })
    }

    pub fn snapshot(&self) -> ClientSnapshot {
        self.snapshot.lock().unwrap().clone()
    }

    pub(super) fn send(&self, command: Command) -> Result<(), String> {
        self.commands
            .try_send(command)
            .map_err(|_| "The answer connection is busy or closed.".into())
    }

    pub fn ask(
        &self,
        question: String,
        chosen: Vec<crate::research::Target>,
    ) -> Result<String, String> {
        self.ask_format(question, chosen, wire::recall::AnswerFormat::Prose)
    }

    pub fn ask_with_evidence(
        &self,
        question: String,
        chosen: Vec<crate::research::Target>,
    ) -> Result<String, String> {
        self.ask_format(question, chosen, wire::recall::AnswerFormat::ClaimSupportV1)
    }

    pub(super) fn ask_format(
        &self,
        question: String,
        chosen: Vec<crate::research::Target>,
        answer_format: wire::recall::AnswerFormat,
    ) -> Result<String, String> {
        if question.trim().is_empty() {
            return Err("Enter a question.".into());
        }
        if chosen.is_empty() {
            return Err("Choose at least one saved source before asking.".into());
        }
        let run = format!("{}/{}", self.owner, uuid::Uuid::new_v4());
        self.send(Command::Ask {
            run: run.clone(),
            question,
            chosen,
            answer_format,
        })?;
        Ok(run)
    }

    pub fn control(&self, run: String, control: Control) -> Result<(), String> {
        self.send(Command::Control { run, control })
    }

    pub fn source(&self, run: String, handle: u32) -> Result<(), String> {
        self.source_request(run, handle, None)
    }

    pub fn claim_source(
        &self,
        run: String,
        handle: u32,
        claim: usize,
        link: usize,
    ) -> Result<(), String> {
        self.source_request(run, handle, Some((claim, link)))
    }

    pub(super) fn source_request(
        &self,
        run: String,
        handle: u32,
        claim_link: Option<(usize, usize)>,
    ) -> Result<(), String> {
        if !run.starts_with(&format!("{}/", self.owner)) {
            return Err("The citation belongs to another browsing session.".into());
        }
        // A repeated activation must resolve again even when its contents are
        // identical; retaining the previous result would suppress its revision.
        {
            let mut snapshot = self.snapshot.lock().unwrap();
            snapshot.source = None;
            snapshot.revision += 1;
        }
        self.send(Command::Source {
            run,
            handle,
            claim_link,
        })
    }

    pub fn refresh(&self) -> Result<(), String> {
        self.send(Command::Refresh)
    }
}
