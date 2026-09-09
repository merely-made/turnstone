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

use std::collections::{BTreeMap, HashMap, HashSet};

use sceno::{
    Arrangement as ScenoArrangement, Footprint, Geographic, Grid, InstanceId, Placement,
    Representation, Score, ScoreItem, Size2, SourceRef, Vec2,
};
use serde::{Deserialize, Serialize};

use crate::projection_editor::{
    Channel, ProjectionDefinition, PublicSourceRevision, RevisionEvidence, SourceBinding,
};

/// A bounded starting definition for the disclosed practice dataset. These
/// field names are a host recipe, not an inferred product schema.
pub fn default_definition(dataset: &ProjectionDataset) -> ProjectionDefinition {
    use crate::projection_editor::*;
    ProjectionDefinition {
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
            kind: GRID_ARRANGEMENT_ID.into(),
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

/// The two stable arrangement ids Graphshell currently compiles.
pub const GRID_ARRANGEMENT_ID: &str = "grid.default";
pub const SCATTER_ARRANGEMENT_ID: &str = "scatter.default";
const COORDINATES_DIRECTION: &str = "coordinates";
const NODES_READING_ID: &str = "nodes";

/// The type a product disclosed for a field in one resolved dataset.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionFieldType {
    Text,
    Number,
    Boolean,
}

/// One source value, kept small enough to make an adapter's disclosure plain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum ProjectionValue {
    Text(String),
    Number(f64),
    Boolean(bool),
}

impl ProjectionValue {
    fn field_type(&self) -> ProjectionFieldType {
        match self {
            Self::Text(_) => ProjectionFieldType::Text,
            Self::Number(_) => ProjectionFieldType::Number,
            Self::Boolean(_) => ProjectionFieldType::Boolean,
        }
    }

    fn text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            _ => None,
        }
    }

    fn number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }
}

/// One occurrence in a product's resolved reading.
///
/// `occurrence_id` is Graphshell's selection/persistence identity. `source`
/// remains source truth identity, so two occurrences may intentionally name
/// the same source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectionOccurrence {
    pub occurrence_id: String,
    pub source: SourceRef,
    pub values: BTreeMap<String, ProjectionValue>,
}

/// A product-resolved dataset supplied to the compiler without a product
/// dependency or a portable product-data contract.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectionDataset {
    pub source: SourceBinding,
    pub revision: PublicSourceRevision,
    pub fields: BTreeMap<String, ProjectionFieldType>,
    pub occurrences: Vec<ProjectionOccurrence>,
}

/// Field-specific compiler feedback suitable for the authoring host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileIssue {
    pub field: String,
    pub message: String,
}

impl CompileIssue {
    fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

/// The scene and exact reverse mappings a Graphshell realization needs.
#[derive(Debug)]
pub struct CompiledProjection {
    pub score: Score,
    pub scene: sceno::Scene,
    pub labels: HashMap<InstanceId, String>,
    pub occurrence_by_instance: HashMap<InstanceId, String>,
    pub instance_by_occurrence: BTreeMap<String, InstanceId>,
    pub selected: Option<InstanceId>,
    /// True when a validated refresh reused placement from the preceding result.
    pub placement_reused: bool,
}

/// The complete save/reopen payload for this proof. Pan and zoom do not belong
/// here until the host implements their durable view state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectionSnapshot {
    pub definition: ProjectionDefinition,
    pub selected_occurrence: Option<String>,
}

/// Compile one saved definition against the current disclosed dataset.
pub fn compile(
    definition: &ProjectionDefinition,
    dataset: &ProjectionDataset,
) -> Result<CompiledProjection, Vec<CompileIssue>> {
    compile_inner(definition, dataset, None, None)
}

/// Revalidate a reading while retaining solved geometry when all solver inputs
/// are unchanged. Labels and unrelated disclosed values do not require solving
/// this fixed-footprint card representation. Future measured representations
/// must put their measurements in the score before this comparison.
pub fn refresh(
    previous: &CompiledProjection,
    definition: &ProjectionDefinition,
    dataset: &ProjectionDataset,
) -> Result<CompiledProjection, Vec<CompileIssue>> {
    compile_inner(definition, dataset, None, Some(previous))
}

