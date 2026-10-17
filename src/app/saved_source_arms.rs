// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Current-session facet admission and immutable saved-reader intent.

use super::App;
use crate::action::{Effect, Update};
use crate::saved_sources::{self, Admission, Content, Entry, Key, Target};
use crate::ui::{OmnibarMode, OmnibarState, Suggestion};

#[derive(Default)]
pub(crate) struct SavedSourcesState {
    pub request: Option<uuid::Uuid>,
    pub admissions: Vec<Admission>,
    pub entries: Vec<Entry>,
    pub status: String,
}

pub(crate) struct SavedReader {
    pub request: Option<uuid::Uuid>,
    pub url: String,
    pub target: Target,
    pub content: Option<Content>,
    pub error: Option<String>,
}

impl App {
    /// Admission follows every live graph owned by this session. Store inventory,
    /// recovery receipts, and a cursor on another graph are never corpus authority.
    pub(crate) fn saved_source_admissions(&self) -> Vec<Admission> {
        if self.session_load_refused() {
            return Vec::new();
        }
        let mut runtimes: Vec<_> = self
            .graph_runtimes
            .iter()
            .filter(|runtime| runtime.session == Some(self.session_id))
            .collect();
        runtimes.sort_by_key(|runtime| runtime.graph.as_uuid());
        let mut admissions = Vec::new();
        for runtime in runtimes {
            let placement_error = runtime
                .persistence
                .placement_for(&self.session_dir())
                .err()
                .map(|error| error.to_string());
            let mut nodes: Vec<_> = runtime
                .canvas
                .graph()
                .nodes()
                .map(|(_, node)| node)
                .collect();
            nodes.sort_by_key(|node| node.id);
            for node in nodes {
                let Some(value) = runtime.canvas.facets().get(
                    &node.id,
                    &chartulary::FacetId::new(
                        crate::source_capture::SOURCE_CAPTURE_REFERENCES_FACET,
                    ),
                ) else {
                    continue;
                };
                let values: Vec<_> = match value.as_array() {
                    Some(values) => values
                        .iter()
                        .enumerate()
                        .map(|(i, v)| (i, v, None))
                        .collect(),
                    None => vec![(
                        0,
                        value,
                        Some("The capture reference series is unreadable".to_string()),
                    )],
                };
                for (occurrence, value, malformed) in values {
                    let annotation_manifest = value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string());
                    let error = placement_error.clone().or(malformed).or_else(|| {
                        (!value.is_string())
                            .then(|| "The capture reference is not a manifest identity".into())
                    });
                    admissions.push(Admission {
                        key: Key {
                            session: self.session_id,
                            graph: runtime.graph,
                            node: node.id,
                            annotation_manifest,
                        },
                        occurrence,
                        title: node.title.clone(),
                        url: node.url().to_owned(),
                        error,
                    });
                }
            }
        }
        admissions
    }

    pub(crate) fn saved_source_admitted(&self, key: &Key) -> bool {
        key.session == self.session_id
            && self
                .saved_source_admissions()
                .iter()
                .any(|admission| &admission.key == key && admission.error.is_none())
    }

    pub(super) fn saved_source_refusal(&mut self, error: String) -> Vec<Effect> {
        self.events
            .push(crate::observe::AppEvent::AffordanceUnavailable {
                what: "saved-source",
                target: error,
            });
        vec![Effect::Redraw]
    }

    pub(crate) fn search_saved_sources(&mut self) -> Vec<Effect> {
        let mut effects = self.cancel_smolweb_conversation();
        self.shell.begin_omnibar(self.fallback_shell_context());
        self.recall.clear();
        self.recall_query.clear();
        self.omnibar = OmnibarState {
            open: true,
            mode: OmnibarMode::SavedSources,
            ..OmnibarState::default()
        };
        self.focus = crate::surface::FocusTarget::Chrome;
        let admissions = self.saved_source_admissions();
        let request = uuid::Uuid::new_v4();
        self.saved_sources = SavedSourcesState {
            request: Some(request),
            admissions: admissions.clone(),
            entries: Vec::new(),
            status: "Loading".into(),
        };
        self.recompute_omnibar_suggestions();
        effects.push(Effect::LoadSavedSources {
            generation: self.source_capture_generation,
            request,
            session: self.session_id,
            admissions,
        });
        effects.push(Effect::Redraw);
        effects
    }

    pub(crate) fn recompute_saved_source_suggestions(&mut self, row_limit: usize) {
        let query = self.omnibar.text.trim();
        let admissions = self.saved_source_admissions();
        let current: Vec<_> = self
            .saved_sources
            .entries
            .iter()
            .filter(|entry| {
                admissions.iter().any(|admission| {
                    admission.key == entry.admission.key && admission.error.is_none()
                })
            })
            .cloned()
            .collect();
        let hits = saved_sources::search(&current, query, row_limit.max(1));
        let mut rows = Vec::new();
        for hit in hits {
            let manifest = &hit.target.key.annotation_manifest;
            let short = manifest.strip_prefix("blake3:").unwrap_or(manifest);
            rows.push(Suggestion::Act {
                label: format!(
                    "Read saved source: {} · {} · {}",
                    hit.title,
                    crate::place::utc_ms(hit.captured_at_ms),
                    &short[..short.len().min(8)]
                ),
                action: crate::action::Action::OpenSavedSource(hit.target),
            });
        }
        for entry in &self.saved_sources.entries {
            if let Some(error) = &entry.error
                && (query.is_empty()
                    || entry.admission.title.contains(query)
                    || entry.admission.key.annotation_manifest.contains(query))
                && admissions.iter().any(|a| a.key == entry.admission.key)
            {
                rows.push(Suggestion::Prompt(format!(
                    "Unavailable saved source: {} · {} · {}",
                    entry.admission.title, entry.admission.key.annotation_manifest, error
                )));
            }
        }
        if rows.is_empty() {
            rows.push(Suggestion::Prompt(
                match self.saved_sources.status.as_str() {
                    "Loading" => "Loading saved sources".into(),
                    "Ready" if query.is_empty() => "No saved sources in this session".into(),
                    "Ready" => "No saved sources match".into(),
                    status if !status.is_empty() => status.to_owned(),
                    _ => "Search saved sources to load this session's evidence".into(),
                },
            ));
        }
        rows.truncate(row_limit.max(1));
        self.omnibar.suggestions = rows;
        self.omnibar.selected = self
            .omnibar
            .suggestions
            .iter()
            .position(|row| matches!(row, Suggestion::Act { .. }))
            .unwrap_or(0);
    }

    pub(crate) fn saved_sources_loaded(
        &mut self,
        request: uuid::Uuid,
        session: crate::panes::SessionId,
        admissions: Vec<Admission>,
        result: Result<Vec<Entry>, String>,
    ) -> Vec<Effect> {
        if session != self.session_id || self.saved_sources.request != Some(request) {
            return Vec::new();
        }
        self.saved_sources.request = None;
        if admissions != self.saved_sources.admissions
            || admissions != self.saved_source_admissions()
        {
            self.saved_sources.entries.clear();
            self.saved_sources.status = "Saved source references changed; search again".into();
        } else {
            match result {
                Ok(entries)
                    if entries
                        .iter()
                        .map(|entry| &entry.admission)
                        .eq(admissions.iter()) =>
                {
                    self.saved_sources.entries = entries;
                    self.saved_sources.status = "Ready".into();
                },
                Ok(_) => {
                    self.saved_sources.entries.clear();
                    self.saved_sources.status =
                        "Saved source response does not match its admission".into();
                },
                Err(error) => {
                    self.saved_sources.entries.clear();
                    self.saved_sources.status = format!("Unavailable: {error}");
                },
            }
        }
        if self.omnibar.open && self.omnibar.mode == OmnibarMode::SavedSources {
            self.recompute_omnibar_suggestions();
        }
        vec![Effect::Redraw]
    }

    pub(crate) fn open_saved_source(&mut self, target: Target) -> Vec<Effect> {
        if !self.saved_source_admitted(&target.key) {
            return self.saved_source_refusal(
                "This saved source is not attached in the current session".into(),
            );
        }
        if let Some(content) = self
            .saved_sources
            .entries
            .iter()
            .find(|entry| entry.admission.key == target.key)
            .and_then(|entry| entry.content.as_ref())
            && let Err(error) = saved_sources::validate_selector(content, &target)
        {
            return self.saved_source_refusal(error);
        }
        match saved_sources::address(&target) {
            Ok(url) => self.open_address(url),
            Err(error) => self.saved_source_refusal(error),
        }
    }

    pub(crate) fn held_saved_source_document(
        &self,
        node: uuid::Uuid,
        url: &str,
        target: &Target,
    ) -> Result<Option<inker::EngineDocument>, String> {
        let current_view = self
            .graph_runtimes
            .graph_containing_member(node)
            .and_then(|graph| self.graph_runtimes.get(graph))
            .is_some_and(|runtime| {
                runtime.session == Some(self.session_id)
                    && crate::browse::still_current(&runtime.canvas, node, url)
            });
        if !current_view || !self.saved_source_admitted(&target.key) {
            return Err(
                "This saved source or reader view is no longer attached in the current session"
                    .into(),
            );
        }
        let Some(reader) = self.saved_readers.get(&node) else {
            return Ok(None);
        };
        if reader.url != url || &reader.target != target {
            return Ok(None);
        }
        match &reader.content {
            Some(content) => saved_sources::document(content, target).map(Some),
            None => Ok(None),
        }
    }

    pub(crate) fn request_saved_source_read(
        &mut self,
        node: uuid::Uuid,
        url: String,
        target: Target,
    ) -> Vec<Effect> {
        if !self
            .graph_runtimes
            .graph_containing_member(node)
            .and_then(|graph| self.graph_runtimes.get(graph))
            .filter(|runtime| runtime.session == Some(self.session_id))
            .map(|runtime| &runtime.canvas)
            .is_some_and(|canvas| crate::browse::still_current(canvas, node, &url))
        {
            return Vec::new();
        }
        let Some(admission) = self
            .saved_source_admissions()
            .into_iter()
            .find(|admission| admission.key == target.key && admission.error.is_none())
        else {
            return self.apply_update(Update::ContentFailed {
                node,
                error: "This saved source is not attached in the current session".into(),
            });
        };
        if self.saved_readers.get(&node).is_some_and(|reader| {
            reader.url == url && reader.target == target && reader.request.is_some()
        }) {
            return Vec::new();
        }
        let request = uuid::Uuid::new_v4();
        self.saved_readers.insert(
            node,
            SavedReader {
                request: Some(request),
                url: url.clone(),
                target: target.clone(),
                content: None,
                error: None,
            },
        );
        vec![Effect::ReadSavedSource {
            generation: self.source_capture_generation,
            request,
            node,
            url,
            target,
            admission,
        }]
    }

    pub(crate) fn saved_source_read(
        &mut self,
        request: uuid::Uuid,
        node: uuid::Uuid,
        url: String,
        target: Target,
        result: Result<Content, String>,
    ) -> Vec<Effect> {
        let current = self
            .graph_runtimes
            .graph_containing_member(node)
            .and_then(|graph| self.graph_runtimes.get(graph))
            .filter(|runtime| runtime.session == Some(self.session_id))
            .map(|runtime| &runtime.canvas)
            .is_some_and(|canvas| crate::browse::still_current(canvas, node, &url));
        if !self.saved_readers.get(&node).is_some_and(|reader| {
            reader.request == Some(request) && reader.url == url && reader.target == target
        }) {
            return Vec::new();
        }
        if !current {
            // A response that lost its view may not leave a pending fence
            // behind: returning through history must be able to read again.
            self.saved_readers.remove(&node);
            return Vec::new();
        }
        let result = result.and_then(|content| {
            if !self.saved_source_admitted(&target.key) {
                return Err("Saved source attachment changed before reading completed".into());
            }
            saved_sources::validate_selector(&content, &target)?;
            Ok(content)
        });
        let reader = self.saved_readers.get_mut(&node).unwrap();
        reader.request = None;
        match result {
            Ok(content) => {
                let title = format!("Saved: {}", content.title);
                reader.content = Some(content);
                reader.error = None;
                let mut effects = self.set_content_title(node, title);
                effects.push(Effect::SpawnContent { node, url });
                effects
            },
            Err(error) => {
                reader.content = None;
                reader.error = Some(error.clone());
                self.apply_update(Update::ContentFailed { node, error })
            },
        }
    }

    pub(crate) fn invalidate_saved_readers(&mut self) -> Vec<Effect> {
        let invalid: Vec<_> = self
            .saved_readers
            .iter()
            .filter(|(_, reader)| {
                reader.content.is_some() && !self.saved_source_admitted(&reader.target.key)
            })
            .map(|(node, _)| *node)
            .collect();
        let mut effects = Vec::new();
        for node in invalid {
            let reader = self.saved_readers.get_mut(&node).unwrap();
            reader.content = None;
            reader.error = Some("Saved source attachment is no longer available".into());
            effects.push(Effect::CloseContent { node });
            self.content.note_failed(
                node,
                "Saved source attachment is no longer available".into(),
            );
        }
        effects
    }
}
