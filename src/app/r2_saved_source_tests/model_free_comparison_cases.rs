// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;

#[test]
fn model_free_comparison_side_opens_and_stale_results_are_refused() {
    use crate::action::ResearchSearchAction as Command;
    use eidetic::models::recall::RetainedSourceText;
    use eidetic_search::compare::{ComparisonRelationship, ComparisonSettings, compare};
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let custody = crate::trail_memory::custody_test_fixture::custody();
    let epoch = Arc::new(AtomicU64::new(1));
    let capture_context = fleece::portable_clip::CaptureContext {
        source: eidetic::clip_profile::ClipSourceV1 {
            owner: None,
            entity: "origin absent".into(),
            revision: None,
        },
        capture_id: "comparison".into(),
        provenance: eidetic::ProvenanceRecord::self_imported("fixture"),
        trust: eidetic::TrustEnvelope::self_asserted(),
        captured_at: eidetic::Timestamp(1),
    };
    let mut items = Vec::new();
    for text in ["café old", "café 🐦 new"] {
        let (export, backing) = crate::producer_retention::export_knot_selection(
            "mere://knot/source",
            text,
            &crate::producer_retention::KnotSelection {
                source_hash: blake3::hash(text.as_bytes()).to_hex().to_string(),
                start: 0,
                end: text.len(),
            },
            1_000_000,
        )
        .unwrap();
        let manifest = crate::retained_clip::retain_native(
            root.path(),
            crate::retained_clip::NativeClip::Knot {
                export: &export,
                backing: &backing,
            },
            &capture_context,
            crate::retained_clip::limits().unwrap(),
            &*custody.sealer,
        )
        .unwrap();
        let url = crate::retained_clip::address(manifest);
        let node = visit(&mut app, &url);
        items.push((text.to_owned(), manifest, url, node));
    }
    app.research.set_available(true);
    app.research.access(Some(crate::research::ClipAccess {
        root: root.path().into(),
        sealer: custody.sealer,
        epoch: {
            let epoch = epoch.clone();
            Arc::new(move || Some(epoch.load(Ordering::SeqCst)))
        },
    }));
    app.update(Action::SearchSavedSources);
    let (context, revision) = app.research.context_revision().unwrap();
    let targets: Vec<_> = items
        .iter()
        .map(|(text, manifest, _, node)| {
            crate::research::Target::Clip(crate::research::ClipTarget {
                clip: context
                    .clips
                    .iter()
                    .find(|clip| clip.node == *node && clip.manifest == *manifest)
                    .unwrap()
                    .clone(),
                text_hash: eidetic::Hash::of(text.as_bytes()).to_string(),
                selector: None,
            })
        })
        .collect();
    // The same manifest and text admitted under another node must remain distinguishable.
    let mut other = targets[0].clone();
    let crate::research::Target::Clip(other_clip) = &mut other else {
        unreachable!()
    };
    other_clip.clip.node = uuid::Uuid::new_v4();
    let sources: Vec<_> = [targets[0].clone(), other.clone()]
        .iter()
        .map(|target| eidetic_search::query::ResearchSource {
            retained: RetainedSourceText {
                source: crate::research::source_id(target).unwrap(),
                version: target.text_hash().into(),
                title: Some("Same title".into()),
                text: items[0].0.clone(),
            },
            producer: eidetic_search::query::ProducerKind::Selection,
            address: None,
            date: None,
            tags: vec![],
        })
        .collect();
    let snapshot = eidetic_search::query::ResearchSnapshot {
        generation: "fixture".into(),
        scope: sources.iter().map(|s| s.retained.source.clone()).collect(),
        sources,
        coverage: eidetic_search::query::Coverage::Complete,
    };
    app.research_search.result = Some(
        eidetic_search::query::search(&snapshot, &Default::default(), &Default::default()).unwrap(),
    );
    app.research_search.view = crate::app::research_search_state::View::ChooseLeft;
    app.research_rows(100);
    let labels: Vec<_> = app
        .research_search
        .rows
        .iter()
        .filter_map(|row| match row {
            crate::ui::Suggestion::Act {
                label,
                action: Action::ResearchSearch(Command::ChooseCompare { .. }),
            } => Some(label),
            _ => None,
        })
        .collect();
    assert_eq!(labels.len(), 2);
    for target in [&targets[0], &other] {
        assert!(
            labels
                .iter()
                .any(|label| label.contains(&target.node().to_string())
                    && label.contains(&target.session().as_uuid().to_string()))
        );
    }
    app.research_search.compare_left = Some(targets[0].clone());
    app.research_search.compare_right = Some(other.clone());
    app.research_search.view = crate::app::research_search_state::View::Compare;
    app.research_rows(100);
    for target in [&targets[0], &other] {
        assert!(app.research_search.rows.iter().any(|row| matches!(row, crate::ui::Suggestion::Act { label, .. } if label.contains(&target.node().to_string()))));
    }
    app.research_search.error = Some("Left withdrawn from scope.".into());
    app.research_rows(100);
    assert!(app.research_search.rows.iter().any(|row| matches!(row, crate::ui::Suggestion::Act { label, .. } if label.contains("Left withdrawn") && !label.contains("unavailable"))));
    app.research_search.error = None;
    app.research_search.compare_right = Some(targets[1].clone());
    app.research_search.compare_page = 12;
    app.issue_research_compare();
    assert_eq!(app.research_search.compare_page, 0);
    let texts: Vec<_> = items
        .iter()
        .zip(&targets)
        .map(|((text, _, _, _), target)| RetainedSourceText {
            source: crate::research::source_id(target).unwrap(),
            version: target.text_hash().into(),
            title: None,
            text: text.clone(),
        })
        .collect();
    let settings = ComparisonSettings::default();
    let request = crate::research_compare::Request {
        id: uuid::Uuid::new_v4(),
        context: context.clone(),
        revision,
        epoch: 1,
        left: targets[0].clone(),
        right: targets[1].clone(),
        settings: settings.clone(),
    };
    let view = crate::research_compare::ResultView {
        comparison: compare(
            &texts[0],
            &texts[1],
            ComparisonRelationship::Explicit,
            &settings,
        )
        .unwrap(),
        left: texts[0].clone(),
        right: texts[1].clone(),
    };
    app.research_search.compare_left = Some(targets[0].clone());
    app.research_search.compare_right = Some(targets[1].clone());
    app.research_search.compare_request = Some(request.clone());
    app.research_search.compare_right = Some(targets[0].clone());
    app.research_compare_finished(request.clone(), Ok(view.clone()));
    assert!(app.research_search.comparison.is_none());
    app.research_search.compare_right = Some(targets[1].clone());
    app.research_compare_finished(request.clone(), Ok(view.clone()));
    assert!(app.research_search.comparison.is_some());
    for (side, index) in [(true, 0), (false, 1)] {
        let span = view
            .comparison
            .spans
            .iter()
            .position(|span| {
                if side {
                    span.old.is_some()
                } else {
                    span.new.is_some()
                }
            })
            .unwrap();
        let effects = app.update(Action::ResearchSearch(Command::OpenCompare {
            left: side,
            span: Some(span),
            request: request.id,
        }));
        assert!(
            effects.iter().any(
                |fx| matches!(fx, Effect::SpawnContent { node, .. } if *node == items[index].3)
            )
        );
        let reader = app.research_clip_readers.get(&items[index].3).unwrap();
        assert_eq!(
            reader.target.selector,
            Some(Selector {
                start: 0,
                end: items[index].0.chars().count()
            })
        );
        assert!(
            app.research_clip_document(items[index].3, &items[index].2, items[index].1)
                .unwrap()
                .is_some()
        );
    }
    let refused = app.update(Action::ResearchSearch(Command::OpenCompare {
        left: true,
        span: Some(999),
        request: request.id,
    }));
    assert!(
        !refused
            .iter()
            .any(|fx| matches!(fx, Effect::SpawnContent { .. }))
    );
    let mut stale = request.clone();
    stale.id = uuid::Uuid::new_v4();
    app.research_search.compare_request = Some(stale.clone());
    epoch.store(2, Ordering::SeqCst);
    app.research_compare_finished(stale, Ok(view));
    assert!(app.research_search.compare_request.is_some());
    app.invalidate_research_search();
    assert!(app.research_search.comparison.is_none());
    assert!(
        app.research_search.compare_left.is_none() && app.research_search.compare_right.is_none()
    );
    app.research_search.scope_revision = Some(revision);
    app.research_search.compare_left = Some(targets[0].clone());
    app.research_search.compare_right = Some(targets[1].clone());
    let mut withdrawn = context;
    withdrawn.clips.retain(|clip| clip.node != items[0].3);
    app.research.project(withdrawn);
    app.invalidate_research_search();
    assert!(
        app.research_search.compare_left.is_none() && app.research_search.compare_right.is_none()
    );
}