/// Reopen a saved definition and restore selection only when that exact
/// occurrence still exists in the resolved dataset.
pub fn compile_snapshot(
    snapshot: &ProjectionSnapshot,
    dataset: &ProjectionDataset,
) -> Result<CompiledProjection, Vec<CompileIssue>> {
    compile_inner(
        &snapshot.definition,
        dataset,
        snapshot.selected_occurrence.as_deref(),
        None,
    )
}

fn compile_inner(
    definition: &ProjectionDefinition,
    dataset: &ProjectionDataset,
    selected_occurrence: Option<&str>,
    previous: Option<&CompiledProjection>,
) -> Result<CompiledProjection, Vec<CompileIssue>> {
    let mut issues = validation_issues(definition, dataset);
    if let Some(selected) = selected_occurrence
        && !dataset
            .occurrences
            .iter()
            .any(|occurrence| occurrence.occurrence_id == selected)
    {
        issues.push(CompileIssue::new(
            "selected_occurrence",
            "the saved selected occurrence is absent from this dataset revision",
        ));
    }
    if !issues.is_empty() {
        return Err(issues);
    }

    let label_field = definition
        .encoding
        .label
        .as_ref()
        .and_then(|channel| field_name(channel, "encoding.label"))
        .expect("validated label field");
    let x_field = field_name(&definition.encoding.x, "encoding.x").expect("validated x field");
    let y_field = field_name(&definition.encoding.y, "encoding.y").expect("validated y field");
    let arrangement = arrangement_for(definition).expect("validated arrangement");

    let mut occurrences: Vec<_> = dataset.occurrences.iter().collect();
    occurrences.sort_by(|left, right| left.occurrence_id.cmp(&right.occurrence_id));
    let grid_ranks = (definition.arrangement.kind == GRID_ARRANGEMENT_ID).then(|| {
        (
            dense_ranks(&occurrences, x_field),
            dense_ranks(&occurrences, y_field),
        )
    });
    let mut score = Score::new(arrangement);
    score.generation = stable_generation(dataset);
    for (ordinal, occurrence) in occurrences.iter().enumerate() {
        let x = number(occurrence, x_field).expect("validated x value");
        let y = number(occurrence, y_field).expect("validated y value");
        score.items.push(ScoreItem {
            source: occurrence.source.clone(),
            ordinal: ordinal as u32,
            footprint: Footprint::Rect {
                size: Size2::new(164.0, 68.0),
            },
            representation: Representation::Card,
            placement: if let Some((x_ranks, y_ranks)) = &grid_ranks {
                // Dense numeric ranks preserve the two declared encodings
                // without allocating ninety-six empty rows for a 96 bpm card.
                Placement::Cell {
                    column: x_ranks[&dense_rank_key(x)],
                    row: y_ranks[&dense_rank_key(y)],
                }
            } else {
                placement_for(definition, x, y).expect("validated placement")
            },
            layer: 0,
            visible: true,
            axis: None,
            embedding: None,
            weight: None,
        });
    }
    // Compare exact solver inputs, not revision hashes: source revisions can
    // change without affecting placement, and unchanged revisions cannot excuse
    // changed data. Ordered occurrence identity also protects repeated sources.
    let reusable = previous.filter(|old| {
        old.score.version == score.version
            && old.score.arrangement == score.arrangement
            && old.score.items == score.items
            && old.score.holds == score.holds
            && occurrences.iter().enumerate().all(|(index, occurrence)| {
                old.occurrence_by_instance.get(&InstanceId(index as u32))
                    == Some(&occurrence.occurrence_id)
            })
    });
    let placement_reused = reusable.is_some();
    let scene = if let Some(old) = reusable {
        let mut scene = old.scene.clone();
        scene.generation = score.generation;
        scene
    } else {
        scenomise::solve(&score)
    };
    let mut labels = HashMap::new();
    let mut occurrence_by_instance = HashMap::new();
    let mut instance_by_occurrence = BTreeMap::new();
    for (index, occurrence) in occurrences.into_iter().enumerate() {
        let instance = InstanceId(index as u32);
        labels.insert(
            instance,
            occurrence
                .values
                .get(label_field)
                .and_then(ProjectionValue::text)
                .expect("validated label value")
                .to_owned(),
        );
        occurrence_by_instance.insert(instance, occurrence.occurrence_id.clone());
        instance_by_occurrence.insert(occurrence.occurrence_id.clone(), instance);
    }
    let selected = selected_occurrence.and_then(|id| instance_by_occurrence.get(id).copied());
    Ok(CompiledProjection {
        score,
        scene,
        labels,
        occurrence_by_instance,
        instance_by_occurrence,
        selected,
        placement_reused,
    })
}

