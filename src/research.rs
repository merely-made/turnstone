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
#[derive(Clone)]
pub(crate) struct ClipAccess {
    pub root: std::path::PathBuf,
    pub sealer: Arc<dyn eidetic::PayloadSealer + Send + Sync>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Choice {
    pub target: Option<Target>,
    pub label: String,
    pub error: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Context {
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
    context: Option<Context>,
    choices: Vec<Choice>,
    intents: Vec<Intent>,
    clip_access: Option<ClipAccess>,
}
#[derive(Clone, Default)]
pub(crate) struct Bridge(Arc<Mutex<State>>);
impl Bridge {
    pub fn access(&self, access: Option<ClipAccess>) {
        self.0.lock().unwrap().clip_access = access;
    }
    pub fn clip_access(&self) -> Result<ClipAccess, String> {
        self.0
            .lock()
            .unwrap()
            .clip_access
            .clone()
            .ok_or_else(|| crate::identity::PENDING.into())
    }
    pub fn project(&self, context: Context) {
        let mut state = self.0.lock().unwrap();
        if state.context.as_ref() != Some(&context) {
            state.choices.clear();
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
    pub fn choices(&self) -> Vec<Choice> {
        self.0.lock().unwrap().choices.clone()
    }
    pub fn send(&self, intent: Intent) {
        self.0.lock().unwrap().intents.push(intent);
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
    pub fn clip_text(
        &self,
        context: &Context,
        admission: &ClipAdmission,
    ) -> Result<(String, String), String> {
        if admission.session != context.session || !context.clips.contains(admission) {
            return Err("The retained clip was withdrawn from this session.".into());
        }
        let access = self.clip_access()?;
        let view = crate::retained_clip::read_sealed(
            &access.root,
            admission.manifest,
            crate::retained_clip::limits()?,
            &*access.sealer,
        )?;
        let crate::retained_clip::ClipBody::Text(text) = view.body else {
            return Err("This retained clip has no recallable text body.".into());
        };
        if self.context()? != *context {
            return Err("The retained clip session or admissions changed.".into());
        }
        Ok((view.clip.title, text))
    }
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
mod tests {
    use super::*;
    use crate::panes::{GraphId, SessionId};
    fn fixture() -> (Target, RetainedSourceText) {
        let text = "α first 🐦 exact source 東京";
        let target = crate::saved_sources::Target {
            key: crate::saved_sources::Key {
                session: SessionId::new(),
                graph: GraphId::new(),
                node: uuid::Uuid::new_v4(),
                annotation_manifest: eidetic::ManifestId::of_blob(b"A1").to_string(),
            },
            text_hash: eidetic::Hash::of(text.as_bytes()).to_string(),
            selector: None,
        };
        let source = RetainedSourceText {
            source: source_id(&target.clone().into()).unwrap(),
            version: target.text_hash.clone(),
            title: Some("A".into()),
            text: text.into(),
        };
        (target.into(), source)
    }
    #[test]
    fn scope_is_nonempty_exact_and_never_falls_back_to_the_corpus() {
        let (a, source_a) = fixture();
        let mut b = a.clone();
        if let Target::Capture(t) = &mut b {
            t.key.node = uuid::Uuid::new_v4();
        }
        let source_b = RetainedSourceText {
            source: source_id(&b).unwrap(),
            version: source_a.version.clone(),
            title: source_a.title.clone(),
            text: source_a.text.clone(),
        };
        let available = vec![source_a.clone(), source_b];
        let owner = namespace(a.session());
        assert!(scope(&owner, &[], &available).is_err());
        assert_eq!(
            scope(&owner, &[a.clone()], &available).unwrap(),
            vec![source_a.source.clone()]
        );
        assert_eq!(
            scope(&owner, &[a.clone(), b.clone()], &available)
                .unwrap()
                .len(),
            2,
            "equal text in different members remains distinct"
        );
        assert!(scope("turnstone/foreign/research/v1", &[a.clone()], &available).is_err());
        let mut missing = b;
        if let Target::Capture(t) = &mut missing {
            t.key.annotation_manifest = eidetic::ManifestId::of_blob(b"A2").to_string();
        }
        assert!(
            scope(&owner, &[a, missing], &available).is_err(),
            "one invalid explicitly chosen source fails the entire ask"
        );
    }
    #[test]
    fn source_identity_roundtrips_all_owners_and_versions_and_rejects_substitution() {
        let (a, retained) = fixture();
        assert_eq!(target(&retained.source, &retained.version).unwrap(), a);
        let mut wrong = retained.source.clone();
        wrong.node = Some(uuid::Uuid::new_v4().to_string());
        assert!(target(&wrong, &retained.version).is_err());
        let mut wrong = retained.source.clone();
        wrong.owner = Some("foreign".into());
        assert!(target(&wrong, &retained.version).is_err());
        let mut wrong = retained.source.clone();
        wrong.kind = RecallSourceKind::PageText;
        assert!(target(&wrong, &retained.version).is_err());
        assert!(target(&retained.source, &eidetic::Hash::of(b"A2").to_string()).is_err());
        let mut passage = a;
        passage.set_selector(crate::saved_sources::Selector { start: 0, end: 1 });
        assert!(source_id(&passage).is_err());
    }
    #[test]
    fn retained_clip_targets_roundtrip_separately_from_capture_targets() {
        let (capture, _) = fixture();
        let Target::Capture(c) = capture.clone() else {
            unreachable!()
        };
        assert_eq!(
            serde_json::to_value(&capture).unwrap(),
            serde_json::to_value(&c).unwrap()
        );
        let target = Target::Clip(ClipTarget {
            clip: ClipAdmission {
                session: c.key.session,
                graph: c.key.graph,
                node: c.key.node,
                manifest: eidetic::ManifestId::of_blob(b"retained clip"),
            },
            text_hash: c.text_hash,
            selector: None,
        });
        let source = source_id(&target).unwrap();
        assert_eq!(super::target(&source, target.text_hash()).unwrap(), target);
        assert_ne!(source, source_id(&capture).unwrap());
        let serialized = serde_json::to_value(&target).unwrap();
        assert_eq!(
            serde_json::from_value::<Target>(serialized.clone()).unwrap(),
            target
        );
        let mut ambiguous = serde_json::to_value(&capture).unwrap();
        ambiguous["clip"] = serialized["clip"].clone();
        assert!(serde_json::from_value::<Target>(ambiguous).is_err());
        let mut malformed = source.clone();
        malformed.source.push_str("?start=0&end=1");
        assert!(super::target(&malformed, target.text_hash()).is_err());
        malformed = source.clone();
        malformed.node = Some(uuid::Uuid::new_v4().to_string());
        assert!(super::target(&malformed, target.text_hash()).is_err());
        assert!(super::target(&source, &eidetic::Hash::of(b"foreign text").to_string()).is_err());
    }
    #[test]
    fn sealed_knot_and_redshank_bodies_reopen_without_sources_or_private_backing() {
        use crate::retained_clip::{NativeClip, retain_native};
        let root = tempfile::tempdir().unwrap();
        let sealer = Arc::new(pandect::WalletEpochSealer::from_epoch(
            pandect::PersonaId::new(),
            pandect::KeyEpochId::new(),
            b"recall fixture",
        ));
        let source = "PRIVATE PREFIX café → harbor PRIVATE SUFFIX";
        let text = "café → harbor";
        let start = source.find(text).unwrap();
        let (export, backing) = crate::producer_retention::export_knot_selection(
            "mere://knot/document/source",
            source,
            &crate::producer_retention::KnotSelection {
                source_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
                start,
                end: start + text.len(),
            },
        )
        .unwrap();
        let capture = fleece::portable_clip::CaptureContext {
            source: eidetic::clip_profile::ClipSourceV1 {
                owner: None,
                entity: "producer absent".into(),
                revision: None,
            },
            capture_id: "recall".into(),
            provenance: eidetic::ProvenanceRecord::self_imported("recall fixture"),
            trust: eidetic::TrustEnvelope::self_asserted(),
            captured_at: eidetic::Timestamp(1),
        };
        let policy = crate::retained_clip::limits().unwrap();
        let knot = retain_native(
            root.path(),
            NativeClip::Knot {
                export: &export,
                backing: &backing,
            },
            &capture,
            policy,
            &*sealer,
        )
        .unwrap();
        let mut note: serde_json::Value = serde_json::from_str(include_str!(
            "../scenarios/fixtures/retained_clips/redshank.json"
        ))
        .unwrap();
        note["annotation"]["body"] =
            serde_json::json!({"kind":"text", "plain_text":"A retained timed note 🐦"});
        let redshank = retain_native(
            root.path(),
            NativeClip::Redshank {
                export: &note,
                voice_body: None,
            },
            &capture,
            policy,
            &*sealer,
        )
        .unwrap();
        // The producer buffers go away before the receiving owner resolves anything.
        drop(backing);
        drop(export);
        drop(note);
        let session = SessionId::new();
        let clips = [knot, redshank]
            .into_iter()
            .map(|manifest| ClipAdmission {
                session,
                graph: GraphId::new(),
                node: uuid::Uuid::new_v4(),
                manifest,
            })
            .collect::<Vec<_>>();
        let context = Context {
            session,
            generation: uuid::Uuid::new_v4(),
            admissions: vec![],
            clips: clips.clone(),
        };
        let bridge = Bridge::default();
        bridge.project(context.clone());
        bridge.access(Some(ClipAccess {
            root: root.path().into(),
            sealer: sealer.clone(),
        }));
        for _ in 0..2 {
            assert_eq!(bridge.clip_text(&context, &clips[0]).unwrap().1, text);
            assert_eq!(
                bridge.clip_text(&context, &clips[1]).unwrap().1,
                "A retained timed note 🐦"
            );
        }
        let mut foreign = clips[0].clone();
        foreign.node = uuid::Uuid::new_v4();
        assert!(bridge.clip_text(&context, &foreign).is_err());
        let voice_export: serde_json::Value = serde_json::from_str(include_str!(
            "../scenarios/fixtures/retained_clips/redshank.json"
        ))
        .unwrap();
        let voice = retain_native(
            root.path(),
            NativeClip::Redshank {
                export: &voice_export,
                voice_body: Some(include_bytes!(
                    "../scenarios/fixtures/retained_clips/voice.wav"
                )),
            },
            &capture,
            policy,
            &*sealer,
        )
        .unwrap();
        let voice_admission = ClipAdmission {
            manifest: voice,
            ..clips[0].clone()
        };
        let voice_context = Context {
            clips: vec![voice_admission.clone()],
            ..context.clone()
        };
        bridge.project(voice_context.clone());
        assert!(
            bridge
                .clip_text(&voice_context, &voice_admission)
                .unwrap_err()
                .contains("no recallable text")
        );
        bridge.project(context.clone());
        bridge.access(None);
        assert!(bridge.clip_text(&context, &clips[0]).is_err());
        bridge.access(Some(ClipAccess {
            root: root.path().into(),
            sealer,
        }));
        bridge.project(Context {
            clips: vec![],
            ..context.clone()
        });
        assert!(bridge.clip_text(&context, &clips[0]).is_err());
    }
    #[test]
    fn malformed_unicode_reference_is_reported_without_poisoning_the_owner_bridge() {
        let (Target::Capture(target), _) = fixture() else {
            unreachable!()
        };
        let admission = Admission {
            key: crate::saved_sources::Key {
                annotation_manifest: "🦊".repeat(8),
                ..target.key
            },
            occurrence: 0,
            title: "Unreadable reference".into(),
            url: "https://gone.test/invalid".into(),
            error: None,
        };
        let context = Context {
            session: admission.key.session,
            generation: uuid::Uuid::new_v4(),
            admissions: vec![admission.clone()],
            clips: vec![],
        };
        let bridge = Bridge::default();
        bridge.project(context.clone());
        bridge
            .loaded(
                &context,
                &[Entry {
                    admission,
                    content: None,
                    error: Some("malformed manifest identity".into()),
                }],
                vec![],
            )
            .unwrap();
        let choices = bridge.choices();
        assert_eq!(choices.len(), 1);
        assert!(choices[0].target.is_none());
        assert!(choices[0].error.is_some());
        assert!(choices[0].label.contains('🦊'));
        assert_eq!(bridge.context().unwrap(), context);
    }
}
