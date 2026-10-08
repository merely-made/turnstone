// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Bounded parsing for the named Woodshed musical-comparison disclosure.
//!
//! This is a product adapter, not an authentication claim. The fixed source
//! tuple identifies the disclosure shape used by this practice proof; it does
//! not establish who supplied the bytes or grant authority over Woodshed.
//!
//! The practice page receives the comparison as a host dataset
//! (`scenomise::host_dataset`, mer3ly site canvas plan, Rulings 124 and 129):
//! [`comparison_host_dataset`] carries the Woodshed evidence in ordinary
//! typed fields of the projection dataset, and [`comparison_from_host_dataset`]
//! rebuilds the record from them and checks it again. The comparison stays
//! an occurrence of its own, as it was before the envelope.

use std::collections::{BTreeMap, BTreeSet};

use scenomise::host_dataset::{HOST_DATASET_SCHEMA_V1, HostDatasetRevision, HostDatasetV1};
use serde_json::{Value, json};

use crate::practice_workspace::ComparisonRecord;
use crate::projection_compile::{
    ProjectionDataset, ProjectionFieldType, ProjectionOccurrence, ProjectionValue,
};
use crate::projection_editor::SourceBinding;

/// Maximum accepted UTF-8 input size before JSON parsing.
pub const MAX_WOODSHED_DISCLOSURE_BYTES: usize = 256 * 1024;

const AUTHORITY: &str = "woodshed";
const DOMAIN: &str = "music.practice";
const RESOURCE: &str = "set:musical-comparison";
const METHOD: &str = "keyed-pitch-set-singleton";
const METHOD_VERSION: u32 = 1;
const MAX_PITCHES: usize = 12;
const MAX_TONE_LABEL_CHARS: usize = 24;
const MAX_SUBJECT_LABEL_CHARS: usize = 100;

/// Parse one bounded, self-consistent Woodshed comparison disclosure.
///
/// The returned record retains the original product evidence. Validation only
/// rejects disclosures whose reported partitions or singleton motion disagree
/// with their literal supplied pitch sets.
pub fn parse_woodshed_comparison(json: &str) -> Result<ComparisonRecord, String> {
    if json.len() > MAX_WOODSHED_DISCLOSURE_BYTES {
        return Err("Woodshed disclosure exceeds the 256 KiB input limit".into());
    }
    let record: ComparisonRecord = serde_json::from_str(json)
        .map_err(|error| format!("Invalid Woodshed disclosure: {error}"))?;
    if record.source.authority != AUTHORITY
        || record.source.domain != DOMAIN
        || record.source.resource != RESOURCE
    {
        return Err("Disclosure does not name the supported Woodshed practice source".into());
    }
    if record.method.id != METHOD || record.method.version != METHOD_VERSION {
        return Err("Disclosure does not use the supported Woodshed comparison method".into());
    }
    if result_field(&record, "status")?.as_str() != Some("applicable") {
        return Err("Woodshed comparison is not an applicable result".into());
    }
    if result_field(&record, "version")?.as_u64() != Some(1) {
        return Err("Woodshed comparison result version must be 1".into());
    }

    validate_subject_label("left.label", &record.left.label)?;
    validate_subject_label("right.label", &record.right.label)?;
    let left = pitch_set("left.pitch_set", &record.left.disclosed)?;
    let right = pitch_set("right.pitch_set", &record.right.disclosed)?;
    let shared = result_pitch_set(&record, "shared")?;
    let left_only = result_pitch_set(&record, "left_only")?;
    let right_only = result_pitch_set(&record, "right_only")?;

    let mut expected_shared = BTreeMap::new();
    for (pitch_class, left_label) in &left {
        if let Some(right_label) = right.get(pitch_class) {
            if left_label != right_label {
                return Err(format!(
                    "Pitch class {pitch_class} has conflicting labels in the supplied pitch sets"
                ));
            }
            expected_shared.insert(*pitch_class, left_label.clone());
        }
    }
    let expected_left_only = difference(&left, &right);
    let expected_right_only = difference(&right, &left);
    if shared != expected_shared {
        return Err("Woodshed shared evidence disagrees with supplied pitch sets".into());
    }
    if left_only != expected_left_only {
        return Err("Woodshed left_only evidence disagrees with supplied pitch sets".into());
    }
    if right_only != expected_right_only {
        return Err("Woodshed right_only evidence disagrees with supplied pitch sets".into());
    }
    validate_singleton_motion(&record, &expected_left_only, &expected_right_only)?;
    Ok(record)
}

