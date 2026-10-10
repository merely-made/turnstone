// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use scenomise::host_dataset::parse_host_history;

/// The site's published history (merelyllc.com, last revision `9ba3f03`),
/// pinned by its sha256 so a changed export is a deliberate fixture update.
fn site_history() -> HostDatasetV2 {
    use sha2::{Digest, Sha256};
    let bytes = include_bytes!("../web/fixtures/site-repository-host-history.json");
    let digest: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(
        digest,
        "aa3411e8b7eb9d4b145ed895c5330ef3be47c31746a5ac9b61f4f7fe1452289d"
    );
    parse_host_history(std::str::from_utf8(bytes).unwrap()).unwrap()
}

fn title_of(view: &HostDatasetView, occurrence: &str) -> String {
    let (key, _) = view
        .graph
        .get_node_by_url(&format!("urn:host-dataset:{occurrence}"))
        .unwrap();
    view.graph.node_display_label(key)
}

#[test]
fn folded_checkpoints_retain_surviving_choices_positions_and_complete_change_evidence() {
    use crate::host_dataset_view::GroupedHostDataset;
    use mere::kernel::geometry::PortablePoint;
    let history =
        parse_host_history(include_str!("../web/fixtures/grouped-host-history.json")).unwrap();
    let mut grouped =
        GroupedHostDataset::new(revision_envelope(&history, 0).unwrap(), "contains").unwrap();
    grouped
        .state
        .expanded
        .extend(["repo:mere".into(), "workspace:mere:root".into()]);
    grouped.state.entered = Some("workspace:mere:root".into());
    let moved = PortablePoint::new(8000.0, -2000.0);
    grouped.remember_positions([("repo:genet".into(), moved)]);

    let next = grouped
        .at_revision(revision_envelope(&history, 1).unwrap())
        .unwrap();
    assert!(next.state.expanded.contains("repo:mere"));
    assert!(!next.state.expanded.contains("workspace:mere:root"));
    assert_eq!(
        next.state.entered, None,
        "a removed group loses its entry choice"
    );
    let shown = history_view(&history, 1, next.view().unwrap()).unwrap();
    assert_eq!(title_of(&shown.view, "repo:mere"), "~ mere next");
    let key = shown.view.occurrences["repo:genet"];
    assert_eq!(
        shown
            .view
            .positions
            .iter()
            .find(|(id, _)| *id == key)
            .unwrap()
            .1,
        moved
    );
    assert_eq!(
        shown.view.scene.items.len(),
        18,
        "hidden instances stay portable"
    );
    assert_eq!(shown.view.scene.validate_folds(), Ok(()));
    assert_eq!(
        shown.changes.unwrap().occurrences.len(),
        18,
        "change evidence includes hidden members"
    );
    assert!(
        shown
            .view
            .relations
            .iter()
            .all(|relation| !relation.witnesses.is_empty())
    );

    let mut foreign = revision_envelope(&history, 1).unwrap();
    foreign.dataset.source.authority = "https://elsewhere.invalid".into();
    assert!(grouped.at_revision(foreign).is_err());
    assert!(history_view(&history, 1, grouped.view().unwrap()).is_err());
}

#[test]
fn changing_fold_disclosure_reapplies_checkpoint_marks_once_and_keeps_camera() {
    use crate::host_dataset_view::GroupedHostDataset;
    let history =
        parse_host_history(include_str!("../web/fixtures/grouped-host-history.json")).unwrap();
    let mut grouped =
        GroupedHostDataset::new(revision_envelope(&history, 1).unwrap(), "contains").unwrap();
    let changes = history.changes_at(1).unwrap();
    let mut canvas = mere::canvas::Canvas::new();
    canvas.resize(800, 600);
    let viewport = canvas.viewport();
    for expanded in [true, false, true] {
        if expanded {
            grouped.state.expanded.insert("repo:mere".into());
        } else {
            grouped.state.expanded.remove("repo:mere");
        }
        let relations = grouped
            .apply_to_canvas_with(&mut canvas, |view| mark_changes(view, &changes))
            .unwrap();
        let (key, _) = canvas
            .graph()
            .get_node_by_url("urn:host-dataset:repo:mere")
            .unwrap();
        assert_eq!(canvas.graph().node_display_label(key), "~ mere next");
        assert_eq!(canvas.viewport(), viewport);
        assert!(canvas.physics_paused());
        assert_eq!(
            canvas.graph().projected_relations().count(),
            relations.len()
        );
    }
}

#[test]
fn a_v1_envelope_is_one_unmarked_checkpoint() {
    let text = include_str!("../web/fixtures/host-dataset-relations.json");
    let envelope = scenomise::host_dataset::parse_host_dataset(text).unwrap();
    let history = parse_host_history(text).unwrap();
    assert_eq!(history.revisions.len(), 1);
    let shown = host_history_view(&history, 0).unwrap();
    assert!(shown.changes.is_none());
    let direct = host_dataset_view(&envelope).unwrap();
    assert_eq!(shown.view.graph.node_count(), direct.graph.node_count());
    assert_eq!(shown.view.relations, direct.relations);
    for occurrence in &envelope.dataset.occurrences {
        assert_eq!(
            title_of(&shown.view, &occurrence.occurrence_id),
            occurrence.values["label"].text().unwrap(),
            "a lone revision is unmarked"
        );
    }
    assert!(host_history_view(&history, 1).is_err());
}

