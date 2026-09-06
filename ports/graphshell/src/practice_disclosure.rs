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

use std::collections::{BTreeMap, BTreeSet};

use crate::practice_workspace::ComparisonRecord;

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
}
