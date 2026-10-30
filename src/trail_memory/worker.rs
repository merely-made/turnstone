// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn run(
    commands: Receiver<TrailCommand>,
    out: Emitter<Update>,
    dir: Option<PathBuf>,
    capture_library: crate::place::captured_collection::LocalCaptureLibrary,
    capture_generation: uuid::Uuid,
) {
    let mut state = dir
        .as_ref()
        .and_then(|dir| open_memory(dir, &capture_library));
    let mut current_dir = dir.unwrap_or_default();
    let mut current_capture_generation = capture_generation;
    let mut custody: Option<SourceCustody> = None;
    let mut sealed_store: Option<FjallStore> = None;
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
            TrailCommand::ResearchRecords(command) => record_actor::handle(
                command,
                &mut sealed_store,
                custody.as_ref(),
                &current_dir,
                current_capture_generation,
            ),
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
                        && let Err(error) =
                            crate::place::captured_collection::refresh_local_capture_library(
                                store,
                                &current_dir,
                                &capture_library,
                            )
                    {
                        capture_library.unavailable(error);
                    }
                } else {
                    capture_library.retire_custody();
                }
                sealed_store = if custody.is_some() && state.is_some() {
                    FjallStore::open(sealed_memory_dir(&current_dir)).ok()
                } else {
                    None
                };
                if let Some(binding) = custody.as_ref() {
                    emit_capture_recovery(
                        &mut state,
                        binding,
                        &current_dir,
                        current_capture_generation,
                        &out,
                    );
                }
                if let (Some(store), Some(binding)) = (sealed_store.as_mut(), custody.as_ref()) {
                    emit_sealed_capture_recovery(
                        store,
                        binding,
                        &current_dir,
                        current_capture_generation,
                        &out,
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
                } else {
                    match state.as_mut() {
                        Some((store, _)) => Ok(saved_sources::load_with_sealed(
                            store,
                            sealed_store.as_mut(),
                            custody
                                .as_ref()
                                .map(|v| v.sealer.as_ref() as &dyn eidetic::PayloadSealer),
                            &admissions,
                        )),
                        None => Err("The session evidence store is unavailable".into()),
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
                } else {
                    match state.as_mut() {
                        Some((store, _)) => saved_sources::resolve_with_sealed(
                            store,
                            sealed_store.as_mut(),
                            custody
                                .as_ref()
                                .map(|v| v.sealer.as_ref() as &dyn eidetic::PayloadSealer),
                            &admission,
                            &target,
                        ),
                        None => Err("The session evidence store is unavailable".into()),
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
                reply,
            } => {
                let custody_epoch = custody.as_ref().and_then(SourceCustody::snapshot);
                let valid = context.generation == current_capture_generation
                    && inference_owner(&current_dir).ok()
                        == Some(format!("turnstone/{}", context.session.as_uuid()))
                    && context
                        .admissions
                        .iter()
                        .all(|a| a.key.session == context.session)
                    && selected.as_ref().is_none_or(|selected| {
                        selected.iter().all(|key| {
                            context
                                .admissions
                                .iter()
                                .any(|admission| &admission.key == key)
                        })
                    });
                let mut result = if !valid {
                    Err("The saved-source owner belongs to another session opening.".into())
                } else if custody_epoch.is_none() {
                    Err("Unlock the profile to read private saved sources.".into())
                } else {
                    state
                        .as_mut()
                        .map(|(store, _)| {
                            let mut seen = std::collections::HashSet::new();
                            let admissions = context
                                .admissions
                                .iter()
                                .filter(|admission| {
                                    selected
                                        .as_ref()
                                        .is_none_or(|keys| keys.contains(&admission.key))
                                        && (max_chars.is_none()
                                            || seen.insert(admission.key.clone()))
                                })
                                .cloned()
                                .collect::<Vec<_>>();
                            saved_sources::load_with_sealed_bounded(
                                store,
                                sealed_store.as_mut(),
                                custody
                                    .as_ref()
                                    .map(|v| v.sealer.as_ref() as &dyn eidetic::PayloadSealer),
                                &admissions,
                                max_chars,
                                max_metadata_chars,
                            )
                        })
                        .ok_or_else(|| "The session evidence store is unavailable.".to_string())
                        .and_then(|result| result)
                };
                if custody_epoch.is_some()
                    && !custody
                        .as_ref()
                        .is_some_and(|binding| binding.remains_current(custody_epoch))
                {
                    result =
                        Err("Profile custody changed while loading private saved sources.".into());
                }
                let _ = reply.send(result);
            },
            TrailCommand::ResearchSource {
                context,
                target,
                reply,
            } => {
                let custody_epoch = custody.as_ref().and_then(SourceCustody::snapshot);
                let valid = context.generation == current_capture_generation
                    && inference_owner(&current_dir).ok()
                        == Some(format!("turnstone/{}", context.session.as_uuid()))
                    && target.key.session == context.session;
                let mut result = if !valid {
                    Err("The cited source belongs to another session opening.".into())
                } else if custody_epoch.is_none() {
                    Err("Unlock the profile to read private saved sources.".into())
                } else if let Some(admission) = context
                    .admissions
                    .iter()
                    .find(|a| a.key == target.key && a.error.is_none())
                {
                    match state.as_mut() {
                        Some((store, _)) => saved_sources::resolve_with_sealed(
                            store,
                            sealed_store.as_mut(),
                            custody
                                .as_ref()
                                .map(|v| v.sealer.as_ref() as &dyn eidetic::PayloadSealer),
                            admission,
                            &target,
                        ),
                        None => Err("The session evidence store is unavailable.".into()),
                    }
                } else {
                    Err("The cited saved source was withdrawn from this session.".into())
                };
                if custody_epoch.is_some()
                    && !custody
                        .as_ref()
                        .is_some_and(|binding| binding.remains_current(custody_epoch))
                {
                    result =
                        Err("Profile custody changed while reading a private cited source.".into());
                }
                let _ = reply.send(result);
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
                    let legacy_key = format!("turnstone/source-capture-request/v1/{}", request.id);
                    let legacy = state
                        .as_ref()
                        .map(|(store, _)| pollster::block_on(store.get(&legacy_key)));
                    if matches!(legacy.as_ref(), Some(Ok(Some(_)))) {
                        source_capture_receipts::completed_legacy(
                            &mut state.as_mut().expect("legacy store checked").0,
                            &request,
                        )
                    } else if !matches!(legacy.as_ref(), Some(Ok(None))) {
                        Err("legacy capture store unavailable".into())
                    } else {
                        match (sealed_store.as_mut(), custody.as_ref()) {
                            (Some(store), Some(binding)) => sealed_capture::capture_request(
                                store,
                                binding.sealer.as_ref(),
                                &request,
                                document.as_ref(),
                            ),
                            _ => Err("Unlock the profile to retain a private source".into()),
                        }
                    }
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
                    let legacy_key = format!("turnstone/source-capture-request/v1/{}", request.id);
                    let legacy = state
                        .as_ref()
                        .map(|(store, _)| pollster::block_on(store.get(&legacy_key)));
                    if matches!(legacy.as_ref(), Some(Ok(Some(_)))) {
                        source_capture_receipts::acknowledge_saved(
                            &mut state.as_mut().expect("legacy store checked").0,
                            &request,
                        )
                    } else if !matches!(legacy.as_ref(), Some(Ok(None))) {
                        Err("legacy capture store unavailable".into())
                    } else {
                        match (sealed_store.as_mut(), custody.as_ref()) {
                            (Some(store), Some(binding)) => sealed_capture::acknowledge(
                                store,
                                binding.sealer.as_ref(),
                                &request,
                            ),
                            _ => Err("Unlock the profile to acknowledge a private source".into()),
                        }
                    }
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
                        && let Err(error) =
                            crate::place::captured_collection::refresh_local_capture_library(
                                store,
                                &current_dir,
                                &capture_library,
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
                // The index belongs to the departing session's corpus;
                // the adopted one mints its own on first recall.
                index = None;
                index_stale = true;
                indexed_sources.clear();
                // A warm reopen can target this same directory. Release
                // its Fjall lock before evaluating the new open.
                drop(state.take());
                drop(sealed_store.take());
                state = open_memory(&dir, &capture_library);
                current_dir = dir;
                current_capture_generation = capture_generation;
                if state.is_some() && custody.is_some() {
                    sealed_store = FjallStore::open(sealed_memory_dir(&current_dir)).ok();
                }
                if let Some(binding) = custody.as_ref() {
                    emit_capture_recovery(
                        &mut state,
                        binding,
                        &current_dir,
                        current_capture_generation,
                        &out,
                    );
                }
                if let (Some(store), Some(binding)) = (sealed_store.as_mut(), custody.as_ref()) {
                    emit_sealed_capture_recovery(
                        store,
                        binding,
                        &current_dir,
                        current_capture_generation,
                        &out,
                    );
                }
            },
            TrailCommand::Release(ack) => {
                if let Some((store, memory)) = state.as_mut() {
                    flush(store, memory);
                }
                state = None;
                sealed_store = None;
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