/// Builds the same dense ranks as the former per-occurrence closure, once for
/// each grid channel. Finite values have one `==` equivalence class per bit
/// pattern except signed zero, which deliberately shares a rank.
fn dense_ranks(occurrences: &[&ProjectionOccurrence], field: &str) -> BTreeMap<u64, i32> {
    let mut values: Vec<_> = occurrences
        .iter()
        .map(|occurrence| number(occurrence, field).expect("validated numeric value"))
        .collect();
    values.sort_by(f64::total_cmp);
    values.dedup();
    values
        .into_iter()
        .enumerate()
        .map(|(rank, value)| (dense_rank_key(value), rank as i32))
        .collect()
}

fn dense_rank_key(value: f64) -> u64 {
    if value == 0.0 { 0 } else { value.to_bits() }
}

fn validation_issues(
    definition: &ProjectionDefinition,
    dataset: &ProjectionDataset,
) -> Vec<CompileIssue> {
    let mut issues: Vec<_> = definition
        .clone()
        .to_json_bytes()
        .ok()
        .and_then(|_| {
            // Definitions are normally promoted through ProjectionDraft. Keep
            // the compiler independently defensive for persisted input.
            let draft = crate::projection_editor::ProjectionDraft {
                version: definition.version,
                id: definition.id.clone(),
                label: definition.label.clone(),
                source: definition.source.clone(),
                reading: definition.reading.clone(),
                encoding: definition.encoding.clone(),
                arrangement: definition.arrangement.clone(),
                interaction: definition.interaction.clone(),
                appearance: definition.appearance.clone(),
                provenance: definition.provenance.clone(),
            };
            draft.validate().err()
        })
        .unwrap_or_default()
        .into_iter()
        .map(|issue| CompileIssue::new(issue.field, issue.message))
        .collect();

    source_matches(&mut issues, definition, dataset);
    if definition.reading.value.is_some() {
        issues.push(CompileIssue::new(
            "reading.value",
            "nodes does not accept a value aggregation",
        ));
    }
    if definition.interaction.selection != crate::projection_editor::SelectionMode::Single
        || definition.interaction.pan
        || definition.interaction.zoom
    {
        issues.push(CompileIssue::new(
            "interaction",
            "this realization supports single selection with a fitted viewport",
        ));
    }
    if definition.appearance.realization != "canvas" {
        issues.push(CompileIssue::new(
            "appearance.realization",
            "supported realization is canvas",
        ));
    }
    if definition.appearance.theme != "slate" {
        issues.push(CompileIssue::new(
            "appearance.theme",
            "supported theme is slate",
        ));
    }
    if definition.reading.kind != NODES_READING_ID {
        issues.push(CompileIssue::new(
            "reading.kind",
            "only the nodes reading is executable in this compiler",
        ));
    }
    required_field(
        &mut issues,
        dataset,
        "reading.key",
        &definition.reading.key,
        ProjectionFieldType::Text,
    );
    required_channel_field(
        &mut issues,
        dataset,
        "encoding.x",
        &definition.encoding.x,
        ProjectionFieldType::Number,
    );
    required_channel_field(
        &mut issues,
        dataset,
        "encoding.y",
        &definition.encoding.y,
        ProjectionFieldType::Number,
    );
    let Some(label) = definition.encoding.label.as_ref() else {
        issues.push(CompileIssue::new(
            "encoding.label",
            "an executable projection needs a text label field",
        ));
        validate_arrangement(&mut issues, definition);
        validate_occurrences(&mut issues, definition, dataset);
        return issues;
    };
    required_channel_field(
        &mut issues,
        dataset,
        "encoding.label",
        label,
        ProjectionFieldType::Text,
    );
    if definition.encoding.color.is_some() {
        issues.push(CompileIssue::new(
            "encoding.color",
            "color encoding is not implemented by this compiler",
        ));
    }
    validate_arrangement(&mut issues, definition);
    validate_occurrences(&mut issues, definition, dataset);
    issues
}

