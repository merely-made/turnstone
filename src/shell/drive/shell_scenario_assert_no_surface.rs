// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn scenario_assert_no_surface(
        &mut self,
        step: &crate::scenario::Step,
    ) -> Result<(), String> {
        use super::super::drive::{cmp_f32, cmp_usize};
        use crate::action::CaretMove;
        use crate::scenario::{CmpOp, EditKey, Step};
        match step {
            Step::AssertNoSurface(kind) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.surfaces.iter().any(|s| s == kind) {
                    return Err(format!(
                        "assert no-surface '{kind}': the primary plan is {:?}",
                        snap.surfaces
                    ));
                }
            },
            Step::AssertReaderAppearances(op, want) => {
                let appearances = self.reader_appearance_observations();
                if appearances
                    .windows(2)
                    .any(|pair| pair[0].id.0 >= pair[1].id.0)
                {
                    return Err(format!(
                        "assert reader-appearances: observations are not sorted: {}",
                        Self::describe_reader_appearances(&appearances)
                    ));
                }
                if !cmp_usize(op, appearances.len(), *want) {
                    return Err(format!(
                        "assert reader-appearances: got {}, expected {:?} {}; {}",
                        appearances.len(),
                        op,
                        want,
                        Self::describe_reader_appearances(&appearances)
                    ));
                }
            },
            Step::AssertReaderSources(op, want) => {
                let appearances = self.reader_appearance_observations();
                let sources = appearances
                    .iter()
                    .map(|appearance| appearance.source_group)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len();
                if !cmp_usize(op, sources, *want) {
                    return Err(format!(
                        "assert reader-sources: got {}, expected {:?} {}; {}",
                        sources,
                        op,
                        want,
                        Self::describe_reader_appearances(&appearances)
                    ));
                }
            },
            Step::AssertReaderRole(role) => {
                let role = super::super::reader_observe::ReaderAppearanceRole::parse(role)
                    .ok_or_else(|| format!("assert reader-role: unknown Reader role '{role}'"))?;
                let appearances = self.reader_appearance_observations();
                let count = appearances
                    .iter()
                    .filter(|appearance| appearance.role == role)
                    .count();
                if count != 1 {
                    return Err(format!(
                        "assert reader-role {}: got {count}; {}",
                        role.label(),
                        Self::describe_reader_appearances(&appearances)
                    ));
                }
            },
            Step::AssertReaderViewport(role) => {
                let role = super::super::reader_observe::ReaderAppearanceRole::parse(role)
                    .ok_or_else(|| {
                        format!("assert reader-viewport: unknown Reader role '{role}'")
                    })?;
                let appearances = self.reader_appearance_observations();
                let matching = appearances
                    .iter()
                    .filter(|appearance| appearance.role == role)
                    .collect::<Vec<_>>();
                let [appearance] = matching.as_slice() else {
                    return Err(format!(
                        "assert reader-viewport {}: expected one appearance; {}",
                        role.label(),
                        Self::describe_reader_appearances(&appearances)
                    ));
                };
                // The reader is laid out in layout pixels, as pelt sizes it.
                let expected = (
                    (appearance.rect.w / appearance.layout_scale)
                        .max(1.0)
                        .ceil() as u32,
                    (appearance.rect.h / appearance.layout_scale)
                        .max(1.0)
                        .ceil() as u32,
                );
                if appearance.viewport != expected {
                    return Err(format!(
                        "assert reader-viewport {}: got {}x{}, plan requires {}x{}; {}",
                        role.label(),
                        appearance.viewport.0,
                        appearance.viewport.1,
                        expected.0,
                        expected.1,
                        Self::describe_reader_appearances(&appearances)
                    ));
                }
            },
            Step::AssertReaderScroll(role, op, want) => {
                let role = super::super::reader_observe::ReaderAppearanceRole::parse(role)
                    .ok_or_else(|| format!("assert reader-scroll: unknown Reader role '{role}'"))?;
                let appearances = self.reader_appearance_observations();
                let matching = appearances
                    .iter()
                    .filter(|appearance| appearance.role == role)
                    .collect::<Vec<_>>();
                let [appearance] = matching.as_slice() else {
                    return Err(format!(
                        "assert reader-scroll {}: expected one appearance; {}",
                        role.label(),
                        Self::describe_reader_appearances(&appearances)
                    ));
                };
                if !cmp_f32(op, appearance.scroll_y, *want) {
                    return Err(format!(
                        "assert reader-scroll {}: got {:.1}, expected {:?} {:.1}; {}",
                        role.label(),
                        appearance.scroll_y,
                        op,
                        want,
                        Self::describe_reader_appearances(&appearances)
                    ));
                }
            },
            Step::AssertContentSessionsIdle => {
                let diagnosis = self.idle_diagnosis();
                if !diagnosis.unsettled_sessions.is_empty() {
                    return Err(format!(
                        "assert content-sessions-idle: {}",
                        diagnosis.describe()
                    ));
                }
            },
            Step::AssertContentReady => {
                let diagnosis = self.idle_diagnosis();
                if !diagnosis.content_ready() {
                    return Err(format!("assert content-ready: {}", diagnosis.describe()));
                }
            },
            Step::AssertEvent(_) => {},
            Step::WaitRow(frames, substr) => {
                let ok = self.wait_frames(*frames, |shell| {
                    let snap = crate::observe::snapshot(&shell.app);
                    snap.trail_rows
                        .iter()
                        .chain(snap.roster_rows.iter())
                        .chain(snap.inspector_rows.iter())
                        .chain(snap.arrange_rows.iter())
                        .any(|r| r.contains(substr.as_str()))
                });
                if !ok {
                    let snap = crate::observe::snapshot(&self.app);
                    return Err(format!(
                        "wait-row '{substr}' after {frames} frames: trail {:?} roster {:?} inspector {:?} arrange {:?}",
                        snap.trail_rows, snap.roster_rows, snap.inspector_rows, snap.arrange_rows
                    ));
                }
            },
            Step::WaitStatus(frames, substr) => {
                self.pending_scenario_wait =
                    Some(PendingScenarioWait::status(substr.clone(), *frames));
            },
            Step::WaitKnot(frames, substr) => {
                fn knot_lines(app: &crate::app::App) -> Vec<String> {
                    app.knot_documents()
                        .iter()
                        .map(|doc| format!("{} {}", doc.status, doc.address))
                        .collect()
                }
                let ok = self.wait_frames(*frames, |shell| {
                    knot_lines(&shell.app)
                        .iter()
                        .any(|line| line.contains(substr.as_str()))
                });
                if !ok {
                    let lines = knot_lines(&self.app);
                    return Err(format!(
                        "wait-knot '{substr}' after {frames} frames: knot documents are {lines:?}"
                    ));
                }
            },
            Step::WaitSourceCapture(frames, ordinal, phase) => {
                let phase_of = |phase: crate::source_capture::CapturePhase| match phase {
                    crate::source_capture::CapturePhase::Depositing => "depositing",
                    crate::source_capture::CapturePhase::Deposited => "deposited",
                    crate::source_capture::CapturePhase::PendingSave => "pending-save",
                    crate::source_capture::CapturePhase::Failed => "failed",
                    crate::source_capture::CapturePhase::Saved => "saved",
                    crate::source_capture::CapturePhase::Recoverable => "recoverable",
                };
                let ok = self.wait_frames(*frames, |shell| {
                    crate::observe::snapshot(&shell.app)
                        .source_captures
                        .iter()
                        .any(|capture| {
                            capture.ordinal == *ordinal && phase_of(capture.phase) == phase
                        })
                });
                if !ok {
                    let current = crate::observe::snapshot(&self.app)
                        .source_captures
                        .iter()
                        .map(|capture| (capture.ordinal, phase_of(capture.phase)))
                        .collect::<Vec<_>>();
                    return Err(format!(
                        "wait-source-capture {ordinal} {phase} after {frames} frames: observed {current:?}"
                    ));
                }
            },
            Step::WaitSavedSources(frames) => {
                let ready = self.wait_frames(*frames, |shell| {
                    shell.app.saved_sources.request.is_none()
                        && shell.app.saved_sources.status == "Ready"
                });
                if !ready {
                    return Err(format!(
                        "wait-saved-sources after {frames} frames: status is '{}' with request {:?}",
                        self.app.saved_sources.status, self.app.saved_sources.request,
                    ));
                }
            },
            Step::WaitResearchSearch(frames) => {
                if !self.wait_frames(*frames, |shell| {
                    shell.app.scenario_research_search_summary().is_some()
                }) {
                    return Err(format!(
                        "wait-research-search after {frames} frames: no current admitted result"
                    ));
                }
            },
            Step::WaitStructuredAuthorSave(frames) => {
                self.wait_frames(*frames, |shell| {
                    shell.app.structured_scenario_author_save_ready()
                        || shell.app.structured_scenario_status()["error"].is_string()
                });
                if !self.app.structured_scenario_author_save_ready() {
                    let status = self.app.structured_scenario_status();
                    return Err(format!(
                        "wait-structured-author-save after {frames} frames: no current confirmed save; error={}",
                        status["error"]
                    ));
                }
            },
            Step::WaitStructuredIdle(frames) => self.scenario_wait_structured(*frames, false)?,
            Step::WaitStructuredLimitRefusal(frames) => self.scenario_wait_structured(*frames, true)?,
            Step::WaitStructuredResult(frames) => {
                if !self.wait_frames(*frames, |shell| {
                    shell.app.structured_scenario_result_ready()
                }) {
                    return Err(format!(
                        "wait-structured-result after {frames} frames: no current saved result or historical replay"
                    ));
                }
            },
            Step::WaitSourceTarget(frames, url) => {
                if !self.wait_frames(*frames, |shell| {
                    crate::scenario::source_target_ready(&shell.app.available_actions(), url)
                }) {
                    return Err(format!(
                        "wait-source-target after {frames} frames: {}",
                        crate::scenario::source_target_wait_diagnostic(&self.app, url)
                    ));
                }
            },
            Step::WaitRetainedClip(frames) => {
                self.wait_frames(*frames, |shell| shell.latest_retention_ready() || shell.latest_retention_error().is_some());
                if !self.latest_retention_ready() {
                    return Err(format!("wait-retained-clip after {frames} frames: {}", self.latest_retention_error().unwrap_or("no current successful retention")));
                }
            },
            Step::WaitRetainedReader(frames) => {
                if !self.wait_frames(*frames, Shell::focused_retained_reader_ready) {
                    return Err(format!("wait-retained-reader after {frames} frames: no current focused admitted live reader"));
                }
            },
            Step::WaitSavedReader(frames) => {
                if !self.wait_frames(*frames, Shell::focused_saved_reader_ready) {
                    let node = self.focused_saved_reader_node();
                    let state = node
                        .and_then(|node| self.app.saved_readers.get(&node))
                        .map(|reader| {
                            if reader.error.is_some() {
                                "Error"
                            } else if reader.request.is_some() || reader.content.is_none() {
                                "Pending"
                            } else {
                                "Content ready; live saved-source engine unavailable"
                            }
                        })
                        .unwrap_or("None");
                    return Err(format!(
                        "wait-saved-reader after {frames} frames: focused reader state is {state}"
                    ));
                }
            },
            Step::WaitFile(frames, path) => {
                self.pending_scenario_wait = Some(PendingScenarioWait::file(path.into(), *frames));
            },
            Step::RecordPlace(name) => {
                let value = crate::observe::place_record(&self.app);
                let body = serde_json::to_string_pretty(&value).map_err(|error| {
                    format!("record-place '{name}': could not serialize: {error}")
                })?;
                let path = self.shared_out_dir.join(format!("{name}.json"));
                std::fs::write(&path, body).map_err(|error| {
                    format!(
                        "record-place '{name}': could not write {}: {error}",
                        path.display()
                    )
                })?;
            },
            Step::RecordSourceCaptures(name) => {
                let snapshot = crate::observe::snapshot(&self.app);
                let captures: Vec<_> = snapshot
                    .source_captures
                    .iter()
                    .map(|capture| {
                        let phase = match capture.phase {
                            crate::source_capture::CapturePhase::Depositing => "depositing",
                            crate::source_capture::CapturePhase::Deposited => "deposited",
                            crate::source_capture::CapturePhase::PendingSave => "pending-save",
                            crate::source_capture::CapturePhase::Failed => "failed",
                            crate::source_capture::CapturePhase::Saved => "saved",
                            crate::source_capture::CapturePhase::Recoverable => "recoverable",
                        };
                        serde_json::json!({
                            "request": capture.request,
                            "ordinal": capture.ordinal,
                            "session": capture.session,
                            "graph": capture.graph,
                            "node": capture.node,
                            "url": capture.url.as_str(),
                            "source": capture.source.as_ref(),
                            "keep": capture.keep,
                            "phase": phase,
                            "error": capture.error.as_deref(),
                            "stored": capture.stored.as_ref(),
                        })
                    })
                    .collect();
                let body = serde_json::to_string_pretty(&captures).map_err(|error| {
                    format!("record-source-captures '{name}': could not serialize: {error}")
                })?;
                let path = self
                    .shared_out_dir
                    .join(format!("{name}.source-captures.json"));
                std::fs::write(&path, body).map_err(|error| {
                    format!(
                        "record-source-captures '{name}': could not write {}: {error}",
                        path.display()
                    )
                })?;
            },
            Step::RecordSavedSources(name) => {
                let selected = self.app.research_search.selected.clone();
                let mut readers = Vec::new();
                let mut nodes: Vec<_> = self.app.saved_readers.keys().copied().collect();
                nodes.sort();
                for node in nodes {
                    let reader = &self.app.saved_readers[&node];
                    if !crate::research_scenario_privacy::capture_selected(
                        &selected,
                        &reader.target.key,
                    ) {
                        continue;
                    }
                    let engine = self
                        .app
                        .content
                        .facts(node)
                        .map(|facts| facts.engine.clone());
                    let node_live = matches!(
                        self.app.content.get(node),
                        Some(crate::content::NodeContent::Live)
                    );
                    let rendered_document = if node_live
                        && engine.as_deref() == Some(crate::saved_sources::ENGINE_ID)
                    {
                        self.content_sessions
                            .session_mut(&node)
                            .and_then(|session| {
                                session
                                    .as_any()
                                    .downcast_mut::<mere_document_lanes::SmolwebDocumentSession>()
                            })
                            .map(|session| session.document_mut().document().clone())
                    } else {
                        None
                    };
                    let phase = if reader.error.is_some() {
                        "Error"
                    } else if reader.request.is_some() || reader.content.is_none() {
                        "Pending"
                    } else if node_live
                        && engine.as_deref() == Some(crate::saved_sources::ENGINE_ID)
                        && rendered_document.is_some()
                    {
                        "Ready"
                    } else {
                        "EngineUnavailable"
                    };
                    readers.push(serde_json::json!({
                        "node": node,
                        "url": reader.url,
                        "target": reader.target,
                        "phase": phase,
                        "error": reader.error,
                        "content": reader.content,
                        "engine": engine,
                        "rendered_document": rendered_document,
                    }));
                }
                let retained_readers = self.scenario_retained_readers(&selected);
                let value = serde_json::json!({
                    "session": self.app.session_id,
                    "generation": self.app.source_capture_generation,
                    "status": self.app.saved_sources.status,
                    "address_events": self.address_event_history.observation(&selected, |key| self.app.saved_source_admitted(key)),
                    "reader_open_events": self.reader_event_history.reader_observation(&selected, |key| self.app.saved_source_admitted(key)),
                    "reader_scope": {"schema":"turnstone.m5-reader-scope/v1",
                        "selected": if selected.is_none() { None::<&str> } else { Some("restricted") }},
                    "admissions": crate::research_scenario_privacy::admissions(&selected, self.app.saved_source_admissions()),
                    "entries": crate::research_scenario_privacy::entries(&selected, &self.app.saved_sources.entries),
                    "readers": readers,
                    "retained_admissions": self.app.research_clip_admissions(),
                    "retained_readers": retained_readers,
                    "search": self.app.scenario_research_search_summary(),
                });
                let body = serde_json::to_string_pretty(&value).map_err(|error| {
                    format!("record-saved-sources '{name}': could not serialize: {error}")
                })?;
                let path = self
                    .shared_out_dir
                    .join(format!("{name}.saved-sources.json"));
                std::fs::write(&path, body).map_err(|error| {
                    format!(
                        "record-saved-sources '{name}': could not write {}: {error}",
                        path.display()
                    )
                })?;
            },
            Step::RecordStructuredResearch(name) => {
                let value = self.app.structured_scenario_observation()?;
                let bytes = serde_json::to_vec_pretty(&value)
                    .map_err(|e| format!("structured observation encoding: {e}"))?;
                let path = self
                    .shared_out_dir
                    .join(format!("{name}.structured-research.json"));
                std::fs::write(path, bytes)
                    .map_err(|e| format!("structured observation write: {e}"))?;
            },
            Step::RecordStructuredStatus(name) => {
                let value = self.app.structured_scenario_status();
                let bytes = serde_json::to_vec_pretty(&value)
                    .map_err(|e| format!("structured status encoding: {e}"))?;
                let path = self
                    .shared_out_dir
                    .join(format!("{name}.structured-status.json"));
                std::fs::write(path, bytes).map_err(|e| format!("structured status write: {e}"))?;
            },
            Step::RecordRuntime(name) => {
                let principal = self
                    .app
                    .identity
                    .as_ref()
                    .and_then(|identity| identity.custody_principal());
                let value = serde_json::json!({
                    "schema": "turnstone.model-free-runtime/v1",
                    "pid": std::process::id(),
                    "session": self.app.session_id,
                    "network_offline": self.network_offline,
                    "offline_acquisition_attempts": self.offline_attempts.load(std::sync::atomic::Ordering::Relaxed),
                    "custody_principal": principal.map(|(profile, public)| serde_json::json!({"profile": profile.0, "public_key": public})),
                    "inference": crate::inference_activity::snapshot(),
                });
                let bytes = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
                std::fs::write(
                    self.shared_out_dir.join(format!("{name}.runtime.json")),
                    bytes,
                )
                .map_err(|e| format!("runtime observation write: {e}"))?;
            },
            Step::RecordComparison(name) => {
                self.scenario_wait_comparison(600)?;
                let value = self.app.research_compare_observation()?;
                let bytes = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
                std::fs::write(
                    self.shared_out_dir.join(format!("{name}.comparison.json")),
                    bytes,
                )
                .map_err(|e| format!("comparison observation write: {e}"))?;
            },
            Step::ChooseCapturedSource { name, ordinal } => {
                let path = self
                    .shared_out_dir
                    .join(format!("{name}.source-captures.json"));
                let bytes = std::fs::read(path)
                    .map_err(|e| format!("capture observation unavailable: {e}"))?;
                let value: serde_json::Value =
                    serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                let label = crate::scenario::observed_capture_manifest(
                    &value,
                    &self.app.session_id.as_uuid().to_string(),
                    *ordinal,
                )?;
                self.scenario_assert_no_surface(&Step::ChooseSuggestion(label))?;
            },
            Step::ChooseSavedResult(name) => {
                let path = self
                    .shared_out_dir
                    .join(format!("{name}.structured-research.json"));
                let bytes = std::fs::read(path)
                    .map_err(|e| format!("saved result observation unavailable: {e}"))?;
                let value: serde_json::Value =
                    serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                let label = crate::scenario::observed_saved_result(
                    &value,
                    &self.app.session_id.as_uuid().to_string(),
                )?;
                self.scenario_assert_no_surface(&Step::ChooseSuggestion(label))?;
            },
            Step::AssertNoAction(label) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.available_actions.iter().any(|action| action == label) {
                    return Err(format!("action unexpectedly offered: {label}"));
                }
            },
            Step::ChooseSuggestion(label) => {
                // Saved Sources scrolls the rendered window rather than moving
                // omnibar.selected. Discover the complete ordered row labels,
                // reject ambiguity, then scroll via the actual keyboard seam.
                if !self.app.omnibar.open
                    || !matches!(self.app.omnibar.mode, crate::ui::OmnibarMode::SavedSources)
                {
                    return Err("saved-source omnibar is not open".into());
                }
                let labels: Vec<_> = self
                    .app
                    .research_search
                    .rows
                    .iter()
                    .map(crate::observe::suggestion_line)
                    .collect();
                if labels.is_empty() || labels.len() > 10_000 {
                    return Err("suggestion list is empty or over limit".into());
                }
                let target = crate::scenario::suggestion_target(&labels, label)?;
                let first = self.app.research_search.first_row;
                if first >= labels.len() {
                    return Err("invalid first suggestion".into());
                }
                let count = (target + labels.len() - first) % labels.len();
                for _ in 0..count {
                    self.scenario_key_in_window(0, &EditKey::Down)?;
                }
                let current = crate::observe::snapshot(&self.app).omnibar;
                let rows: Vec<_> = self
                    .app
                    .research_search
                    .rows
                    .iter()
                    .map(crate::observe::suggestion_line)
                    .collect();
                if rows != labels
                    || self.app.research_search.first_row != target
                    || !current.open
                    || current.selected != 0
                    || current.suggestions.first() != Some(&labels[target])
                {
                    return Err("suggestion changed during keyboard selection".into());
                }
                self.scenario_key_in_window(0, &EditKey::Enter)?;
            },
            Step::TouchFile(path) => {
                let p = std::path::Path::new(path.as_str());
                std::fs::write(p, "touched\n")
                    .map_err(|error| format!("touch '{path}': could not write: {error}"))?;
            },
            _ => unreachable!("dispatch group disagrees with exhaustive routing"),
        }
        Ok(())
    }
}