fn result_field<'a>(
    record: &'a ComparisonRecord,
    name: &str,
) -> Result<&'a serde_json::Value, String> {
    record
        .result
        .as_object()
        .and_then(|result| result.get(name))
        .ok_or_else(|| format!("Woodshed result is missing {name}"))
}

fn validate_subject_label(path: &str, label: &str) -> Result<(), String> {
    if label.trim().is_empty() || label.chars().count() > MAX_SUBJECT_LABEL_CHARS {
        return Err(format!(
            "{path} must be non-empty and at most {MAX_SUBJECT_LABEL_CHARS} characters"
        ));
    }
    Ok(())
}

fn pitch_set(
    path: &str,
    values: &BTreeMap<String, serde_json::Value>,
) -> Result<BTreeMap<u64, String>, String> {
    let value = values
        .get("pitch_set")
        .ok_or_else(|| format!("{path} is missing"))?;
    parse_pitch_set(path, value)
}

fn result_pitch_set(
    record: &ComparisonRecord,
    name: &str,
) -> Result<BTreeMap<u64, String>, String> {
    parse_pitch_set(&format!("result.{name}"), result_field(record, name)?)
}

fn parse_pitch_set(path: &str, value: &serde_json::Value) -> Result<BTreeMap<u64, String>, String> {
    let values = value
        .as_array()
        .ok_or_else(|| format!("{path} must be an array"))?;
    if values.len() > MAX_PITCHES {
        return Err(format!("{path} may contain at most {MAX_PITCHES} pitches"));
    }
    let mut pitches = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let object = value
            .as_object()
            .ok_or_else(|| format!("{path}[{index}] must be an object"))?;
        let pitch_class = object
            .get("pitch_class")
            .and_then(serde_json::Value::as_u64)
            .filter(|pitch_class| *pitch_class < 12)
            .ok_or_else(|| format!("{path}[{index}].pitch_class must be an integer below 12"))?;
        let label = object
            .get("label")
            .and_then(serde_json::Value::as_str)
            .filter(|label| !label.trim().is_empty() && label.chars().count() <= MAX_TONE_LABEL_CHARS)
            .ok_or_else(|| {
                format!(
                    "{path}[{index}].label must be non-empty and at most {MAX_TONE_LABEL_CHARS} characters"
                )
            })?;
        if !seen.insert(pitch_class) {
            return Err(format!("{path} repeats pitch class {pitch_class}"));
        }
        pitches.insert(pitch_class, label.to_owned());
    }
    Ok(pitches)
}

fn difference(
    left: &BTreeMap<u64, String>,
    right: &BTreeMap<u64, String>,
) -> BTreeMap<u64, String> {
    left.iter()
        .filter(|(pitch_class, _)| !right.contains_key(pitch_class))
        .map(|(pitch_class, label)| (*pitch_class, label.clone()))
        .collect()
}

fn validate_singleton_motion(
    record: &ComparisonRecord,
    left_only: &BTreeMap<u64, String>,
    right_only: &BTreeMap<u64, String>,
) -> Result<(), String> {
    let Some(motion) = record
        .result
        .as_object()
        .and_then(|result| result.get("singleton_motion"))
    else {
        return Ok(());
    };
    if left_only.len() != 1 || right_only.len() != 1 {
        return Err("singleton_motion requires one left_only and one right_only pitch".into());
    }
    let motion = motion
        .as_object()
        .ok_or_else(|| "singleton_motion must be an object".to_string())?;
    let from = parse_pitch_set(
        "result.singleton_motion.from",
        &serde_json::Value::Array(vec![
            motion
                .get("from")
                .cloned()
                .ok_or_else(|| "singleton_motion.from is missing".to_string())?,
        ]),
    )?;
    let to = parse_pitch_set(
        "result.singleton_motion.to",
        &serde_json::Value::Array(vec![
            motion
                .get("to")
                .cloned()
                .ok_or_else(|| "singleton_motion.to is missing".to_string())?,
        ]),
    )?;
    if &from != left_only || &to != right_only {
        return Err("singleton_motion disagrees with the disclosed directional differences".into());
    }
    Ok(())
}