fn source_matches(
    issues: &mut Vec<CompileIssue>,
    definition: &ProjectionDefinition,
    dataset: &ProjectionDataset,
) {
    for (field, expected, actual) in [
        (
            "source.authority",
            &definition.source.authority,
            &dataset.source.authority,
        ),
        (
            "source.domain",
            &definition.source.domain,
            &dataset.source.domain,
        ),
        (
            "source.resource",
            &definition.source.resource,
            &dataset.source.resource,
        ),
    ] {
        if expected != actual {
            issues.push(CompileIssue::new(
                field,
                "definition does not match the resolved dataset",
            ));
        }
    }
    if definition.provenance.source_revision.as_ref() != Some(&dataset.revision) {
        issues.push(CompileIssue::new(
            "provenance.source_revision",
            "definition does not match the resolved dataset",
        ));
    }
}

fn required_field(
    issues: &mut Vec<CompileIssue>,
    dataset: &ProjectionDataset,
    target: &str,
    field: &str,
    expected: ProjectionFieldType,
) {
    match dataset.fields.get(field) {
        Some(actual) if *actual == expected => {},
        Some(_) => issues.push(CompileIssue::new(target, "field has an incompatible type")),
        None => issues.push(CompileIssue::new(
            target,
            "field is not disclosed by this dataset",
        )),
    }
}

fn required_channel_field(
    issues: &mut Vec<CompileIssue>,
    dataset: &ProjectionDataset,
    target: &str,
    channel: &Channel,
    expected: ProjectionFieldType,
) {
    match channel {
        Channel::Field(field) => required_field(issues, dataset, target, field, expected),
        Channel::Constant(_) => issues.push(CompileIssue::new(
            target,
            "constants are not executable for this channel",
        )),
    }
}

fn validate_arrangement(issues: &mut Vec<CompileIssue>, definition: &ProjectionDefinition) {
    match definition.arrangement.kind.as_str() {
        GRID_ARRANGEMENT_ID | SCATTER_ARRANGEMENT_ID => {},
        _ => issues.push(CompileIssue::new(
            "arrangement.kind",
            "supported arrangements are grid.default and scatter.default",
        )),
    }
    if definition.arrangement.direction != COORDINATES_DIRECTION {
        issues.push(CompileIssue::new(
            "arrangement.direction",
            "this compiler requires the coordinates direction",
        ));
    }
    if !definition.arrangement.options.is_empty() {
        issues.push(CompileIssue::new(
            "arrangement.options",
            "this compiler does not support arrangement options",
        ));
    }
}

fn validate_occurrences(
    issues: &mut Vec<CompileIssue>,
    definition: &ProjectionDefinition,
    dataset: &ProjectionDataset,
) {
    let mut seen = HashSet::new();
    let label = definition
        .encoding
        .label
        .as_ref()
        .and_then(|channel| field_name(channel, ""));
    let x = field_name(&definition.encoding.x, "");
    let y = field_name(&definition.encoding.y, "");
    for occurrence in &dataset.occurrences {
        let prefix = format!("occurrences.{}", occurrence.occurrence_id);
        if occurrence.occurrence_id.trim().is_empty() || !seen.insert(&occurrence.occurrence_id) {
            issues.push(CompileIssue::new(
                format!("{prefix}.occurrence_id"),
                "occurrence ids must be non-empty and unique",
            ));
        }
        if occurrence.source.adapter.trim().is_empty() || occurrence.source.id.trim().is_empty() {
            issues.push(CompileIssue::new(
                format!("{prefix}.source"),
                "each occurrence needs an exact source reference",
            ));
        }
        for (name, value) in &occurrence.values {
            match dataset.fields.get(name) {
                Some(field_type) if *field_type == value.field_type() => {},
                Some(_) => issues.push(CompileIssue::new(
                    format!("{prefix}.values.{name}"),
                    "value does not match the disclosed field type",
                )),
                None => issues.push(CompileIssue::new(
                    format!("{prefix}.values.{name}"),
                    "value names a field absent from the disclosed schema",
                )),
            }
        }
        if occurrence
            .values
            .get(&definition.reading.key)
            .and_then(ProjectionValue::text)
            != Some(occurrence.occurrence_id.as_str())
        {
            issues.push(CompileIssue::new(
                format!("{prefix}.values.{}", definition.reading.key),
                "the reading key must repeat this occurrence id exactly",
            ));
        }
        for (target, field, type_name) in [
            ("encoding.label", label, "text"),
            ("encoding.x", x, "number"),
            ("encoding.y", y, "number"),
        ] {
            let Some(field) = field else {
                continue;
            };
            let value = occurrence.values.get(field);
            let valid = match type_name {
                "text" => value.and_then(ProjectionValue::text).is_some(),
                "number" => value
                    .and_then(ProjectionValue::number)
                    .is_some_and(f64::is_finite),
                _ => false,
            };
            if !valid {
                issues.push(CompileIssue::new(
                    format!("{prefix}.values.{field}"),
                    format!("{target} needs a finite {type_name} value"),
                ));
            }
        }
        if definition.arrangement.kind == SCATTER_ARRANGEMENT_ID {
            for field in [x, y].into_iter().flatten() {
                if let Some(value) = occurrence
                    .values
                    .get(field)
                    .and_then(ProjectionValue::number)
                    && (!value.is_finite()
                        || value.abs() * definition.arrangement.spacing as f64 > 1.0e9)
                {
                    issues.push(CompileIssue::new(
                        format!("{prefix}.values.{field}"),
                        "scatter coordinates must fit finite scene units",
                    ));
                }
            }
        }
    }
}

