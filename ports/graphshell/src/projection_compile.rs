// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The bounded, executable half of Graphshell projection authoring.
//!
//! This module deliberately accepts a resolved dataset rather than a product
//! handle. A product adapter reads its own authority and supplies the source
//! binding, revision, typed fields, and source references here. The compiler
//! then checks the saved definition against that disclosure, emits a genuine
//! [`sceno::Score`], and asks `scenomise` to make a scene. It does not infer
//! fields, source meaning, or missing coordinates.

use std::collections::BTreeMap;

use crate::projection_editor::ProjectionDefinition;
pub use scenomise::host_dataset::*;
pub use scenomise::projection::*;

use sceno::Size2;

/// Graphshell's card, in scene units. Paint scales the whole scene to fit the
/// window, so this is the one size the compiler lays out.
pub const PRACTICE_CARD: Size2 = Size2 { w: 164.0, h: 68.0 };

/// Graphshell's one projection compiler: its card, and no custom solvers yet.
pub fn practice_compiler() -> &'static ProjectionCompiler {
    static COMPILER: std::sync::OnceLock<ProjectionCompiler> = std::sync::OnceLock::new();
    COMPILER.get_or_init(|| ProjectionCompiler::new(ItemSizes { card: PRACTICE_CARD }))
}

/// A bounded starting definition for the disclosed practice dataset. These
/// field names are a host recipe, not an inferred product schema.
pub fn default_definition(dataset: &ProjectionDataset) -> ProjectionDefinition {
    use crate::projection_editor::*;
    ProjectionDefinition {
        dynamics: None,
        version: PROJECTION_DEFINITION_VERSION,
        id: "practice-projection".into(),
        label: "Practice Set".into(),
        source: dataset.source.clone(),
        reading: Reading {
            kind: "nodes".into(),
            key: "occurrence_id".into(),
            value: None,
        },
        encoding: Encoding {
            x: Channel::Field("order".into()),
            y: Channel::Field("tempo_bpm".into()),
            color: None,
            label: Some(Channel::Field("label".into())),
        },
        arrangement: crate::projection_editor::Arrangement {
            kind: scenomise::catalog::Family::Grid.id().into(),
            direction: "coordinates".into(),
            spacing: 16,
            options: BTreeMap::new(),
        },
        interaction: Interaction {
            selection: SelectionMode::Single,
            pan: false,
            zoom: false,
        },
        appearance: Appearance {
            realization: "canvas".into(),
            title: "Practice Set".into(),
            theme: "slate".into(),
        },
        provenance: Provenance {
            author: "Graphshell".into(),
            source_revision: Some(dataset.revision.clone()),
            revision_evidence: RevisionEvidence::PublicGeneration,
            note: "Disclosed Woodshed Set; grid ranks numeric fields, scatter uses their values."
                .into(),
        },
    }
}

