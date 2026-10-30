// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use apparatus_core::{RecordRef, RetentionLimits, RunId, SourceBootId};
use cambium::reactive_diagnostics::{
    CauseRelation, DiagnosticDetails, DiagnosticKind, ProducerRef, ReactiveDiagnosticEvent, Subject,
};
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use std::collections::BTreeMap;
use tracing_subscriber::{
    filter::LevelFilter,
    layer::{Layer, SubscriberExt},
};

const MAX_TEST_RECORDS: usize = 16_384;
const MAX_TEST_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone)]
struct TraceRecord {
    record: RecordRef,
    payload: crate::reactive_trace::ReactiveTracePayload,
}

/// Producer-gap assertions need a process where no other test thread evaluates
/// views: Meristem's no_std trace sequence is process-wide, while a test's
/// subscriber sees only its own thread. Returns true in the isolated rerun.
fn in_own_process(test: &str) -> bool {
    const CHILD: &str = "TURNSTONE_REACTIVE_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        return true;
    }
    let module = module_path!().split_once("::").map_or("", |(_, path)| path);
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &format!("{module}::{test}"), "--test-threads=1"])
        .env(CHILD, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

fn limits() -> RetentionLimits {
    RetentionLimits {
        max_records: MAX_TEST_RECORDS,
        max_bytes: MAX_TEST_BYTES,
        max_age: std::time::Duration::from_secs(300),
    }
}

fn trace_records(
    batch: &apparatus_core::Batch<crate::diagnostic_observations::EventCategory>,
) -> Vec<TraceRecord> {
    for payload in batch
        .records
        .iter()
        .filter_map(|r| r.payload.reactive.as_ref())
    {
        use crate::reactive_trace::{MeristemReactiveEvent, ReactiveDiagnosticPayload};
        assert!(
            !matches!(
                &payload.event,
                ReactiveDiagnosticPayload::Cambium(
                    ReactiveDiagnosticEvent::SequenceExhausted { .. }
                        | ReactiveDiagnosticEvent::ProducerIdUnavailable { .. }
                ) | ReactiveDiagnosticPayload::Meristem(MeristemReactiveEvent::CoverageGap { .. })
            ),
            "negative assertions require available producer coverage"
        );
    }
    batch
        .records
        .iter()
        .filter_map(|record| {
            Some(TraceRecord {
                record: record.envelope.reference.clone(),
                payload: record.payload.reactive.clone()?,
            })
        })
        .collect()
}

fn decision(
    record: &TraceRecord,
) -> Option<(
    ProducerRef,
    DiagnosticKind,
    Option<Subject>,
    DiagnosticDetails,
)> {
    match &record.payload.event {
        crate::reactive_trace::ReactiveDiagnosticPayload::Cambium(
            ReactiveDiagnosticEvent::Decision {
                reference,
                kind,
                subject,
                details,
                ..
            },
        ) => Some((*reference, *kind, *subject, *details)),
        _ => None,
    }
}

fn decision_ref(record: &TraceRecord) -> Option<ProducerRef> {
    crate::reactive_trace::producer_ref(&record.payload.event)
}

fn resolution(
    records: &[TraceRecord],
    child: ProducerRef,
    parent: ProducerRef,
    relation: CauseRelation,
) -> Option<(RecordRef, RecordRef)> {
    records.iter().find_map(|record| {
        let crate::reactive_trace::ReactiveDiagnosticPayload::CauseResolution {
            child: resolved_child,
            parent: resolved_parent,
            relation: resolved_relation,
            child_record,
            parent_record,
            ..
        } = &record.payload.event
        else {
            return None;
        };
        (*resolved_child == child && *resolved_parent == parent && *resolved_relation == relation)
            .then(|| (child_record.clone(), parent_record.clone()))
    })
}

fn assert_resolved_record_refs(records: &[TraceRecord]) {
    let refs = records
        .iter()
        .filter_map(|record| Some((decision_ref(record)?, record.record.clone())))
        .collect::<BTreeMap<_, _>>();
    for record in records {
        if let crate::reactive_trace::ReactiveDiagnosticPayload::CauseResolution {
            child,
            parent,
            child_record,
            parent_record,
            ..
        } = &record.payload.event
        {
            assert_eq!(refs.get(child), Some(child_record));
            assert_eq!(refs.get(parent), Some(parent_record));
        }
    }
}

fn exact_control(pane: &SettingsPane, class: &str, setting_id: &str) -> genet_scripted_dom::NodeId {
    let dom = pane.dom_ref();
    dom.all_with_class(dom.document(), class)
        .into_iter()
        .find(|node| {
            let mut ancestor = Some(*node);
            while let Some(candidate) = ancestor {
                if dom.attribute(
                    candidate,
                    &Namespace::default(),
                    &LocalName::from("data-setting"),
                ) == Some(setting_id)
                {
                    return true;
                }
                ancestor = dom.parent(candidate);
            }
            false
        })
        .unwrap_or_else(|| panic!("missing {class} for {setting_id}"))
}

fn dispatch(pane: &mut SettingsPane, class: &str, setting_id: &str) {
    let node = exact_control(pane, class, setting_id);
    let _: Vec<()> = pane
        .runner
        .dispatch_click(node, cambium::PointerClick::at((1.0, 1.0)));
    pane.flush_editor_commands();
}

fn component_subject(pane: &SettingsPane, setting_id: &str) -> Subject {
    let key = composition::component_key(setting_id);
    let route = pane
        .runner
        .state()
        .composition_lifecycle
        .route(&key)
        .expect("component route exists");
    let queue = pane
        .runner
        .state()
        .input_queue
        .as_ref()
        .expect("composition input queue exists");
    Subject {
        owner_id: queue.owner_id(),
        slot: route.route.slot,
        route_generation: route.route.route_generation,
        placement_generation: route.route.placement_generation,
    }
}

fn find_decision(
    records: &[TraceRecord],
    kind: DiagnosticKind,
    subject: Option<Subject>,
) -> (ProducerRef, DiagnosticDetails) {
    records
        .iter()
        .filter_map(decision)
        .find_map(|(reference, found, found_subject, details)| {
            (found == kind && found_subject == subject).then_some((reference, details))
        })
        .unwrap_or_else(|| panic!("missing {kind:?} decision for {subject:?}"))
}

fn find_linked_decision(
    records: &[TraceRecord],
    kind: DiagnosticKind,
    subject: Option<Subject>,
    parent: ProducerRef,
    relation: CauseRelation,
) -> ProducerRef {
    records
        .iter()
        .filter_map(|record| {
            let (reference, found, found_subject, _) = decision(record)?;
            (found == kind
                && found_subject == subject
                && resolution(records, reference, parent, relation).is_some())
            .then_some(reference)
        })
        .next()
        .unwrap_or_else(|| panic!("missing {kind:?} decision linked to {parent:?} by {relation:?}"))
}

fn baseline_reactive_stats(
    observations: &mut crate::diagnostic_observations::DiagnosticObservations,
) -> crate::diagnostic_observations::ReactiveObservationStats {
    observations.reactive_stats()
}

fn assert_no_new_coverage_loss(
    before: &crate::diagnostic_observations::ReactiveObservationStats,
    after: &crate::diagnostic_observations::ReactiveObservationStats,
) {
    assert_eq!(after.ingress_full_loss, before.ingress_full_loss);
    assert_eq!(after.ingress_disabled_loss, before.ingress_disabled_loss);
    assert_eq!(after.ingress_other_loss, before.ingress_other_loss);
    assert_eq!(after.malformed_rejected, before.malformed_rejected);
    assert_eq!(after.store_rejected, before.store_rejected);
    assert_eq!(after.join_evicted, before.join_evicted);
    assert_eq!(after.pending_edge_dropped, before.pending_edge_dropped);
    assert_eq!(after.causes_omitted, before.causes_omitted);
    assert_eq!(
        after.producer_gap_candidates,
        before.producer_gap_candidates
    );
    assert_eq!(
        after.producer_gap_late_fills,
        before.producer_gap_late_fills
    );
    assert_eq!(
        after.producer_tracker_evicted,
        before.producer_tracker_evicted
    );
    assert_eq!(
        after.producer_missing_ranges,
        before.producer_missing_ranges
    );
    assert_eq!(after.renderer_coverage_gaps, before.renderer_coverage_gaps);
}

#[test]
fn installed_trace_links_owner_input_flush_mutation_livery_and_submitted_frame() {
    if !in_own_process("installed_trace_links_owner_input_flush_mutation_livery_and_submitted_frame") {
        return;
    }
    let run = RunId::from("settings-reactive-receiving");
    let boot = SourceBootId::from("settings-reactive-boot");
    let (layer, receiver) = crate::reactive_trace::channel(run.clone(), boot.clone());
    let subscriber = tracing_subscriber::registry().with(layer.with_filter(LevelFilter::TRACE));

    tracing::subscriber::with_default(subscriber, || {
        let mut observations =
            crate::diagnostic_observations::DiagnosticObservations::new(run.clone(), limits());
        observations.attach_reactive(receiver, &boot).unwrap();
        let mut cursor = observations.cursor();
        let directory = tempfile::tempdir().unwrap();
        let mut pane = SettingsPane::new(directory.path().to_path_buf());
        pane.sync(640.0, 780.0);
        let _initial_scene = pane.scene(640, 780, 1.0);
        let startup = observations.read(&mut cursor, MAX_TEST_RECORDS).unwrap();
        let mut records = trace_records(&startup);
        let before = baseline_reactive_stats(&mut observations);
        let before_store = observations.stats().unwrap();
        let submitted_subject = component_subject(&pane, "chrome.shellbar.visible");

        let frame = crate::reactive_render::FrameScope::begin(0);
        dispatch(&mut pane, "toggle", "chrome.shellbar.visible");
        dispatch(&mut pane, "setting-apply", "chrome.shellbar.visible");
        assert!(
            pane.settings().shellbar_hidden,
            "actual Apply persists the toggle"
        );
        pane.sync(640.0, 780.0);
        let _scene = pane.scene(640, 780, 1.0);
        frame.submitted(1);
        drop(frame);

        let interval = observations.read(&mut cursor, MAX_TEST_RECORDS).unwrap();
        records.extend(trace_records(&interval));
        let after = observations.reactive_stats();
        let after_store = observations.stats().unwrap();
        assert_no_new_coverage_loss(&before, &after);
        assert_eq!(after_store.loss, before_store.loss, "store reports no loss");
        assert_resolved_record_refs(&records);

        let (requested, _) = find_decision(&records, DiagnosticKind::SettingsRequested, None);
        let (committed, _) = find_decision(&records, DiagnosticKind::SettingsCommitted, None);
        let (revision_advanced, _) =
            find_decision(&records, DiagnosticKind::RevisionAdvanced, None);
        assert!(resolution(&records, committed, requested, CauseRelation::CommittedBy,).is_some());
        assert!(
            resolution(
                &records,
                revision_advanced,
                committed,
                CauseRelation::CommittedBy,
            )
            .is_some()
        );

        let invalidated = find_linked_decision(
            &records,
            DiagnosticKind::CompositionInputInvalidated,
            None,
            revision_advanced,
            CauseRelation::TriggeredBy,
        );
        let captured = find_linked_decision(
            &records,
            DiagnosticKind::CompositionInputCaptured,
            None,
            invalidated,
            CauseRelation::CapturedFrom,
        );
        let evaluated = find_linked_decision(
            &records,
            DiagnosticKind::ComponentInputEvaluated,
            Some(submitted_subject),
            captured,
            CauseRelation::CapturedFrom,
        );
        let operation = find_linked_decision(
            &records,
            DiagnosticKind::ComponentOperationStarted,
            Some(submitted_subject),
            captured,
            CauseRelation::CapturedFrom,
        );
        assert!(resolution(&records, evaluated, operation, CauseRelation::ProducedBy).is_some());
        assert!(
            records.iter().any(|record| {
                matches!(
                    record.payload.event,
                    crate::reactive_trace::ReactiveDiagnosticPayload::Meristem(
                        crate::reactive_trace::MeristemReactiveEvent::Decision { .. }
                    )
                ) && record.payload.span_context.is_some_and(|span| {
                    span.subject == Some(submitted_subject) && span.operation == Some(operation)
                }) && record.payload.scope_cause == Some(operation)
            }),
            "actual core decisions retain the evaluated component and operation"
        );
        let flush = find_linked_decision(
            &records,
            DiagnosticKind::CompositionFlushCompleted,
            None,
            evaluated,
            CauseRelation::EvaluatedFrom,
        );
        let flush_details = records
            .iter()
            .filter_map(decision)
            .find_map(|(reference, kind, _, details)| {
                (reference == flush && kind == DiagnosticKind::CompositionFlushCompleted)
                    .then_some(details)
            })
            .expect("selected flush details");
        assert!(
            flush_details
                .mutation_cursor_start
                .zip(flush_details.mutation_cursor_end)
                .is_some_and(|(start, end)| end > start)
        );

        let mutation_batch = find_linked_decision(
            &records,
            DiagnosticKind::MutationBatch,
            None,
            flush,
            CauseRelation::ProducedBy,
        );
        let mutation_details = records
            .iter()
            .filter_map(decision)
            .find_map(|(reference, kind, _, details)| {
                (reference == mutation_batch && kind == DiagnosticKind::MutationBatch)
                    .then_some(details)
            })
            .expect("selected mutation batch details");
        assert_eq!(
            mutation_details.mutation_cursor_start,
            flush_details.mutation_cursor_start
        );
        assert_eq!(
            mutation_details.mutation_cursor_end,
            flush_details.mutation_cursor_end
        );

        let styled = find_linked_decision(
            &records,
            DiagnosticKind::RendererStyleCompleted,
            None,
            mutation_batch,
            CauseRelation::RenderedFrom,
        );
        let laid_out = find_linked_decision(
            &records,
            DiagnosticKind::RendererLayoutCompleted,
            None,
            styled,
            CauseRelation::ProducedBy,
        );
        let painted = find_linked_decision(
            &records,
            DiagnosticKind::RendererPaintCompleted,
            None,
            laid_out,
            CauseRelation::ProducedBy,
        );
        let submitted = find_linked_decision(
            &records,
            DiagnosticKind::HostFrameSubmitted,
            None,
            painted,
            CauseRelation::ScopeContains,
        );

        let serialized = serde_json::to_string(
            &records
                .iter()
                .map(|record| &record.payload)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert!(!serialized.contains(directory.path().to_string_lossy().as_ref()));
        assert!(!serialized.contains("chrome.shellbar.visible"));
        assert!(!serialized.contains("Show shellbar"));
    });
}

#[test]
fn unrelated_behavior_revision_skips_the_unchanged_shell_row() {
    if !in_own_process("unrelated_behavior_revision_skips_the_unchanged_shell_row") {
        return;
    }
    let run = RunId::from("settings-reactive-skip-receiving");
    let boot = SourceBootId::from("settings-reactive-skip-boot");
    let (layer, receiver) = crate::reactive_trace::channel(run.clone(), boot.clone());
    let subscriber = tracing_subscriber::registry().with(layer.with_filter(LevelFilter::TRACE));

    tracing::subscriber::with_default(subscriber, || {
        let mut observations =
            crate::diagnostic_observations::DiagnosticObservations::new(run, limits());
        observations.attach_reactive(receiver, &boot).unwrap();
        let mut cursor = observations.cursor();
        let directory = tempfile::tempdir().unwrap();
        let mut pane = SettingsPane::new(directory.path().to_path_buf());
        pane.sync(640.0, 780.0);
        let _initial_scene = pane.scene(640, 780, 1.0);
        let startup = observations.read(&mut cursor, MAX_TEST_RECORDS).unwrap();
        let _ = trace_records(&startup);
        let before = baseline_reactive_stats(&mut observations);
        let before_store = observations.stats().unwrap();
        let unchanged_shell_subject = component_subject(&pane, "chrome.shellbar.visible");

        let frame = crate::reactive_render::FrameScope::begin(0);
        pane.runner.update(|state| {
            apply_value(state, "behaviors.cascade_budget", SettingValue::Number(9.0));
        });
        pane.sync(640.0, 780.0);
        let _scene = pane.scene(640, 780, 1.0);
        frame.submitted(2);
        drop(frame);

        let interval = observations.read(&mut cursor, MAX_TEST_RECORDS).unwrap();
        let records = trace_records(&interval);
        let after = observations.reactive_stats();
        let after_store = observations.stats().unwrap();
        assert_no_new_coverage_loss(&before, &after);
        assert_eq!(after_store.loss, before_store.loss);

        let (skipped, _) = find_decision(
            &records,
            DiagnosticKind::ComponentInputSkipped,
            Some(unchanged_shell_subject),
        );
        assert!(
            !records
                .iter()
                .filter_map(decision)
                .any(|(reference, kind, subject, _)| {
                    reference != skipped
                        && subject == Some(unchanged_shell_subject)
                        && matches!(
                            kind,
                            DiagnosticKind::ComponentInputEvaluated
                                | DiagnosticKind::ComponentRebuildCompleted
                        )
                })
        );
    });
}

#[test]
fn disconnected_trace_sink_does_not_change_an_actual_apply_result() {
    let run = RunId::from("settings-reactive-disconnected-receiving");
    let boot = SourceBootId::from("settings-reactive-disconnected-boot");
    let (layer, receiver) = crate::reactive_trace::channel(run, boot);
    drop(receiver);
    let subscriber = tracing_subscriber::registry().with(layer.with_filter(LevelFilter::TRACE));

    tracing::subscriber::with_default(subscriber, || {
        let directory = tempfile::tempdir().unwrap();
        let mut pane = SettingsPane::new(directory.path().to_path_buf());
        pane.sync(640.0, 780.0);
        let _scene = pane.scene(640, 780, 1.0);
        dispatch(&mut pane, "toggle", "chrome.shellbar.visible");
        dispatch(&mut pane, "setting-apply", "chrome.shellbar.visible");
        assert!(pane.settings().shellbar_hidden);
        assert!(
            pandect::load_application_settings(directory.path())
                .unwrap()
                .unwrap()
                .shellbar_hidden
        );
    });
}