#[test]
fn every_site_checkpoint_draws_its_revision_with_its_marks() {
    let history = site_history();
    assert_eq!(history.revisions.len(), 15);
    let mut removed_seen = false;
    for (index, entry) in history.revisions.iter().enumerate() {
        let shown = host_history_view(&history, index).unwrap();
        let view = &shown.view;
        assert_eq!(view.revision, entry.revision.as_str());
        assert_eq!(view.graph.node_count(), entry.dataset.occurrences.len());
        assert_eq!(view.relations.len(), entry.relationships.len());
        // Edges as the canvas draws them: Resource relations projected onto
        // their endpoints, not the Surface relations alone.
        assert_eq!(
            view.graph.projected_relations().count(),
            entry.relationships.len()
        );
        let expected = history.changes_at(index).unwrap();
        let changes = shown.changes.as_ref().unwrap();
        assert_eq!(changes.occurrences.len(), expected.occurrences.len());
        assert_eq!(changes.relationships.len(), expected.relationships.len());
        for occurrence in &entry.dataset.occurrences {
            let change = expected.occurrence(&occurrence.occurrence_id).unwrap();
            let label = occurrence.values["label"].text().unwrap();
            assert_eq!(
                title_of(view, &occurrence.occurrence_id),
                format!("{} {label}", change_mark(change))
            );
        }
        if index == 0 {
            assert_eq!(
                changes.count(Change::Added),
                entry.dataset.occurrences.len()
            );
        }
        for removed in changes
            .occurrences
            .iter()
            .filter(|entry| entry.change == Change::Removed)
        {
            removed_seen = true;
            let previous = &history.revisions[index - 1].dataset;
            assert_eq!(removed.label, label_of(previous, &removed.occurrence_id));
            assert!(
                view.graph
                    .get_node_by_url(&format!("urn:host-dataset:{}", removed.occurrence_id))
                    .is_none(),
                "a removed occurrence is told, not drawn"
            );
        }
    }
    assert!(removed_seen, "the fixture exercises removal");
}

#[test]
fn the_site_transitions_read_as_the_sandbox_reads_them() {
    let history = site_history();
    // Checkpoint 5: Graphshell left the graph and Mere was pushed.
    let changes = host_history_view(&history, 4).unwrap().changes.unwrap();
    assert_eq!(changes.summary(), "0 added, 1 updated, 2 stable, 1 removed");
    let spoken = changes.spoken();
    assert!(spoken.contains(&"Graphshell: removed".to_string()));
    assert!(spoken.contains(&"Mere: updated (pushed_at)".to_string()));
    // Checkpoint 14: five relationships appeared; the narrowed comparison
    // (label, class, status, pushed_at) holds the rest stable.
    let changes = host_history_view(&history, 13).unwrap().changes.unwrap();
    assert_eq!(
        changes.summary(),
        "0 added, 8 updated, 13 stable, 0 removed"
    );
    let added = changes
        .relationships
        .iter()
        .filter(|entry| entry.change == Change::Added)
        .count();
    assert!(added > 0);
    assert!(spoken_has_suffix(&changes, ", added"));
}

fn spoken_has_suffix(changes: &ViewedChanges, suffix: &str) -> bool {
    changes.spoken().iter().any(|text| text.ends_with(suffix))
}

#[test]
fn a_removed_relationship_between_remaining_occurrences_is_told() {
    let mut history = site_history();
    let last = history.revisions.len() - 1;
    let mut next = history.revisions[last].clone();
    let dropped = next.relationships.remove(0);
    next.sequence += 1;
    next.revision = "next".into();
    next.dataset.revision = next.revision.clone();
    for relationship in &mut next.relationships {
        relationship.provenance.source_revision = next.revision.clone();
    }
    history.revisions.push(next);
    let changes = host_history_view(&history, last + 1)
        .unwrap()
        .changes
        .unwrap();
    let removed: Vec<_> = changes
        .relationships
        .iter()
        .filter(|entry| entry.change == Change::Removed)
        .collect();
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].id, dropped.id);
    assert_eq!(removed[0].label, dropped.label);
    assert!(removed[0].spoken().ends_with(", removed"));
}

#[test]
fn a_checkpoint_is_one_envelope_and_names_itself() {
    let history = site_history();
    let envelope = revision_envelope(&history, 4).unwrap();
    assert_eq!(envelope.dataset, history.revisions[4].dataset);
    assert_eq!(envelope.revisions.len(), 5);
    assert_eq!(
        envelope.revisions.last().unwrap().revision,
        envelope.dataset.revision
    );
    assert!(revision_envelope(&history, 15).is_none());
    assert_eq!(
        checkpoint_text(&history, 14),
        format!(
            "Checkpoint 15 of 15: {}",
            history.revisions[14].revision.as_str()
        )
    );
    let change = ViewedChange {
        occurrence_id: "genet".into(),
        label: "Genet".into(),
        change: Change::Updated,
        fields: vec!["pushed_at".into()],
        relationships_differ: true,
    };
    assert_eq!(change.spoken(), "Genet: updated (pushed_at, relations)");
}