#[cfg(test)]
const COORDINATES_DIRECTION: &str = "coordinates";
#[cfg(test)]
const NODES_READING_ID: &str = "nodes";
#[cfg(test)]
use crate::projection_editor::{Channel, RevisionEvidence, SourceBinding};
#[cfg(test)]
use sceno::{Footprint, InstanceId, SourceRef, Vec2};
#[cfg(test)]
use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection_editor::{
        Appearance, Arrangement, Encoding, Interaction, ProjectionDefinition, Provenance, Reading,
        SelectionMode,
    };

    #[test]
    fn refresh_reuses_geometry_for_labels_and_new_revisions() {
        let mut data = dataset();
        let mut recipe = definition(GRID_ARRANGEMENT_ID);
        let previous = practice_compiler().compile(&recipe, &data).unwrap();
        data.occurrences[0]
            .values
            .insert("label".into(), ProjectionValue::Text("New caption".into()));
        data.revision = "new-revision".into();
        recipe.provenance.source_revision = Some(data.revision.clone());
        let next = practice_compiler().refresh(&previous, &recipe, &data).unwrap();
        assert!(next.placement_reused);
        assert_eq!(next.scene, practice_compiler().compile(&recipe, &data).unwrap().scene);
        assert_eq!(next.scene.items, previous.scene.items);
        assert!(next.labels.values().any(|label| label == "New caption"));
        assert_ne!(next.scene.generation, previous.scene.generation);
    }

    #[test]
    fn refresh_rejects_invalid_readings_and_resolves_changed_geometry() {
        let mut data = dataset();
        let mut recipe = definition(SCATTER_ARRANGEMENT_ID);
        let previous = practice_compiler().compile(&recipe, &data).unwrap();
        data.occurrences[0]
            .values
            .insert("x".into(), ProjectionValue::Number(42.0));
        let moved = practice_compiler().refresh(&previous, &recipe, &data).unwrap();
        assert!(!moved.placement_reused);
        assert_eq!(moved.scene, practice_compiler().compile(&recipe, &data).unwrap().scene);
        recipe.arrangement.spacing += 1;
        assert!(!practice_compiler().refresh(&moved, &recipe, &data).unwrap().placement_reused);
        data.occurrences[0]
            .values
            .insert("label".into(), ProjectionValue::Number(3.0));
        assert!(practice_compiler().refresh(&moved, &recipe, &data).is_err());
    }

    #[test]
    fn refresh_preserves_occurrence_identity_and_measured_footprints() {
        let mut data = dataset();
        let recipe = definition(GRID_ARRANGEMENT_ID);
        let mut previous = practice_compiler().compile(&recipe, &data).unwrap();
        data.occurrences.reverse();
        assert!(practice_compiler().refresh(&previous, &recipe, &data).unwrap().placement_reused);
        previous.score.items[0].footprint = Footprint::Rect {
            size: Size2::new(300.0, 68.0),
        };
        assert!(!practice_compiler().refresh(&previous, &recipe, &data).unwrap().placement_reused);
        let previous = practice_compiler().compile(&recipe, &data).unwrap();
        data.occurrences[0].occurrence_id = "replacement".into();
        data.occurrences[0].values.insert(
            "occurrence_id".into(),
            ProjectionValue::Text("replacement".into()),
        );
        assert!(!practice_compiler().refresh(&previous, &recipe, &data).unwrap().placement_reused);
    }

    #[test]
    fn exported_woodshed_set_drives_both_arrangements_and_reopens_exact_occurrence() {
        let dataset: ProjectionDataset =
            serde_json::from_str(include_str!("../web/fixtures/woodshed-stage.json")).unwrap();
        let mut definition = default_definition(&dataset);
        let grid = practice_compiler().compile(&definition, &dataset).unwrap();
        assert_eq!(grid.scene.items.len(), 3);
        assert_eq!(grid.scene.sources.len(), 2);
        assert_eq!(grid.scene.items[0].source, grid.scene.items[1].source);
        assert_ne!(
            grid.occurrence_by_instance[&InstanceId(0)],
            grid.occurrence_by_instance[&InstanceId(1)]
        );
        definition.arrangement.kind = SCATTER_ARRANGEMENT_ID.into();
        definition.encoding.label = Some(Channel::Field("catalog_id".into()));
        let snapshot = ProjectionSnapshot {
            definition,
            selected_occurrence: Some("card:2".into()),
        };
        let saved = serde_json::to_vec(&snapshot).unwrap();
        let reopened =
            practice_compiler().compile_snapshot(&serde_json::from_slice(&saved).unwrap(), &dataset).unwrap();
        assert_eq!(reopened.selected, Some(InstanceId(1)));
        assert_eq!(reopened.labels[&InstanceId(1)], "chord:Major");
        assert_ne!(
            grid.scene.items[1].transform,
            reopened.scene.items[1].transform
        );
        assert_eq!(grid.occurrence_by_instance, reopened.occurrence_by_instance);
        let replay = practice_compiler().compile_snapshot(&snapshot, &dataset).unwrap();
        assert_eq!(
            serde_json::to_vec(&reopened.score).unwrap(),
            serde_json::to_vec(&replay.score).unwrap()
        );
        assert_eq!(reopened.scene, replay.scene);
    }

    #[test]
    fn unresolved_registration_missing_field_and_unsupported_realization_refuse_execution() {
        let mut definition = definition(SCATTER_ARRANGEMENT_ID);
        definition.arrangement.kind = "made-up-layout".into();
        definition.encoding.x = Channel::Field("missing".into());
        definition.appearance.realization = "unregistered-renderer".into();
        let issues = practice_compiler().compile(&definition, &dataset()).unwrap_err();
        for field in ["arrangement.kind", "encoding.x", "appearance.realization"] {
            assert!(issues.iter().any(|issue| issue.field == field));
        }
    }

    fn dataset() -> ProjectionDataset {
        let fields = BTreeMap::from([
            ("occurrence_id".into(), ProjectionFieldType::Text),
            ("label".into(), ProjectionFieldType::Text),
            ("x".into(), ProjectionFieldType::Number),
            ("y".into(), ProjectionFieldType::Number),
        ]);
        let source = SourceBinding {
            authority: "woodshed.local".into(),
            domain: "set".into(),
            resource: "set:practice".into(),
        };
        let occurrence = |id: &str, label: &str, x: f64, y: f64| ProjectionOccurrence {
            occurrence_id: id.into(),
            // `bar:1` repeats on purpose: occurrences, rather than sources,
            // are selected and coordinated across views.
            source: SourceRef::new("woodshed.set", "bar:1"),
            values: BTreeMap::from([
                ("occurrence_id".into(), ProjectionValue::Text(id.into())),
                ("label".into(), ProjectionValue::Text(label.into())),
                ("x".into(), ProjectionValue::Number(x)),
                ("y".into(), ProjectionValue::Number(y)),
            ]),
        };
        ProjectionDataset {
            source,
            revision: "woodshed-fixture-v1".into(),
            fields,
            occurrences: vec![
                occurrence("phrase:repeat", "Repeat bar", 2.0, 0.0),
                occurrence("phrase:opening", "Opening bar", 0.0, 1.0),
            ],
        }
    }

    fn definition(kind: &str) -> ProjectionDefinition {
        ProjectionDefinition {
            dynamics: None,
            version: 1,
            id: "woodshed-set".into(),
            label: "Practice set".into(),
            source: dataset().source,
            reading: Reading {
                kind: NODES_READING_ID.into(),
                key: "occurrence_id".into(),
                value: None,
            },
            encoding: Encoding {
                x: Channel::Field("x".into()),
                y: Channel::Field("y".into()),
                color: None,
                label: Some(Channel::Field("label".into())),
            },
            arrangement: Arrangement {
                kind: kind.into(),
                direction: COORDINATES_DIRECTION.into(),
                spacing: 16,
                options: BTreeMap::new(),
            },
            interaction: Interaction {
                selection: SelectionMode::Single,
                pan: false,
                zoom: false,
            },
            appearance: Appearance {
                realization: "canvas".into(),
                title: "Practice set".into(),
                theme: "slate".into(),
            },
            provenance: Provenance {
                author: "fixture".into(),
                source_revision: Some("woodshed-fixture-v1".into()),
                revision_evidence: RevisionEvidence::PublicGeneration,
                note: "real-set-shaped fixture".into(),
            },
        }
    }

    #[test]
    fn scatter_compiles_repeated_sources_to_distinct_instances_in_stable_order() {
        let compiled = practice_compiler().compile(&definition(SCATTER_ARRANGEMENT_ID), &dataset()).expect("compiles");
        assert_eq!(compiled.scene.items.len(), 2);
        assert_eq!(
            compiled.scene.sources.len(),
            1,
            "source is correctly interned"
        );
        assert_eq!(
            compiled.occurrence_by_instance.get(&InstanceId(0)),
            Some(&"phrase:opening".to_owned())
        );
        assert_eq!(
            compiled.labels.get(&InstanceId(1)),
            Some(&"Repeat bar".to_owned())
        );
        assert_eq!(
            compiled.scene.items[1].transform.translate,
            Vec2::new(32.0, 0.0)
        );
    }

    #[test]
    fn grid_uses_dense_numeric_ranks() {
        let compiled = practice_compiler().compile(&definition(GRID_ARRANGEMENT_ID), &dataset()).expect("compiles");
        assert_eq!(
            compiled.scene.items[0].transform.translate,
            Vec2::new(0.0, 84.0)
        );
        // Pitch is the measured card plus spacing (stack seams S16).
        assert_eq!(
            compiled.scene.items[1].transform.translate,
            Vec2::new(180.0, 0.0)
        );
    }

    #[test]
    fn snapshot_round_trips_definition_and_selection() {
        let snapshot = ProjectionSnapshot {
            definition: definition(SCATTER_ARRANGEMENT_ID),
            selected_occurrence: Some("phrase:repeat".into()),
        };
        let bytes = serde_json::to_vec(&snapshot).expect("serializes");
        let reopened: ProjectionSnapshot = serde_json::from_slice(&bytes).expect("deserializes");
        let compiled = practice_compiler().compile_snapshot(&reopened, &dataset()).expect("restores");
        assert_eq!(compiled.selected, Some(InstanceId(1)));
        assert_eq!(
            compiled
                .occurrence_by_instance
                .get(&compiled.selected.expect("selection")),
            Some(&"phrase:repeat".to_owned())
        );
    }

    #[test]
    fn rejects_unresolved_channels_binding_and_stale_selection() {
        let mut definition = definition(SCATTER_ARRANGEMENT_ID);
        definition.encoding.color = Some(Channel::Field("label".into()));
        definition.provenance.source_revision = Some("stale".into());
        let snapshot = ProjectionSnapshot {
            definition,
            selected_occurrence: Some("gone".into()),
        };
        let issues = practice_compiler().compile_snapshot(&snapshot, &dataset()).expect_err("must refuse mismatch");
        assert!(issues.iter().any(|issue| issue.field == "encoding.color"));
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "provenance.source_revision")
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "selected_occurrence")
        );
    }

    #[test]
    fn refuses_an_arrangement_option_the_executable_compiler_does_not_own() {
        let mut definition = definition(GRID_ARRANGEMENT_ID);
        definition
            .arrangement
            .options
            .insert("era_bands".into(), "false".into());
        let issues = practice_compiler().compile(&definition, &dataset()).expect_err("must refuse unknown option");
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "arrangement.options.era_bands")
        );
    }

    #[test]
    fn grid_ranks_fractional_values_without_truncation() {
        let mut dataset = dataset();
        dataset.occurrences[0]
            .values
            .insert("x".into(), ProjectionValue::Number(0.5));
        let compiled = practice_compiler().compile(&definition(GRID_ARRANGEMENT_ID), &dataset).expect("ranked cell");
        assert_eq!(compiled.scene.items[1].transform.translate.x, 180.0);
    }

    #[test]
    fn grid_ranks_unsorted_repeated_and_signed_zero_values_stably() {
        let mut dataset = dataset();
        dataset.occurrences = vec![
            occurrence("c", "C", 4.0, 0.0),
            occurrence("a", "A", -1.0, 2.0),
            occurrence("b", "B", -1.0, -0.0),
            occurrence("d", "D", 2.0, 2.0),
        ];

        let compiled = practice_compiler().compile(&definition(GRID_ARRANGEMENT_ID), &dataset).expect("ranked grid");
        assert_eq!(
            compiled.occurrence_by_instance,
            HashMap::from([
                (InstanceId(0), "a".to_owned()),
                (InstanceId(1), "b".to_owned()),
                (InstanceId(2), "c".to_owned()),
                (InstanceId(3), "d".to_owned()),
            ])
        );
        assert_eq!(
            compiled
                .scene
                .items
                .iter()
                .map(|item| item.transform.translate)
                .collect::<Vec<_>>(),
            vec![
                Vec2::new(0.0, 84.0),
                Vec2::new(0.0, 0.0),
                Vec2::new(360.0, 0.0),
                Vec2::new(180.0, 84.0),
            ]
        );
    }

    fn occurrence(id: &str, label: &str, x: f64, y: f64) -> ProjectionOccurrence {
        ProjectionOccurrence {
            occurrence_id: id.into(),
            source: SourceRef::new("woodshed.set", "bar:1"),
            values: BTreeMap::from([
                ("occurrence_id".into(), ProjectionValue::Text(id.into())),
                ("label".into(), ProjectionValue::Text(label.into())),
                ("x".into(), ProjectionValue::Number(x)),
                ("y".into(), ProjectionValue::Number(y)),
            ]),
        }
    }
}
