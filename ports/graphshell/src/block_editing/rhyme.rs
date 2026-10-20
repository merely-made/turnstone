// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A small editor adapter for the existing Knot `rhyme` attribute.
//!
//! This module records authored expectations only. It does not infer rhyme
//! from spelling, pronunciation, or sound.

use super::Change;
use sceno::SourceRef;
use scenomise::backdrops::{MetreGrid, SceneFacts, SceneInput, Syllable, VerseLine};
use std::collections::{BTreeMap, HashSet};

const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_ATTRIBUTE_BYTES: usize = 16 * 1024;
const MAX_PATTERN_BYTES: usize = 512;
const MAX_LINES: usize = 256;

/// Produce one proposed authored rhyme-group change per selected nonblank line.
///
/// The `rhyme` attribute is whitespace separated, aligned with nonblank body
/// lines. `-` means that no expectation is authored for that line. If the
/// attribute exists but no longer has one token per current nonblank line, the
/// caller must ask for review instead of silently shifting the labels.
pub fn choices(
    body: &str,
    attrs: &BTreeMap<String, String>,
    selected: &[usize],
    pattern: &str,
) -> Result<Vec<Change>, String> {
    let lines = checked_lines(body)?;
    let selected = checked_selection(&lines, selected)?;
    let pattern = parse_pattern(pattern)?;
    if pattern.len() != selected.len() {
        return Err(format!(
            "Rhyme pattern has {} labels for {} selected nonblank lines",
            pattern.len(),
            selected.len()
        ));
    }
    let before = existing_groups(attrs, nonblank_count(&lines))?;
    Ok(selected
        .into_iter()
        .zip(pattern)
        .map(|((index, ordinal), after)| {
            let current = before[ordinal].clone();
            let conflict = current.as_ref().is_some_and(|value| value != &after);
            Change {
                index,
                label: format!("Line {} · rhyme {after}", index + 1),
                before: current,
                after,
                conflict,
                use_update: !conflict,
            }
        })
        .collect())
}