/// The source adapter that names the comparison occurrence.
const COMPARISON_ADAPTER: &str = "graphshell.comparison";
/// Subject fields the dataset carries; any other subject disclosure is refused.
const SUBJECT_EVIDENCE: [&str; 2] = ["pitch_set", "root_pitch_class"];
/// Result fields the dataset carries; any other result disclosure is refused.
const RESULT_EVIDENCE: [&str; 6] = [
    "version",
    "status",
    "shared",
    "left_only",
    "right_only",
    "singleton_motion",
];

/// Carry one checked Woodshed comparison as a host dataset.
///
/// Each subject is an occurrence with its label, its pitch set as text
/// (`"0:C 4:E 7:G"`, in disclosed order) and its root pitch class. The
/// comparison is a third occurrence, identified by
/// [`ComparisonRecord::relation`], carrying the two subjects, the method,
/// and the result. `x` and `y` are this host's slots for the three cards.
/// There are no relationships: the comparison is an occurrence, as before.
pub fn comparison_host_dataset(record: &ComparisonRecord) -> Result<HostDatasetV1, String> {
    if let Some(key) = record.disclosed.keys().next() {
        return Err(format!(
            "the comparison discloses {key}, which the dataset cannot carry"
        ));
    }
    let result = record
        .result
        .as_object()
        .ok_or("Woodshed result must be an object")?;
    if let Some(key) = result
        .keys()
        .find(|key| !RESULT_EVIDENCE.contains(&key.as_str()))
    {
        return Err(format!(
            "the result discloses {key}, which the dataset cannot carry"
        ));
    }
    let relation = record.relation();
    let text = |value: &str| ProjectionValue::Text(value.to_owned());
    let mut occurrences = Vec::new();
    for (subject, x) in [(&record.left, 0.0), (&record.right, 24.0)] {
        if let Some(key) = subject
            .disclosed
            .keys()
            .find(|key| !SUBJECT_EVIDENCE.contains(&key.as_str()))
        {
            return Err(format!(
                "{} discloses {key}, which the dataset cannot carry",
                subject.occurrence_id
            ));
        }
        let mut values = BTreeMap::from([
            ("occurrence_id".into(), text(&subject.occurrence_id)),
            ("label".into(), text(&subject.label)),
            ("x".into(), ProjectionValue::Number(x)),
            ("y".into(), ProjectionValue::Number(0.0)),
            (
                "pitch_set".into(),
                ProjectionValue::Text(encode_pitches(&subject.disclosed["pitch_set"])?),
            ),
        ]);
        if let Some(root) = subject.disclosed.get("root_pitch_class") {
            let root = root
                .as_u64()
                .ok_or("root_pitch_class must be a whole number")?;
            values.insert(
                "root_pitch_class".into(),
                ProjectionValue::Number(root as f64),
            );
        }
        occurrences.push(ProjectionOccurrence {
            occurrence_id: subject.occurrence_id.clone(),
            source: sceno::SourceRef::new(&subject.source.adapter, &subject.source.id),
            values,
        });
    }
    let number = |name: &str| {
        result[name]
            .as_u64()
            .map(|value| ProjectionValue::Number(value as f64))
            .ok_or(format!("result.{name} must be a whole number"))
    };
    let mut values = BTreeMap::from([
        ("occurrence_id".into(), text(&relation.id)),
        (
            "label".into(),
            text(&format!("Common tones: {}", tone_labels(&result["shared"]))),
        ),
        ("x".into(), ProjectionValue::Number(12.0)),
        ("y".into(), ProjectionValue::Number(14.0)),
        ("left_occurrence".into(), text(&record.left.occurrence_id)),
        ("right_occurrence".into(), text(&record.right.occurrence_id)),
        ("method".into(), text(&record.method.id)),
        (
            "method_version".into(),
            ProjectionValue::Number(record.method.version.into()),
        ),
        ("result_version".into(), number("version")?),
        (
            "result_status".into(),
            text(
                result["status"]
                    .as_str()
                    .ok_or("result.status must be text")?,
            ),
        ),
        (
            "shared".into(),
            ProjectionValue::Text(encode_pitches(&result["shared"])?),
        ),
        (
            "left_only".into(),
            ProjectionValue::Text(encode_pitches(&result["left_only"])?),
        ),
        (
            "right_only".into(),
            ProjectionValue::Text(encode_pitches(&result["right_only"])?),
        ),
    ]);
    if let Some(motion) = result.get("singleton_motion") {
        for end in ["from", "to"] {
            values.insert(
                format!("motion_{end}"),
                ProjectionValue::Text(encode_pitches(&Value::Array(vec![motion[end].clone()]))?),
            );
        }
    }
    occurrences.push(ProjectionOccurrence {
        occurrence_id: relation.id.clone(),
        source: sceno::SourceRef::new(COMPARISON_ADAPTER, &relation.id),
        values,
    });
    let mut fields: BTreeMap<String, ProjectionFieldType> = BTreeMap::new();
    for occurrence in &occurrences {
        for (name, value) in &occurrence.values {
            fields.insert(name.clone(), value.field_type());
        }
    }
    Ok(HostDatasetV1 {
        schema: HOST_DATASET_SCHEMA_V1.into(),
        dataset: ProjectionDataset {
            source: SourceBinding {
                authority: record.source.authority.clone(),
                domain: record.source.domain.clone(),
                resource: record.source.resource.clone(),
            },
            revision: record.revision.clone().into(),
            fields,
            occurrences,
        },
        relationships: Vec::new(),
        revisions: vec![HostDatasetRevision {
            sequence: 1,
            revision: record.revision.clone().into(),
        }],
    })
}

