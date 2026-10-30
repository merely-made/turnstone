// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn run(
    commands: Receiver<TrailCommand>,
    out: Emitter<Update>,
    dir: Option<PathBuf>,
    capture_library: crate::place::captured_collection::LocalCaptureLibrary,
    capture_generation: uuid::Uuid,
    limits: Result<crate::research_storage::StorageLimits, String>,
) {
    let initial = dir.as_ref().map(|dir| match &limits {
        Ok(limits) => open_memory(dir, &capture_library, limits),
        Err(error) => Err(error.clone().into()),
    });
    let mut memory_error = initial
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .cloned();
    if let Some(error) = &memory_error {
        capture_library.unavailable(error.to_string());
    }
    let mut state = initial.and_then(Result::ok);
    let mut current_dir = dir.unwrap_or_default();
    let mut current_capture_generation = capture_generation;
    let mut custody: Option<SourceCustody> = None;
    let mut sealed_store: Option<FjallStore> = None;
    let mut sealed_error: Option<crate::research_storage::SourceReadError> = None;
    // The derived lexical/behavioural projection and whether the corpus has
    // moved since it was minted. Built on the first recall, not at
    // spawn: a session that never searches never pays for one.
    let mut index: Option<RecallIndex> = None;
    let mut index_stale = true;
    let mut indexed_sources = Vec::new();
    // Per-owner origin chain: the last destination becomes the next
    // event's `from`.
    let mut last_to: HashMap<String, PageRef> = HashMap::new();
    while let Ok(command) = commands.recv() {
        match command {
            TrailCommand::ResearchRecords(command) => {
                if let Some(error) = &sealed_error {
                    record_actor::reject(command, error.clone());
                } else {
                    record_actor::handle(
                        command,
                        &mut sealed_store,
                        custody.as_ref(),
                        &current_dir,
                        current_capture_generation,
                    );
                }
            },
            TrailCommand::SetCustody(binding) => {
                drop(sealed_store.take());
                custody = binding;
                let custody_ready = custody.as_ref().is_some_and(|binding| {
                    capture_library.bind_custody(binding) && binding.snapshot().is_some()
                });
                if !custody_ready {
                    custody = None;
                }
                if custody_ready {
                    if let Some((store, _)) = state.as_mut()
                        && let Err(error) = worker_capture_ops::refresh(
                            store,
                            &current_dir,
                            &capture_library,
                            &limits,
                        )
                    {
                        capture_library.unavailable(error);
                    }
                } else {
                    capture_library.retire_custody();
                }
                sealed_store = if custody.is_some() && state.is_some() {
                    let (store, error) = worker_open_sealed::run(&current_dir, &limits);
                    sealed_error = error;
                    store
                } else {
                    sealed_error = memory_error.clone();
                    None
                };
                if custody.is_some()
                    && let Some(error) = &sealed_error
                {
                    out.emit(Update::SourceCaptureRecoveryFailed {
                        capture_generation: current_capture_generation,
                        error: error.clone(),
                    });
                }
                if let (Some(binding), Ok(limits)) = (custody.as_ref(), limits.as_ref()) {
                    emit_capture_recovery(
                        &mut state,
                        binding,
                        &current_dir,
                        current_capture_generation,
                        &out,
                        limits,
                    );
                }
                if let (Some(store), Some(binding), Ok(limits)) =
                    (sealed_store.as_mut(), custody.as_ref(), limits.as_ref())
                {
                    emit_sealed_capture_recovery(
                        store,
                        binding,
                        &current_dir,
                        current_capture_generation,
                        &out,
                        limits,
                    );
                }
            },
            TrailCommand::CustodyReady(ready) => {
                let _ = ready.send(());
            },
            TrailCommand::LoadSavedSources {
                generation,
                request,
                session,
                admissions,
                directory,
            } => {
                let custody_epoch = custody.as_ref().and_then(SourceCustody::snapshot);
                let owner = current_dir
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str());
                let mut result = if generation != current_capture_generation
                    || directory != current_dir
                    || owner != Some(session.as_uuid().to_string().as_str())
                    || admissions
                        .iter()
                        .any(|admission| admission.key.session != session)
                {
                    Err("Saved source search belongs to another session opening or store".into())
                } else if custody_epoch.is_none() {
                    Err("Unlock the profile to read private saved sources".into())
                } else if let Some(error) = &sealed_error {
                    Err(error.clone())
                } else {
                    match state.as_mut() {
                        Some((store, _)) => {
                            let budget =
                                limits.as_ref().map(|limits| limits.budget()).map_err(|e| {
                                    crate::research_storage::SourceReadError::from(e.clone())
                                });
                            budget.and_then(|budget| {
                                let mut legacy = crate::research_storage::BudgetStore {
                                    store,
                                    budget: budget.clone(),
                                };
                                let mut sealed = sealed_store.as_mut().map(|store| {
                                    crate::research_storage::BudgetStore { store, budget }
                                });
                                saved_sources::load_with_sealed_bounded(
                                    &mut legacy,
                                    sealed.as_mut().map(|s| s as &mut dyn eidetic::Store),
                                    custody
                                        .as_ref()
                                        .map(|v| v.sealer.as_ref() as &dyn eidetic::PayloadSealer),
                                    &admissions,
                                    None,
                                    None,
                                )
                            })
                        },
                        None => Err(memory_error
                            .clone()
                            .unwrap_or_else(|| "The session evidence store is unavailable".into())),
                    }
                };
                if custody_epoch.is_some()
                    && !custody
                        .as_ref()
                        .is_some_and(|binding| binding.remains_current(custody_epoch))
                {
                    result =
                        Err("Profile custody changed while loading private saved sources".into());
                }
                out.emit(Update::SavedSourcesLoaded {
                    generation,
                    request,
                    session,
                    admissions,
                    result,
                });
            },
            TrailCommand::ReadSavedSource {
                generation,
                request,
                node,
                url,
                target,
                admission,
                directory,
            } => {
                let custody_epoch = custody.as_ref().and_then(SourceCustody::snapshot);
                let owner = current_dir
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str());
                let mut result = if generation != current_capture_generation
                    || directory != current_dir
                    || owner != Some(target.key.session.as_uuid().to_string().as_str())
                    || admission.key != target.key
                {
                    Err(
                        "Saved source reading belongs to another session opening or attachment"
                            .into(),
                    )
                } else if custody_epoch.is_none() {
                    Err("Unlock the profile to read private saved sources".into())
                } else if let Some(error) = &sealed_error {
                    Err(error.clone())
                } else {
                    match state.as_mut() {
                        Some((store, _)) => limits
                            .as_ref()
                            .map_err(|e| crate::research_storage::SourceReadError::from(e.clone()))
                            .and_then(|limits| {
                                saved_sources::resolve_with_budget(
                                    store,
                                    sealed_store.as_ref(),
                                    custody
                                        .as_ref()
                                        .map(|v| v.sealer.as_ref() as &dyn eidetic::PayloadSealer),
                                    &admission,
                                    &target,
                                    limits.budget(),
                                )
                            }),
                        None => Err(memory_error
                            .clone()
                            .unwrap_or_else(|| "The session evidence store is unavailable".into())),
                    }
                };
                if custody_epoch.is_some()
                    && !custody
                        .as_ref()
                        .is_some_and(|binding| binding.remains_current(custody_epoch))
                {
                    result =
                        Err("Profile custody changed while reading a private saved source".into());
                }
                out.emit(Update::SavedSourceRead {
                    generation,
                    request,
                    node,
                    url,
                    target,
                    result,
                });
            },
            TrailCommand::ResearchSnapshot {
                context,
                selected,
                max_chars,
                max_metadata_chars,
                budget,
                reply,
            } => {
                worker_research_snapshot::run(
                    context,
                    selected,
                    max_chars,
                    max_metadata_chars,
                    budget,
                    reply,
                    &mut state,
                    &mut sealed_store,
                    &custody,
                    &current_dir,
                    current_capture_generation,
                    &memory_error,
                    &sealed_error,
                    &limits,
                );
            },
            TrailCommand::ResearchSource {
                context,
                target,
                budget,
                reply,
            } => {
                worker_research_source::run(
                    context,
                    target,
                    budget,
                    reply,
                    &mut state,
                    &mut sealed_store,
                    &custody,
                    &current_dir,
                    current_capture_generation,
                    &memory_error,
                    &sealed_error,
                    &limits,
                );
            },
            TrailCommand::InferenceSnapshot { owner, reply } => {
                let custody_epoch = custody.as_ref().and_then(SourceCustody::snapshot);
                let mut result = if custody_epoch.is_some() {
                    inference_snapshot(state.as_ref().map(|(store, _)| store), &current_dir, &owner)
                } else {
                    Err("Unlock the profile to read private retained sources".into())
                };
                if custody_epoch.is_some()
                    && !custody
                        .as_ref()
                        .is_some_and(|binding| binding.remains_current(custody_epoch))
                {
                    result = Err(
                        "Profile custody changed while loading private retained sources".into(),
                    );
                }
                let _ = reply.send(result);
            },
            TrailCommand::InferenceSource {
                source,
                version,
                reply,
            } => {
                let custody_epoch = custody.as_ref().and_then(SourceCustody::snapshot);
                let mut result = if custody_epoch.is_some() {
                    inference_source(
                        state.as_ref().map(|(store, _)| store),
                        &current_dir,
                        &source,
                        &version,
                    )
                } else {
                    Err("Unlock the profile to read a private retained source".into())
                };
                if custody_epoch.is_some()
                    && !custody
                        .as_ref()
                        .is_some_and(|binding| binding.remains_current(custody_epoch))
                {
                    result = Err(
                        "Profile custody changed while reading a private retained source".into(),
                    );
                }
                let _ = reply.send(result);
            },
            TrailCommand::Record {
                owner,
                url,
                transition,
                at_ms,
            } => {
                let Some((store, memory)) = state.as_mut() else {
                    continue;
                };
                let to = PageRef { url, title: None };
                let event = TraceEvent {
                    from: last_to.insert(owner.clone(), to.clone()),
                    to,
                    transition,
                    at_ms,
                    dwell_ms: None,
                    candidates: Vec::new(),
                };
                index_stale = true;
                if memory.record_traversal(&owner, event) {
                    flush(store, memory);
                }
            },
            TrailCommand::Recall {
                query,
                limit,
                sources,
                config,
            } => {
                let Some((store, memory)) = state.as_mut() else {
                    out.emit(Update::RecallFailed {
                        error: "the trail store is not open".to_string(),
                    });
                    continue;
                };
                let sources = canonical_sources(sources);
                if sources != indexed_sources {
                    index_stale = true;
                    indexed_sources = sources;
                }
                match recall(
                    store,
                    memory,
                    &mut index,
                    &mut index_stale,
                    &current_dir,
                    RecallRequest {
                        sources: &indexed_sources,
                        query: &query,
                        limit,
                        config,
                    },
                ) {
                    Ok(hits) => out.emit(Update::RecallHits { query, hits }),
                    Err(error) => out.emit(Update::RecallFailed { error }),
                }
            },
            TrailCommand::RecordText { url, text } => {
                if let Some((store, _)) = state.as_ref() {
                    keep_page_text(store, &url, &text);
                }
            },
            TrailCommand::CaptureSourceDocument {
                capture_generation,
                request,
                directory,
                document,
            } => {
                let custody_epoch = custody.as_ref().and_then(SourceCustody::snapshot);
                let admitted_session = current_dir
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str());
                let mut result = if directory != current_dir
                    || capture_generation != current_capture_generation
                    || admitted_session != Some(request.session.as_uuid().to_string().as_str())
                {
                    Err("the capture actor belongs to another session opening or directory".into())
                } else if custody_epoch.is_none() {
                    Err("Unlock the profile to retain a private source".into())
                } else {
                    worker_capture_ops::apply(
                        &mut state,
                        &mut sealed_store,
                        custody.as_ref().expect("custody checked"),
                        &limits,
                        &memory_error,
                        &sealed_error,
                        &request,
                        document.as_ref(),
                        false,
                    )
                    .and_then(|stored| stored.ok_or_else(|| "Capture result missing".into()))
                };
                if custody_epoch.is_some()
                    && !custody
                        .as_ref()
                        .is_some_and(|binding| binding.remains_current(custody_epoch))
                {
                    result = Err("Profile custody changed while capturing a private source".into());
                }
                out.emit(Update::SourceDocumentCaptured {
                    capture_generation,
                    request,
                    result,
                });
            },
            TrailCommand::AcknowledgeSourceCapture {
                capture_generation,
                request,
                directory,
            } => {
                let custody_epoch = custody.as_ref().and_then(SourceCustody::snapshot);
                let admitted_session = current_dir
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str());
                let mut result = if directory != current_dir
                    || capture_generation != current_capture_generation
                    || admitted_session != Some(request.session.as_uuid().to_string().as_str())
                {
                    Err(
                        "capture save acknowledgement belongs to another session opening or store"
                            .into(),
                    )
                } else if custody_epoch.is_none() {
                    Err("Unlock the profile to acknowledge a private source".into())
                } else {
                    worker_capture_ops::apply(
                        &mut state,
                        &mut sealed_store,
                        custody.as_ref().expect("custody checked"),
                        &limits,
                        &memory_error,
                        &sealed_error,
                        &request,
                        None,
                        true,
                    )
                    .map(|_| ())
                };
                if custody_epoch.is_some()
                    && !custody
                        .as_ref()
                        .is_some_and(|binding| binding.remains_current(custody_epoch))
                {
                    result =
                        Err("Profile custody changed while acknowledging a private source".into());
                }
                if result.is_ok() {
                    if let Some((store, _)) = state.as_mut()
                        && let Err(error) = worker_capture_ops::refresh(
                            store,
                            &current_dir,
                            &capture_library,
                            &limits,
                        )
                    {
                        capture_library.unavailable(error);
                    }
                }
                out.emit(Update::SourceCaptureSaved {
                    capture_generation,
                    request: request.id,
                    result,
                });
            },
            TrailCommand::Flush => {
                if let Some((store, memory)) = state.as_mut() {
                    flush(store, memory);
                }
            },
            TrailCommand::Reopen {
                directory: dir,
                capture_generation,
            } => {
                if let Some((store, memory)) = state.as_mut() {
                    flush(store, memory);
                }
                last_to.clear();
                // The departing session's index is recreated on first recall.
                index = None;
                index_stale = true;
                indexed_sources.clear();
                // A warm reopen can target this same directory. Release
                // its Fjall lock before evaluating the new open.
                drop(state.take());
                drop(sealed_store.take());
                let opened = match &limits {
                    Ok(limits) => open_memory(&dir, &capture_library, limits),
                    Err(error) => Err(error.clone().into()),
                };
                memory_error = opened.as_ref().err().cloned();
                if let Some(error) = &memory_error {
                    capture_library.unavailable(error.to_string());
                }
                state = opened.ok();
                current_dir = dir;
                current_capture_generation = capture_generation;
                if state.is_some() && custody.is_some() {
                    (sealed_store, sealed_error) = worker_open_sealed::run(&current_dir, &limits);
                } else if state.is_none() {
                    sealed_error = memory_error.clone();
                }
                if custody.is_some()
                    && let Some(error) = &sealed_error
                {
                    out.emit(Update::SourceCaptureRecoveryFailed {
                        capture_generation: current_capture_generation,
                        error: error.clone(),
                    });
                }
                if let (Some(binding), Ok(limits)) = (custody.as_ref(), limits.as_ref()) {
                    emit_capture_recovery(
                        &mut state,
                        binding,
                        &current_dir,
                        current_capture_generation,
                        &out,
                        limits,
                    );
                }
                if let (Some(store), Some(binding), Ok(limits)) =
                    (sealed_store.as_mut(), custody.as_ref(), limits.as_ref())
                {
                    emit_sealed_capture_recovery(
                        store,
                        binding,
                        &current_dir,
                        current_capture_generation,
                        &out,
                        limits,
                    );
                }
            },
            TrailCommand::Release(ack) => {
                if let Some((store, memory)) = state.as_mut() {
                    flush(store, memory);
                }
                state = None;
                sealed_store = None;
                sealed_error = None;
                index = None;
                index_stale = true;
                indexed_sources.clear();
                last_to.clear();
                capture_library.unavailable("the session capture store is released");
                let _ = ack.send(());
            },
        }
    }
}
