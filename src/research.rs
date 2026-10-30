// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! App-owned admission and view intents for capture and retained-text Answers.
use crate::{
    panes::SessionId,
    saved_sources::{Admission, Entry},
};
use eidetic::models::recall::{RecallSourceId, RecallSourceKind, RetainedSourceText};
use std::sync::{Arc, Mutex};

/// Persist identities only. The untagged capture arm preserves existing Answers v2 data.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Target {
    Capture(crate::saved_sources::Target),
    Clip(ClipTarget),
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipTarget {
    pub clip: ClipAdmission,
    pub text_hash: String,
    pub selector: Option<crate::saved_sources::Selector>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipAdmission {
    pub session: SessionId,
    pub graph: crate::panes::GraphId,
    pub node: uuid::Uuid,
    pub manifest: eidetic::ManifestId,
}
impl Target {
    pub fn text_hash(&self) -> &str {
        match self {
            Self::Capture(t) => &t.text_hash,
            Self::Clip(t) => &t.text_hash,
        }
    }
    pub fn selector(&self) -> &Option<crate::saved_sources::Selector> {
        match self {
            Self::Capture(t) => &t.selector,
            Self::Clip(t) => &t.selector,
        }
    }
    pub fn set_selector(&mut self, selector: crate::saved_sources::Selector) {
        match self {
            Self::Capture(t) => t.selector = Some(selector),
            Self::Clip(t) => t.selector = Some(selector),
        }
    }
    pub fn session(&self) -> SessionId {
        match self {
            Self::Capture(t) => t.key.session,
            Self::Clip(t) => t.clip.session,
        }
    }
    pub fn node(&self) -> uuid::Uuid {
        match self {
            Self::Capture(t) => t.key.node,
            Self::Clip(t) => t.clip.node,
        }
    }
    pub fn identity(&self) -> String {
        match self {
            Self::Capture(t) => t.key.annotation_manifest.clone(),
            Self::Clip(t) => t.clip.manifest.to_string(),
        }
    }
}
impl From<crate::saved_sources::Target> for Target {
    fn from(value: crate::saved_sources::Target) -> Self {
        Self::Capture(value)
    }
}
pub(crate) struct ClipReader {
    pub target: ClipTarget,
    pub generation: uuid::Uuid,
    pub identity: Arc<crate::identity::RootIdentity>,
    pub custody_epoch: u64,
}

#[derive(Clone)]
pub(crate) struct ClipAccess {
    pub root: std::path::PathBuf,
    pub sealer: Arc<dyn eidetic::PayloadSealer + Send + Sync>,
    pub epoch: Arc<dyn Fn() -> Option<u64> + Send + Sync>,
}
impl ClipAccess {
    pub(crate) fn from_bound(
        root: std::path::PathBuf,
        bound: Arc<crate::custody_binding::BoundSealer>,
    ) -> Self {
        let epoch_bound = bound.clone();
        Self {
            root,
            sealer: bound,
            epoch: Arc::new(move || epoch_bound.epoch()),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Choice {
    pub target: Option<Target>,
    pub label: String,
    pub error: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub session: SessionId,
    pub generation: uuid::Uuid,
    pub admissions: Vec<Admission>,
    pub clips: Vec<ClipAdmission>,
}
#[derive(Clone)]
pub(crate) enum Intent {
    Selection {
        context: Context,
        expected: crate::inference_surface::AnswerSourceV2,
        chosen: Vec<Target>,
    },
    Citation {
        context: Context,
        target: Target,
    },
}
#[derive(Default)]
struct State {
    available: bool,
    context: Option<Context>,
    revision: u64,
    choices: Vec<Choice>,
    intents: Vec<Intent>,
    clip_access: Option<ClipAccess>,
    storage_limits: Option<Result<crate::research_storage::StorageLimits, String>>,
}
#[derive(Clone, Default)]
pub(crate) struct Bridge(Arc<Mutex<State>>);
impl Bridge {
    pub(crate) fn set_storage_limits(
        &self,
        limits: Result<crate::research_storage::StorageLimits, String>,
    ) {
        self.0.lock().unwrap().storage_limits = Some(limits);
    }
    pub(crate) fn storage_limits(&self) -> Result<crate::research_storage::StorageLimits, String> {
        self.0
            .lock()
            .unwrap()
            .storage_limits
            .clone()
            .unwrap_or_else(|| Ok(Default::default()))
    }
    /// Invalidate session-scoped projections while preserving the live custody
    /// connection. The next session can publish a fresh context after its
    /// admissions have been loaded.
    pub fn invalidate_projection(&self) {
        let mut state = self.0.lock().unwrap();
        state.context = None;
        state.revision = state.revision.saturating_add(1);
        state.choices.clear();
        state.intents.clear();
    }

    pub fn set_available(&self, available: bool) {
        let mut state = self.0.lock().unwrap();
        state.available = available;
        if !available {
            state.revision = state.revision.saturating_add(1);
            state.context = None;
            state.choices.clear();
            state.intents.clear();
            state.clip_access = None;
        }
    }
    pub fn is_available(&self) -> bool {
        self.0.lock().unwrap().available
    }
    pub fn access(&self, access: Option<ClipAccess>) {
        self.0.lock().unwrap().clip_access = access;
    }
    pub fn clip_access(&self) -> Result<ClipAccess, String> {
        let state = self.0.lock().unwrap();
        if !state.available {
            return Err(crate::identity::PENDING.into());
        }
        state
            .clip_access
            .clone()
            .ok_or_else(|| crate::identity::PENDING.into())
    }
    pub fn project(&self, context: Context) {
        let mut state = self.0.lock().unwrap();
        if !state.available {
            return;
        }
        if state.context.as_ref() != Some(&context) {
            state.revision = state.revision.saturating_add(1);
            state.choices.clear();
            state.intents.clear();
            state.context = Some(context);
        }
    }
    pub fn context(&self) -> Result<Context, String> {
        self.0
            .lock()
            .unwrap()
            .context
            .clone()
            .ok_or_else(|| "Saved sources have no active session.".into())
    }
    pub fn context_revision(&self) -> Result<(Context, u64), String> {
        let state = self.0.lock().unwrap();
        let context = state
            .context
            .clone()
            .ok_or("Saved sources have no active session.")?;
        Ok((context, state.revision))
    }
    pub fn choices(&self) -> Vec<Choice> {
        self.0.lock().unwrap().choices.clone()
    }
    pub fn send(&self, intent: Intent) {
        let mut state = self.0.lock().unwrap();
        if state.available {
            state.intents.push(intent);
        }
    }
    pub fn drain(&self) -> Vec<Intent> {
        std::mem::take(&mut self.0.lock().unwrap().intents)
    }
    pub fn loaded(
        &self,
        context: &Context,
        entries: &[Entry],
        clips: Vec<Choice>,
    ) -> Result<(), String> {
        let mut state = self.0.lock().unwrap();
        if state.context.as_ref() != Some(context) {
            return Err("The saved-source session or admissions changed.".into());
        }
        let mut seen = std::collections::HashSet::new();
        state.choices = entries
            .iter()
            .filter(|entry| seen.insert(entry.admission.key.clone()))
            .map(|entry| Choice {
                target: entry.content.as_ref().map(|content| {
                    Target::Capture(crate::saved_sources::Target {
                        key: entry.admission.key.clone(),
                        text_hash: content.text_hash.clone(),
                        selector: None,
                    })
                }),
                label: format!(
                    "{} · saved version {} · {}",
                    entry.admission.title,
                    entry.admission.occurrence + 1,
                    entry
                        .admission
                        .key
                        .annotation_manifest
                        .chars()
                        .take(19)
                        .collect::<String>()
                ),
                error: entry.error.clone(),
            })
            .collect();
        state.choices.extend(clips);
        Ok(())
    }
}
pub(crate) fn admitted(context: &Context, target: &Target) -> bool {
    target.session() == context.session
        && match target {
            Target::Capture(t) => context
                .admissions
                .iter()
                .any(|a| a.key == t.key && a.error.is_none()),
            Target::Clip(t) => context.clips.contains(&t.clip),
        }
}
impl Bridge {
    pub fn clip_view(
        &self,
        context: &Context,
        admission: &ClipAdmission,
    ) -> Result<crate::retained_clip::ClipView, String> {
        let limits = self.storage_limits()?;
        self.clip_view_bounded(context, admission, limits.budget())
            .map_err(|error| error.to_string())
    }
    pub(crate) fn clip_view_bounded(
        &self,
        context: &Context,
        admission: &ClipAdmission,
        budget: crate::research_storage::SharedReadBudget,
    ) -> Result<crate::retained_clip::ClipView, crate::research_storage::SourceReadError> {
        if admission.session != context.session || !context.clips.contains(admission) {
            return Err("The retained clip was withdrawn from this session.".into());
        }
        let access = self.clip_access()?;
        let custody_epoch = (access.epoch)().ok_or_else(|| crate::identity::PENDING.to_string())?;
        let mut view = crate::retained_clip::read_sealed_bounded(
            &access.root,
            admission.manifest,
            crate::retained_clip::limits()?,
            &*access.sealer,
            &self.storage_limits()?,
            budget,
        )?;
        if (access.epoch)() != Some(custody_epoch) {
            if let crate::retained_clip::ClipBody::Text(text) = &mut view.body {
                text.clear();
            }
            return Err("Profile custody changed while reading the retained clip.".into());
        }
        if !matches!(&view.body, crate::retained_clip::ClipBody::Text(_)) {
            return Err("This retained clip has no recallable text body.".into());
        }
        if self.context()? != *context {
            return Err("The retained clip session or admissions changed.".into());
        }
        Ok(view)
    }
    pub fn clip_text(
        &self,
        context: &Context,
        admission: &ClipAdmission,
    ) -> Result<(String, String), String> {
        let view = self.clip_view(context, admission)?;
        let crate::retained_clip::ClipBody::Text(text) = view.body else {
            unreachable!()
        };
        Ok((view.clip.title, text))
    }
}
/// Construct a reader only from the exact sealed text named by the citation.
pub(crate) fn clip_document(
    view: &crate::retained_clip::ClipView,
    target: &ClipTarget,
) -> Result<inker::EngineDocument, String> {
    if view.id != target.clip.manifest {
        return Err("The retained clip manifest changed.".into());
    }
    let crate::retained_clip::ClipBody::Text(text) = &view.body else {
        return Err("The retained clip has no text body.".into());
    };
    if eidetic::Hash::of(text.as_bytes()).to_string() != target.text_hash {
        return Err("The retained clip text version does not match the citation.".into());
    }
    let mut document = view.document();
    if let Some(selector) = &target.selector {
        let chars: Vec<char> = text.chars().collect();
        if selector.start >= selector.end || selector.end > chars.len() {
            return Err("The retained clip selector is outside the retained text.".into());
        }
        let quote: String = chars[selector.start..selector.end].iter().collect();
        document.blocks.insert(
            1,
            inker::Block::Heading {
                level: 2,
                spans: vec![inker::InlineSpan::Text("Selected passage".into())],
            },
        );
        document.blocks.insert(
            2,
            inker::Block::Quote {
                blocks: vec![inker::Block::Paragraph {
                    spans: vec![inker::InlineSpan::Text(quote)],
                }],
            },
        );
    }
    Ok(document)
}

pub(crate) fn namespace(session: SessionId) -> String {
    format!("turnstone/{}/research/v1", session.as_uuid())
}
pub(crate) fn source_id(target: &Target) -> Result<RecallSourceId, String> {
    if target.selector().is_some() {
        return Err("An answer source must name the whole retained text.".into());
    }
    let address = match target {
        Target::Capture(t) => crate::saved_sources::address(t)?,
        Target::Clip(t) => {
            let hash =
                eidetic::Hash::parse(&t.text_hash).map_err(|_| "Invalid retained text hash")?;
            if t.clip.manifest.0.func() != eidetic::Hash::of(b"").func()
                || hash.to_string() != t.text_hash
                || hash.func() != eidetic::Hash::of(b"").func()
            {
                return Err("Invalid retained text hash".into());
            }
            format!(
                "mere://retained-text/{}/{}/{}/{}/{}",
                t.clip.session.as_uuid(),
                t.clip.graph.as_uuid(),
                t.clip.node,
                t.clip.manifest.0.to_hex(),
                hash.to_hex()
            )
        },
    };
    Ok(RecallSourceId {
        kind: RecallSourceKind::Fleece,
        owner: Some(namespace(target.session())),
        source: address,
        node: Some(target.node().to_string()),
    })
}
pub(crate) fn target(source: &RecallSourceId, version: &str) -> Result<Target, String> {
    let target = if source.source.starts_with("mere://retained-text/") {
        // Reuse the strict canonical UUID/hash parser, then recheck the full typed identity.
        let capture = crate::saved_sources::parse_address(&source.source.replacen(
            "mere://retained-text/",
            "mere://saved-source/",
            1,
        ))?
        .ok_or("Invalid retained text address")?;
        Target::Clip(ClipTarget {
            clip: ClipAdmission {
                session: capture.key.session,
                graph: capture.key.graph,
                node: capture.key.node,
                manifest: serde_json::from_value(serde_json::json!(
                    capture.key.annotation_manifest
                ))
                .map_err(|_| "Invalid retained clip manifest")?,
            },
            text_hash: capture.text_hash,
            selector: capture.selector,
        })
    } else {
        Target::Capture(
            crate::saved_sources::parse_address(&source.source)?
                .ok_or("The citation does not name retained text.")?,
        )
    };
    if source_id(&target)? != *source || version != target.text_hash() {
        return Err("The citation's retained-source identity or version is invalid.".into());
    }
    Ok(target)
}
pub(crate) fn scope(
    owner: &str,
    chosen: &[Target],
    available: &[RetainedSourceText],
) -> Result<Vec<RecallSourceId>, String> {
    if chosen.is_empty() {
        return Err("Choose at least one saved source before asking.".into());
    }
    let mut scope = Vec::new();
    for choice in chosen {
        let source = source_id(choice)?;
        if source.owner.as_deref() != Some(owner)
            || !available
                .iter()
                .any(|entry| entry.source == source && entry.version == choice.text_hash())
        {
            return Err("A chosen saved source is stale, unavailable, or belongs to another session. Choose available sources before asking.".into());
        }
        if !scope.contains(&source) {
            scope.push(source);
        }
    }
    Ok(scope)
}

#[cfg(test)]
#[path = "research/tests.rs"]
mod tests;