/// Rebuild the Woodshed comparison a host dataset carries, and check it.
///
/// The dataset must hold exactly the two subjects and the comparison
/// occurrence. The rebuilt record passes [`parse_woodshed_comparison`], and
/// the comparison occurrence's identity must be the record's relation id.
pub fn comparison_from_host_dataset(envelope: &HostDatasetV1) -> Result<ComparisonRecord, String> {
    let dataset = &envelope.dataset;
    let mut comparisons = dataset
        .occurrences
        .iter()
        .filter(|occurrence| occurrence.values.contains_key("left_occurrence"));
    let comparison = comparisons
        .next()
        .ok_or("the practice dataset carries no comparison occurrence")?;
    if comparisons.next().is_some() {
        return Err("the practice dataset carries more than one comparison".into());
    }
    let text = |occurrence: &ProjectionOccurrence, name: &str| {
        occurrence
            .values
            .get(name)
            .and_then(ProjectionValue::text)
            .map(str::to_owned)
            .ok_or_else(|| format!("{} carries no text {name}", occurrence.occurrence_id))
    };
    let whole = |occurrence: &ProjectionOccurrence, name: &str| {
        occurrence
            .values
            .get(name)
            .and_then(ProjectionValue::number)
            .filter(|value| *value >= 0.0 && value.fract() == 0.0 && *value <= u32::MAX as f64)
            .map(|value| value as u64)
            .ok_or_else(|| {
                format!(
                    "{} carries no whole number {name}",
                    occurrence.occurrence_id
                )
            })
    };
    let subject = |side: &str| -> Result<Value, String> {
        let id = text(comparison, side)?;
        let occurrence = dataset
            .occurrences
            .iter()
            .find(|occurrence| occurrence.occurrence_id == id)
            .ok_or_else(|| format!("{side} names {id}, which the dataset does not carry"))?;
        let mut value = json!({
            "occurrence_id": id,
            "source": { "adapter": occurrence.source.adapter, "id": occurrence.source.id },
            "label": text(occurrence, "label")?,
            "pitch_set": decode_pitches(&text(occurrence, "pitch_set")?)?,
        });
        if occurrence.values.contains_key("root_pitch_class") {
            value["root_pitch_class"] = whole(occurrence, "root_pitch_class")?.into();
        }
        Ok(value)
    };
    let mut result = json!({
        "version": whole(comparison, "result_version")?,
        "status": text(comparison, "result_status")?,
        "shared": decode_pitches(&text(comparison, "shared")?)?,
        "left_only": decode_pitches(&text(comparison, "left_only")?)?,
        "right_only": decode_pitches(&text(comparison, "right_only")?)?,
    });
    if comparison.values.contains_key("motion_from") || comparison.values.contains_key("motion_to")
    {
        let single = |name: &str| -> Result<Value, String> {
            match decode_pitches(&text(comparison, name)?)? {
                Value::Array(mut pitches) if pitches.len() == 1 => Ok(pitches.remove(0)),
                _ => Err(format!("{name} must name one pitch")),
            }
        };
        result["singleton_motion"] =
            json!({ "from": single("motion_from")?, "to": single("motion_to")? });
    }
    let value = json!({
        "source": {
            "authority": dataset.source.authority,
            "domain": dataset.source.domain,
            "resource": dataset.source.resource,
        },
        "revision": dataset.revision.as_str(),
        "method": {
            "id": text(comparison, "method")?,
            "version": whole(comparison, "method_version")?,
        },
        "left": subject("left_occurrence")?,
        "right": subject("right_occurrence")?,
        "result": result,
    });
    let record = parse_woodshed_comparison(&value.to_string())?;
    if comparison.occurrence_id != record.relation().id {
        return Err(format!(
            "the comparison occurrence {} is not this comparison's relation {}",
            comparison.occurrence_id,
            record.relation().id
        ));
    }
    let carried: BTreeSet<_> = dataset
        .occurrences
        .iter()
        .map(|occurrence| occurrence.occurrence_id.as_str())
        .collect();
    let expected = BTreeSet::from([
        record.left.occurrence_id.as_str(),
        record.right.occurrence_id.as_str(),
        comparison.occurrence_id.as_str(),
    ]);
    if carried != expected || dataset.occurrences.len() != 3 {
        return Err(
            "the practice dataset must carry exactly the two subjects and their comparison".into(),
        );
    }
    Ok(record)
}

