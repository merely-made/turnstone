// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! App-owned admission and view intents for capture-backed Answers.
use crate::{
    panes::SessionId,
    saved_sources::{Admission, Entry, Target},
};
use eidetic::models::recall::{RecallSourceId, RecallSourceKind, RetainedSourceText};
use std::sync::{Arc, Mutex};

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
}
#[derive(Clone, Default)]
pub(crate) struct Bridge(Arc<Mutex<State>>);
impl Bridge {
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
    pub fn loaded(&self, context: &Context, entries: &[Entry]) -> Result<(), String> {
        let mut state = self.0.lock().unwrap();
        if state.context.as_ref() != Some(context) {
            return Err("The saved-source session or admissions changed.".into());
        }
        let mut seen = std::collections::HashSet::new();
        state.choices = entries
            .iter()
            .filter(|entry| seen.insert(entry.admission.key.clone()))
            .map(|entry| Choice {
                target: entry.content.as_ref().map(|content| Target {
                    key: entry.admission.key.clone(),
                    text_hash: content.text_hash.clone(),
                    selector: None,
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
        Ok(())
    }
}
pub(crate) fn namespace(session: SessionId) -> String {
    format!("turnstone/{}/research/v1", session.as_uuid())
}
pub(crate) fn source_id(target: &Target) -> Result<RecallSourceId, String> {
    if target.selector.is_some() {
        return Err("An answer source must name the whole saved capture.".into());
    }
    Ok(RecallSourceId {
        kind: RecallSourceKind::Fleece,
        owner: Some(namespace(target.key.session)),
        source: crate::saved_sources::address(target)?,
        node: Some(target.key.node.to_string()),
    })
}
pub(crate) fn target(source: &RecallSourceId, version: &str) -> Result<Target, String> {
    let target = crate::saved_sources::parse_address(&source.source)?
        .ok_or("The citation does not name a saved source.")?;
    if target.selector.is_some() || source_id(&target)? != *source || version != target.text_hash {
        return Err("The citation's saved-source identity or version is invalid.".into());
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
                .any(|entry| entry.source == source && entry.version == choice.text_hash)
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
        let target = Target {
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
            source: source_id(&target).unwrap(),
            version: target.text_hash.clone(),
            title: Some("A".into()),
            text: text.into(),
        };
        (target, source)
    }
    #[test]
    fn scope_is_nonempty_exact_and_never_falls_back_to_the_corpus() {
        let (a, source_a) = fixture();
        let mut b = a.clone();
        b.key.node = uuid::Uuid::new_v4();
        let source_b = RetainedSourceText {
            source: source_id(&b).unwrap(),
            version: source_a.version.clone(),
            title: source_a.title.clone(),
            text: source_a.text.clone(),
        };
        let available = vec![source_a.clone(), source_b];
        let owner = namespace(a.key.session);
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
        missing.key.annotation_manifest = eidetic::ManifestId::of_blob(b"A2").to_string();
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
        passage.selector = Some(crate::saved_sources::Selector { start: 0, end: 1 });
        assert!(source_id(&passage).is_err());
    }
    #[test]
    fn malformed_unicode_reference_is_reported_without_poisoning_the_owner_bridge() {
        let (target, _) = fixture();
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