fn field_name<'a>(channel: &'a Channel, _target: &str) -> Option<&'a str> {
    match channel {
        Channel::Field(field) if !field.trim().is_empty() => Some(field),
        Channel::Field(_) | Channel::Constant(_) => None,
    }
}

fn number(occurrence: &ProjectionOccurrence, field: &str) -> Option<f64> {
    occurrence
        .values
        .get(field)
        .and_then(ProjectionValue::number)
}

fn arrangement_for(definition: &ProjectionDefinition) -> Option<ScenoArrangement> {
    let spacing = definition.arrangement.spacing as f32;
    match definition.arrangement.kind.as_str() {
        GRID_ARRANGEMENT_ID => Some(ScenoArrangement::Grid(Grid {
            origin: Vec2::ZERO,
            cell: Vec2::new(184.0, 84.0),
            columns: 8,
            gap: spacing,
        })),
        SCATTER_ARRANGEMENT_ID => Some(ScenoArrangement::Geographic(Geographic {
            origin: Vec2::ZERO,
            units_per_coordinate: spacing,
            invert_y: false,
        })),
        _ => None,
    }
}

fn placement_for(definition: &ProjectionDefinition, x: f64, y: f64) -> Option<Placement> {
    match definition.arrangement.kind.as_str() {
        GRID_ARRANGEMENT_ID => Some(Placement::Cell {
            column: x as i32,
            row: y as i32,
        }),
        SCATTER_ARRANGEMENT_ID => Some(Placement::Coordinate(Vec2::new(x as f32, y as f32))),
        _ => None,
    }
}