/// `[{pitch_class: 0, label: "C"}, ...]` as `"0:C ..."`, in disclosed order.
fn encode_pitches(value: &Value) -> Result<String, String> {
    let pitches = parse_pitch_set("pitch set", value)?;
    let mut entries = Vec::new();
    for item in value.as_array().into_iter().flatten() {
        let pitch_class = item["pitch_class"].as_u64().unwrap_or_default();
        let label = &pitches[&pitch_class];
        if label.chars().any(char::is_whitespace) {
            return Err(format!("tone label {label:?} cannot be carried as text"));
        }
        entries.push(format!("{pitch_class}:{label}"));
    }
    Ok(entries.join(" "))
}

/// The inverse of [`encode_pitches`]; the parse checks the members.
fn decode_pitches(text: &str) -> Result<Value, String> {
    text.split_whitespace()
        .map(|entry| {
            let (pitch_class, label) = entry
                .split_once(':')
                .ok_or_else(|| format!("pitch {entry:?} is not class:label"))?;
            let pitch_class: u64 = pitch_class
                .parse()
                .map_err(|_| format!("pitch {entry:?} has no whole pitch class"))?;
            Ok(json!({ "pitch_class": pitch_class, "label": label }))
        })
        .collect::<Result<Vec<_>, String>>()
        .map(Value::Array)
}

