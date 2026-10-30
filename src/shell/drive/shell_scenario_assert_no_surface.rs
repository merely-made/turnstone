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
                let mut readers = Vec::new();
                let mut nodes: Vec<_> = self.app.saved_readers.keys().copied().collect();
                nodes.sort();
                for node in nodes {
                    let reader = &self.app.saved_readers[&node];
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
                let mut retained_readers = Vec::new();
                let mut members: Vec<_> = self
                    .app
                    .content
                    .live_nodes()
                    .filter(|node| {
                        self.app
                            .content
                            .facts(*node)
                            .is_some_and(|f| f.engine == crate::retained_clip::ENGINE_ID)
                    })
                    .collect();
                members.sort();
                for node in members {
                    if let Some(session) =
                        self.content_sessions
                            .session_mut(&node)
                            .and_then(|session| {
                                session
                                    .as_any()
                                    .downcast_mut::<mere_document_lanes::SmolwebDocumentSession>()
                            })
                    {
                        retained_readers.push(serde_json::json!({ "node": node, "engine": crate::retained_clip::ENGINE_ID, "rendered_document": session.document_mut().document().clone() }));
                    }
                }
                let value = serde_json::json!({
                    "session": self.app.session_id,
                    "generation": self.app.source_capture_generation,
                    "status": self.app.saved_sources.status,
                    "admissions": self.app.saved_source_admissions(),
                    "entries": self.app.saved_sources.entries,
                    "readers": readers,
                    "retained_admissions": self.app.research_clip_admissions(),
                    "retained_readers": retained_readers,
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