fn stable_generation(dataset: &ProjectionDataset) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for byte in dataset.revision.bytes().chain([0]) {
        value ^= u64::from(byte);
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mut ids: Vec<_> = dataset
        .occurrences
        .iter()
        .map(|occurrence| occurrence.occurrence_id.as_bytes())
        .collect();
    ids.sort();
    for id in ids {
        for byte in id.iter().copied().chain([0]) {
            value ^= u64::from(byte);
            value = value.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    value
}

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
        let previous = compile(&recipe, &data).unwrap();
        data.occurrences[0]
            .values
            .insert("label".into(), ProjectionValue::Text("New caption".into()));
        data.revision = "new-revision".into();
        recipe.provenance.source_revision = Some(data.revision.clone());
        let next = refresh(&previous, &recipe, &data).unwrap();
        assert!(next.placement_reused);
        assert_eq!(next.scene, compile(&recipe, &data).unwrap().scene);
        assert_eq!(next.scene.items, previous.scene.items);
        assert!(next.labels.values().any(|label| label == "New caption"));
        assert_ne!(next.scene.generation, previous.scene.generation);
    }

    #[test]
    fn refresh_rejects_invalid_readings_and_resolves_changed_geometry() {
        let mut data = dataset();
        let mut recipe = definition(SCATTER_ARRANGEMENT_ID);
        let previous = compile(&recipe, &data).unwrap();
        data.occurrences[0]
            .values
            .insert("x".into(), ProjectionValue::Number(42.0));
        let moved = refresh(&previous, &recipe, &data).unwrap();
        assert!(!moved.placement_reused);
        assert_eq!(moved.scene, compile(&recipe, &data).unwrap().scene);
        recipe.arrangement.spacing += 1;
        assert!(!refresh(&moved, &recipe, &data).unwrap().placement_reused);
        data.occurrences[0]
            .values
            .insert("label".into(), ProjectionValue::Number(3.0));
        assert!(refresh(&moved, &recipe, &data).is_err());
    }

    #[test]
    fn refresh_preserves_occurrence_identity_and_measured_footprints() {
        let mut data = dataset();
        let recipe = definition(GRID_ARRANGEMENT_ID);
        let mut previous = compile(&recipe, &data).unwrap();
        data.occurrences.reverse();
        assert!(refresh(&previous, &recipe, &data).unwrap().placement_reused);
        previous.score.items[0].footprint = Footprint::Rect {
            size: Size2::new(300.0, 68.0),
        };
        assert!(!refresh(&previous, &recipe, &data).unwrap().placement_reused);
        let previous = compile(&recipe, &data).unwrap();
        data.occurrences[0].occurrence_id = "replacement".into();
        data.occurrences[0].values.insert(
            "occurrence_id".into(),
            ProjectionValue::Text("replacement".into()),
        );
        assert!(!refresh(&previous, &recipe, &data).unwrap().placement_reused);
    }

    #[test]
    fn exported_woodshed_set_drives_both_arrangements_and_reopens_exact_occurrence() {
        let dataset: ProjectionDataset =
            serde_json::from_str(include_str!("../web/fixtures/woodshed-stage.json")).unwrap();
        let mut definition = default_definition(&dataset);
        let grid = compile(&definition, &dataset).unwrap();
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
            compile_snapshot(&serde_json::from_slice(&saved).unwrap(), &dataset).unwrap();
        assert_eq!(reopened.selected, Some(InstanceId(1)));
        assert_eq!(reopened.labels[&InstanceId(1)], "chord:Major");
        assert_ne!(
            grid.scene.items[1].transform,
            reopened.scene.items[1].transform
        );
        assert_eq!(grid.occurrence_by_instance, reopened.occurrence_by_instance);
        let replay = compile_snapshot(&snapshot, &dataset).unwrap();
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
        let issues = compile(&definition, &dataset()).unwrap_err();
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
        let compiled = compile(&definition(SCATTER_ARRANGEMENT_ID), &dataset()).expect("compiles");
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
        let compiled = compile(&definition(GRID_ARRANGEMENT_ID), &dataset()).expect("compiles");
        assert_eq!(
            compiled.scene.items[0].transform.translate,
            Vec2::new(0.0, 100.0)
        );
        assert_eq!(
            compiled.scene.items[1].transform.translate,
            Vec2::new(200.0, 0.0)
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
        let compiled = compile_snapshot(&reopened, &dataset()).expect("restores");
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
        let issues = compile_snapshot(&snapshot, &dataset()).expect_err("must refuse mismatch");
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
        let issues = compile(&definition, &dataset()).expect_err("must refuse unknown option");
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "arrangement.options")
        );
    }

    #[test]
    fn grid_ranks_fractional_values_without_truncation() {
        let mut dataset = dataset();
        dataset.occurrences[0]
            .values
            .insert("x".into(), ProjectionValue::Number(0.5));
        let compiled = compile(&definition(GRID_ARRANGEMENT_ID), &dataset).expect("ranked cell");
        assert_eq!(compiled.scene.items[1].transform.translate.x, 200.0);
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

        let compiled = compile(&definition(GRID_ARRANGEMENT_ID), &dataset).expect("ranked grid");
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
                Vec2::new(0.0, 100.0),
                Vec2::new(0.0, 0.0),
                Vec2::new(400.0, 0.0),
                Vec2::new(200.0, 100.0),
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