/// The labels of a disclosed pitch set, joined for a card.
pub fn tone_labels(value: &Value) -> String {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.get("label").and_then(|v| v.as_str()))
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn export() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../../../woodshed/scenarios/woodshed_musical_comparison.json"
        ))
        .unwrap()
    }

    #[test]
    fn accepts_the_real_woodshed_export() {
        let record = parse_woodshed_comparison(&export().to_string()).unwrap();
        assert_eq!(
            record.left.disclosed["pitch_set"].as_array().unwrap().len(),
            3
        );
    }

    #[test]
    fn rejects_wrong_source() {
        let mut value = export();
        value["source"]["resource"] = "set:another".into();
        assert!(
            parse_woodshed_comparison(&value.to_string())
                .unwrap_err()
                .contains("source")
        );
    }

    #[test]
    fn rejects_missing_pitch_set() {
        let mut value = export();
        value["left"].as_object_mut().unwrap().remove("pitch_set");
        assert!(
            parse_woodshed_comparison(&value.to_string())
                .unwrap_err()
                .contains("left.pitch_set")
        );
    }

    #[test]
    fn rejects_excessive_pitch_cardinality_before_reading_members() {
        let mut value = export();
        let pitches = value["left"]["pitch_set"].as_array_mut().unwrap();
        while pitches.len() <= MAX_PITCHES {
            pitches.push(serde_json::json!({ "pitch_class": 0, "label": "C" }));
        }
        assert!(
            parse_woodshed_comparison(&value.to_string())
                .unwrap_err()
                .contains("at most")
        );
    }

    #[test]
    fn rejects_a_reported_partition_that_contradicts_literal_pitch_sets() {
        let mut value = export();
        value["result"]["shared"] = serde_json::json!([]);
        assert!(
            parse_woodshed_comparison(&value.to_string())
                .unwrap_err()
                .contains("shared evidence")
        );
    }

    #[test]
    fn rejects_directionally_wrong_singleton_motion() {
        let mut value = export();
        value["result"]["singleton_motion"]["from"]["label"] = "A".into();
        assert!(
            parse_woodshed_comparison(&value.to_string())
                .unwrap_err()
                .contains("singleton_motion")
        );
    }

    /// The served practice dataset, relative to this crate.
    const SERVED: &str = "web/fixtures/woodshed-comparison.host-dataset.json";

    fn record() -> ComparisonRecord {
        parse_woodshed_comparison(&export().to_string()).unwrap()
    }

    fn served() -> String {
        serde_json::to_string_pretty(&comparison_host_dataset(&record()).unwrap()).unwrap() + "\n"
    }

    /// The served file is this conversion of Woodshed's export, byte for byte.
    /// `GRAPHSHELL_WRITE_PRACTICE_DATASET=1 cargo test -p graphshell
    /// served_practice_dataset` rewrites it.
    #[test]
    fn served_practice_dataset_is_the_converted_woodshed_export() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(SERVED);
        if std::env::var_os("GRAPHSHELL_WRITE_PRACTICE_DATASET").is_some() {
            std::fs::write(&path, served()).unwrap();
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), served());
    }

    #[test]
    fn the_served_dataset_rebuilds_the_woodshed_record() {
        let envelope = scenomise::host_dataset::parse_host_dataset(&served()).unwrap();
        assert!(envelope.relationships.is_empty());
        assert_eq!(envelope.dataset.occurrences.len(), 3);
        assert_eq!(comparison_from_host_dataset(&envelope).unwrap(), record());
    }

    #[test]
    fn the_served_dataset_keeps_the_comparison_an_occurrence() {
        let envelope = comparison_host_dataset(&record()).unwrap();
        let relation = record().relation();
        let comparison = envelope
            .dataset
            .occurrences
            .iter()
            .find(|occurrence| occurrence.occurrence_id == relation.id)
            .unwrap();
        assert_eq!(
            comparison.source,
            sceno::SourceRef::new(COMPARISON_ADAPTER, &relation.id)
        );
        assert_eq!(
            comparison.values["label"],
            ProjectionValue::Text("Common tones: C · E".into())
        );
    }

    #[test]
    fn rebuild_refuses_a_comparison_under_another_identity() {
        let mut envelope = comparison_host_dataset(&record()).unwrap();
        let comparison = envelope.dataset.occurrences.last_mut().unwrap();
        comparison.occurrence_id = "comparison:0000000000000000".into();
        comparison.values.insert(
            "occurrence_id".into(),
            ProjectionValue::Text(comparison.occurrence_id.clone()),
        );
        assert!(
            comparison_from_host_dataset(&envelope)
                .unwrap_err()
                .contains("is not this comparison's relation")
        );
    }

    #[test]
    fn rebuild_rechecks_the_woodshed_evidence() {
        let mut envelope = comparison_host_dataset(&record()).unwrap();
        envelope.dataset.occurrences[0].values.insert(
            "pitch_set".into(),
            ProjectionValue::Text("0:C 4:E 6:Gb".into()),
        );
        assert!(
            comparison_from_host_dataset(&envelope)
                .unwrap_err()
                .contains("disagrees")
        );
    }

    #[test]
    fn rebuild_refuses_an_extra_occurrence() {
        let mut envelope = comparison_host_dataset(&record()).unwrap();
        let mut extra = envelope.dataset.occurrences[0].clone();
        extra.occurrence_id = "card:3".into();
        extra.values.insert(
            "occurrence_id".into(),
            ProjectionValue::Text("card:3".into()),
        );
        envelope.dataset.occurrences.push(extra);
        assert!(
            comparison_from_host_dataset(&envelope)
                .unwrap_err()
                .contains("exactly the two subjects")
        );
    }
}