/// Apply accepted rhyme changes, preserving every other attribute verbatim.
///
/// When any change is applied, `-` placeholders retain the absence of an
/// expectation on unselected rows. This is a provisional, source-compatible
/// Knot adapter format; canonical adoption remains a separate decision.
pub fn attributes(
    body: &str,
    attrs: &BTreeMap<String, String>,
    changes: &[Change],
) -> Result<BTreeMap<String, String>, String> {
    let lines = checked_lines(body)?;
    let mut groups = existing_groups(attrs, nonblank_count(&lines))?;
    let mut seen = HashSet::new();
    let mut changed = false;
    for change in changes {
        if change.index >= lines.len() || !seen.insert(change.index) {
            return Err("Rhyme changes contain an invalid or repeated line index".into());
        }
        if lines[change.index].trim().is_empty() {
            return Err(format!(
                "Line {} is blank and cannot have a rhyme label",
                change.index + 1
            ));
        }
        let ordinal = nonblank_ordinal(&lines, change.index).ok_or("Selected line is blank")?;
        if groups[ordinal] != change.before {
            return Err(format!(
                "Rhyme label for line {} changed; review it before applying",
                change.index + 1
            ));
        }
        if !change.use_update {
            continue;
        }
        validate_label(&change.after)?;
        let next = Some(change.after.clone());
        if groups[ordinal] != next {
            groups[ordinal] = next;
            changed = true;
        }
    }
    let mut result = attrs.clone();
    if changed {
        result.insert(
            "rhyme".into(),
            groups
                .into_iter()
                .map(|group| group.unwrap_or_else(|| "-".into()))
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    Ok(result)
}

/// Build the existing portable metre preview from source text and authored
/// rhyme expectations. Syllables, stress, and actual rhyme tails stay unknown.
pub fn scene(
    address: &str,
    revision: &str,
    body: &str,
    attrs: &BTreeMap<String, String>,
) -> Result<SceneInput, String> {
    let lines = checked_lines(body)?;
    let groups = existing_groups(attrs, nonblank_count(&lines))?;
    if address.trim().is_empty() || revision.trim().is_empty() {
        return Err("A source address and revision are required for the preview".into());
    }
    let source = SourceRef::new("knot.djot", address);
    let verse_lines = lines
        .iter()
        .enumerate()
        .filter(|(_, text)| !text.trim().is_empty())
        .map(|(index, text)| VerseLine {
            key: format!("line-{index}"),
            source: SourceRef::new("knot.djot.line", format!("{address}#line/{index}")),
            text: (*text).to_owned(),
            syllables: Vec::<Syllable>::new(),
            alignment: None,
            rhyme_group: groups[index_to_ordinal(&lines, index).expect("nonblank line")].clone(),
            rhyme_tail: None,
        })
        .collect();
    let input = SceneInput {
        version: scenomise::backdrops::BACKDROP_SCENE_VERSION,
        source,
        revision: revision.to_owned(),
        title: "Authored rhyme expectations".into(),
        facts: SceneFacts::MetreGrid(MetreGrid {
            feet: 0,
            positions_per_foot: 2,
            lines: verse_lines,
        }),
    };
    input.validate().map_err(|error| error.to_string())?;
    Ok(input)
}

fn checked_lines(body: &str) -> Result<Vec<&str>, String> {
    if body.len() > MAX_BODY_BYTES {
        return Err("Text is too large to preview (maximum 64 KiB)".into());
    }
    let lines: Vec<_> = body.lines().collect();
    if lines.iter().filter(|line| !line.trim().is_empty()).count() > MAX_LINES {
        return Err("Text has more than 256 nonblank lines".into());
    }
    Ok(lines)
}

fn nonblank_count(lines: &[&str]) -> usize {
    lines.iter().filter(|line| !line.trim().is_empty()).count()
}

fn existing_groups(
    attrs: &BTreeMap<String, String>,
    line_count: usize,
) -> Result<Vec<Option<String>>, String> {
    let Some(value) = attrs.get("rhyme") else {
        return Ok(vec![None; line_count]);
    };
    if value.len() > MAX_ATTRIBUTE_BYTES {
        return Err("Existing rhyme metadata is too large to review".into());
    }
    let labels: Vec<_> = value.split_whitespace().collect();
    if labels.len() != line_count {
        return Err(format!(
            "Existing rhyme metadata has {} labels for {line_count} nonblank lines; review before editing",
            labels.len()
        ));
    }
    labels
        .into_iter()
        .map(|label| {
            if label == "-" {
                Ok(None)
            } else {
                validate_label(label)?;
                Ok(Some(label.to_owned()))
            }
        })
        .collect()
}

fn validate_label(label: &str) -> Result<(), String> {
    if label.len() != 1 || !label.as_bytes()[0].is_ascii_alphabetic() {
        return Err("Rhyme labels must be single letters or '-' for no expectation".into());
    }
    Ok(())
}

fn parse_pattern(pattern: &str) -> Result<Vec<String>, String> {
    if pattern.len() > MAX_PATTERN_BYTES {
        return Err("Rhyme pattern is too long".into());
    }
    let labels: Vec<String> = if pattern.chars().any(char::is_whitespace) {
        pattern.split_whitespace().map(str::to_owned).collect()
    } else {
        pattern.chars().map(|ch| ch.to_string()).collect()
    };
    for label in &labels {
        validate_label(label)?;
    }
    Ok(labels)
}

fn checked_selection(lines: &[&str], selected: &[usize]) -> Result<Vec<(usize, usize)>, String> {
    if selected.is_empty() {
        return Err("Select at least one nonblank line".into());
    }
    let mut seen = HashSet::new();
    selected
        .iter()
        .map(|&index| {
            if !seen.insert(index) {
                return Err("A line is selected more than once".into());
            }
            if lines.get(index).is_none_or(|line| line.trim().is_empty()) {
                return Err(format!("Line {} is blank or no longer exists", index + 1));
            }
            let ordinal = nonblank_ordinal(lines, index).ok_or("Selected line is blank")?;
            Ok((index, ordinal))
        })
        .collect()
}

fn nonblank_ordinal(lines: &[&str], index: usize) -> Option<usize> {
    lines
        .get(index)
        .filter(|line| !line.trim().is_empty())
        .and_then(|_| index_to_ordinal(lines, index))
}

fn index_to_ordinal(lines: &[&str], index: usize) -> Option<usize> {
    (index < lines.len() && !lines[index].trim().is_empty()).then(|| {
        lines[..index]
            .iter()
            .filter(|line| !line.trim().is_empty())
            .count()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(value: &str) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("rhyme".into(), value.into()),
            ("class".into(), "verse stanza".into()),
            ("custom".into(), "keep this exactly".into()),
        ])
    }

    #[test]
    fn subset_update_preserves_unselected_groups_and_other_attributes() {
        let body = "first\nsecond\n\nthird\nfourth";
        let current = attrs("A B A B");
        let mut changes = choices(body, &current, &[1, 4], "C C").unwrap();
        assert_eq!(changes.len(), 2);
        for change in &mut changes {
            change.use_update = true;
        }
        let next = attributes(body, &current, &changes).unwrap();
        assert_eq!(next.get("rhyme").map(String::as_str), Some("A C A C"));
        assert_eq!(next.get("class"), current.get("class"));
        assert_eq!(next.get("custom"), current.get("custom"));
        assert_eq!(body, "first\nsecond\n\nthird\nfourth");
    }

    #[test]
    fn blank_stanza_lines_do_not_consume_rhyme_labels() {
        let body = "first\n\nsecond\nthird\n\nfourth";
        let current = attrs("A B C D");
        let mut changes = choices(body, &current, &[2, 5], "D E").unwrap();
        assert_eq!(changes[0].before.as_deref(), Some("B"));
        assert_eq!(changes[1].before.as_deref(), Some("D"));
        for change in &mut changes {
            change.use_update = true;
        }
        let next = attributes(body, &current, &changes).unwrap();
        assert_eq!(next.get("rhyme").map(String::as_str), Some("A D C E"));
    }

    #[test]
    fn selected_subset_adds_unselected_no_expectation_sentinels() {
        let body = "first\n\nsecond\nthird";
        let mut current = BTreeMap::new();
        current.insert("class".into(), "metre".into());
        let changes = choices(body, &current, &[0, 3], "A B").unwrap();
        let next = attributes(body, &current, &changes).unwrap();
        assert_eq!(next.get("rhyme").map(String::as_str), Some("A - B"));

        let body = "first\nsecond\nthird";
        let changes = choices(body, &current, &[0, 2], "A C").unwrap();
        let next = attributes(body, &current, &changes).unwrap();
        assert_eq!(next.get("rhyme").map(String::as_str), Some("A - C"));
    }

    #[test]
    fn keeping_every_suggestion_preserves_the_original_attribute_map() {
        let body = "first\nsecond";
        let current = attrs("A B");
        let mut changes = choices(body, &current, &[0, 1], "C D").unwrap();
        for change in &mut changes {
            change.use_update = false;
        }
        assert_eq!(attributes(body, &current, &changes).unwrap(), current);
    }

    #[test]
    fn identical_pattern_preserves_authored_rhyme_whitespace() {
        let body = "first\nsecond\nthird\nfourth";
        let current = attrs("A  B\tB A");
        let changes = choices(body, &current, &[0, 1, 2, 3], "A B B A").unwrap();
        assert_eq!(attributes(body, &current, &changes).unwrap(), current);
    }

    #[test]
    fn conflicting_local_group_is_kept_unless_user_accepts_update() {
        let body = "first\nsecond";
        let current = attrs("A B");
        let mut changes = choices(body, &current, &[0], "C").unwrap();
        assert!(changes[0].conflict);
        assert!(!changes[0].use_update);
        let kept = attributes(body, &current, &changes).unwrap();
        assert_eq!(kept.get("rhyme").map(String::as_str), Some("A B"));
        changes[0].use_update = true;
        let updated = attributes(body, &current, &changes).unwrap();
        assert_eq!(updated.get("rhyme").map(String::as_str), Some("C B"));
    }

    #[test]
    fn preview_keeps_text_and_expectations_without_inventing_rhyme_evidence() {
        let body = "A line\n\nAnother line";
        let input = scene("urn:test:poem", "rev-1", body, &attrs("A B")).unwrap();
        let compiled = input.compile().unwrap();
        assert!(compiled.guides.iter().any(|label| label.text == "A line"));
        assert!(
            compiled
                .guides
                .iter()
                .any(|label| label.text == "Another line")
        );
        assert_eq!(compiled.marks.len(), 2); // Text captions, no invented syllable marks.
        assert!(compiled.scene.bounds.size.w > 200.0);
        let SceneFacts::MetreGrid(grid) = input.facts else {
            panic!("expected a metre-grid preview")
        };
        assert_eq!(grid.feet, 0);
        assert_eq!(grid.positions_per_foot, 2);
        assert_eq!(grid.lines.len(), 2);
        assert_eq!(grid.lines[0].text, "A line");
        assert_eq!(grid.lines[1].text, "Another line");
        assert!(grid.lines.iter().all(|line| line.syllables.is_empty()));
        assert!(grid.lines.iter().all(|line| line.rhyme_tail.is_none()));
        assert_eq!(grid.lines[0].rhyme_group.as_deref(), Some("A"));
        assert_eq!(grid.lines[1].rhyme_group.as_deref(), Some("B"));
    }

    #[test]
    fn malformed_pattern_and_stale_attribute_count_require_review() {
        let body = "first\nsecond";
        assert!(choices(body, &BTreeMap::new(), &[0, 1], "A").is_err());
        assert!(choices(body, &attrs("A B C"), &[0], "C").is_err());
        assert!(attributes(body, &attrs("A B C"), &[]).is_err());
        assert!(choices(body, &BTreeMap::new(), &[0, 1], "A 1").is_err());
    }
}
