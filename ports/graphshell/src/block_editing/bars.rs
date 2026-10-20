// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Graphshell adapter over Woodshed's actual portable Set and arrangement.
//! Musical coordinates and bar validation remain owned by `woodshedding`.
use super::Change;
use sceno::SourceRef;
use scenomise::{
    backdrops::{BACKDROP_SCENE_VERSION, SceneFacts, SceneInput},
    template::{
        Placement, SequenceTemplate, Span, TemplateGroup, TemplateOccurrence, TemplatedSequence,
    },
};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use woodshedding::rehearsal::{
    Material, Set,
    arrangement::{BarGroup, ProgressionArrangement, TimeSignatureChange, bars},
};

pub const SOURCE: &str = "woodshed";

/// Read the carrier without requiring metadata used by other commands.
fn raw_source(attrs: &BTreeMap<String, String>) -> Result<Value, String> {
    let json = attrs
        .get(SOURCE)
        .ok_or("This progression has no structured Woodshed source")?;
    if json.len() > super::MAX_DOCUMENT_BYTES {
        return Err("Woodshed source exceeds one MiB".into());
    }
    let raw: Value =
        serde_json::from_str(json).map_err(|e| format!("Invalid Woodshed source: {e}"))?;
    if raw.get("version").and_then(Value::as_u64) != Some(1) {
        return Err("Unsupported Woodshed source version; source preserved".into());
    }
    if !raw.get("set").is_some_and(Value::is_object) {
        return Err("Woodshed source needs its owning Set".into());
    }
    Ok(raw)
}

/// Keep the original JSON tree alongside the typed source. Applying a group
/// patches one owner field, retaining source fields this host does not know.
fn source(attrs: &BTreeMap<String, String>) -> Result<(Value, Set), String> {
    let raw = raw_source(attrs)?;
    let set: Set = serde_json::from_value(
        raw.get("set")
            .cloned()
            .ok_or("Woodshed source needs its owning Set")?,
    )
    .map_err(|e| format!("Invalid Woodshed Set: {e}"))?;
    let arr = arrangement(&set)?;
    if arr.cards.len() > 512
        || arr.rests.len() > 512
        || set.cards.len() > 512
        || arr.role_spans.len() > 512
    {
        return Err("Arrangement exceeds preview limits".into());
    }
    bars::validate_groups(arr)?;
    let mut ids = HashSet::new();
    for card in &set.cards {
        if !card.id.is_assigned() || !ids.insert(card.id) {
            return Err("Cards need unique persistent identities".into());
        }
    }
    let mut bound = HashSet::new();
    for card in &arr.cards {
        if !ids.contains(&card.card) || !bound.insert(card.card) {
            return Err("Alignment must name an existing Card once".into());
        }
    }
    let mut rests = HashSet::new();
    if arr
        .rests
        .iter()
        .any(|r| r.key.trim().is_empty() || !rests.insert(&r.key))
    {
        return Err("Rests need unique source identities".into());
    }
    Ok((raw, set))
}
/// Read the saved musical authority independently of Card/arrangement metadata.
/// Readings never derive or author notation.
pub fn music(
    attrs: &BTreeMap<String, String>,
) -> Result<Option<woodshedding::notation::Score>, String> {
    let raw = raw_source(attrs)?;
    raw.pointer("/set/music")
        .filter(|value| !value.is_null())
        .map(|value| {
            serde_json::from_value::<woodshedding::notation::MusicDocument>(value.clone())
                .map_err(|e| format!("Invalid saved MusicXML document: {e}"))?
                .score()
        })
        .transpose()
}

/// Explicitly write notation once, preserving unknown source fields. All
/// conversion and XML validation completes before a new declaration is returned.
pub fn write_music(
    attrs: &BTreeMap<String, String>,
    bpm: u16,
) -> Result<BTreeMap<String, String>, String> {
    let (mut raw, set) = source(attrs)?;
    if set.music.is_some() {
        return Err("This Set already has MusicXML; its saved notation was preserved".into());
    }
    if !(1..=400).contains(&bpm) {
        return Err("Notation tempo must be a whole number from 1 to 400 BPM".into());
    }
    let saved = set
        .progression_arrangement
        .as_ref()
        .ok_or("Save an arrangement first")?;
    if saved.alignment.time_signatures.is_empty() {
        return Err("Author a time-signature map before writing notation".into());
    }
    let context = woodshedding::card_shapes::ShapeContext::from_saved(&set)?;
    let score = woodshedding::notation::draft::score_from_set(
        &context,
        &set,
        &saved.alignment.name,
        f32::from(bpm),
    )?;
    let document = woodshedding::notation::MusicDocument::from_score(&score)?;
    raw.get_mut("set")
        .and_then(Value::as_object_mut)
        .ok_or("Missing owning Set")?
        .insert(
            "music".into(),
            serde_json::to_value(document).map_err(|e| e.to_string())?,
        );
    let mut result = attrs.clone();
    result.insert(
        SOURCE.into(),
        serde_json::to_string(&raw).map_err(|e| e.to_string())?,
    );
    Ok(result)
}

fn arrangement(set: &Set) -> Result<&ProgressionArrangement, String> {
    let saved = set
        .progression_arrangement
        .as_ref()
        .ok_or("Set has no saved progression arrangement")?;
    if saved.version != 1 {
        return Err("Unsupported saved arrangement version".into());
    }
    if saved.roles.len() != saved.alignment.role_spans.len() {
        return Err("Role expectations do not match their spans".into());
    }
    let arr = &saved.alignment;
    let valid = |lane: u16, s: woodshedding::rehearsal::arrangement::Span| {
        lane == 0
            && s.length > 0
            && s.start
                .checked_add(s.length)
                .is_some_and(|e| e <= arr.extent)
    };
    if arr.role_spans.iter().any(|s| !valid(0, *s))
        || arr.cards.iter().any(|c| {
            c.role.is_some_and(|r| r >= saved.roles.len())
                || c.placement.as_ref().is_some_and(|p| !valid(p.lane, p.span))
        })
        || arr.rests.iter().any(|r| {
            r.role.is_some_and(|i| i >= saved.roles.len())
                || !valid(r.placement.lane, r.placement.span)
        })
    {
        return Err("Invalid source musical alignment".into());
    }
    Ok(arr)
}
/// Read the owner's authored meter map without deriving a default meter.
pub fn time_signatures(
    attrs: &BTreeMap<String, String>,
) -> Result<Vec<TimeSignatureChange>, String> {
    let (_, set) = source(attrs)?;
    Ok(arrangement(&set)?.time_signatures.clone())
}

/// Patch a validated meter/group result into the retained source tree. The
/// caller supplies the owner's complete proposal; this helper does not choose
/// how meter changes should affect groups or musical material.
pub fn meter_attributes(
    attrs: &BTreeMap<String, String>,
    time_signatures: &[TimeSignatureChange],
    groups: &[BarGroup],
) -> Result<BTreeMap<String, String>, String> {
    let (mut raw, set) = source(attrs)?;
    let original = arrangement(&set)?;
    let mut next = original.clone();
    next.time_signatures = time_signatures.to_vec();
    next.bar_groups = groups.to_vec();
    bars::validate_groups(&next)?;
    if &next == original {
        return Ok(attrs.clone());
    }
    let alignment = raw
        .pointer_mut("/set/progression_arrangement/alignment")
        .and_then(Value::as_object_mut)
        .ok_or("Missing source arrangement")?;
    alignment.insert(
        "time_signatures".into(),
        serde_json::to_value(time_signatures).map_err(|e| e.to_string())?,
    );
    alignment.insert(
        "bar_groups".into(),
        serde_json::to_value(groups).map_err(|e| e.to_string())?,
    );
    let mut result = attrs.clone();
    result.insert(
        SOURCE.into(),
        serde_json::to_string(&raw).map_err(|e| e.to_string())?,
    );
    Ok(result)
}

fn counts(pattern: &str) -> Result<Vec<u32>, String> {
    if pattern.len() > 1024 {
        return Err("Bar grouping is too long".into());
    }
    let counts: Vec<u32> = pattern
        .split(|c: char| c.is_whitespace() || c == '+')
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse()
                .map_err(|_| "Use bar counts such as 2 + 2".to_string())
        })
        .collect::<Result<_, _>>()?;
    if counts.is_empty() || counts.iter().any(|n| *n == 0) {
        return Err("Use positive bar counts such as 2 + 2".into());
    }
    Ok(counts)
}
fn grouping(groups: &[BarGroup]) -> String {
    groups
        .iter()
        .map(|g| g.count.to_string())
        .collect::<Vec<_>>()
        .join(" + ")
}
fn old_selected(arr: &ProgressionArrangement, selected: &[usize]) -> Vec<BarGroup> {
    arr.bar_groups
        .iter()
        .filter(|g| selected.contains(&(g.first as usize)))
        .cloned()
        .collect()
}
pub fn rows(attrs: &BTreeMap<String, String>) -> Result<Vec<String>, String> {
    let (_, set) = source(attrs)?;
    let arr = arrangement(&set)?;
    Ok(bars::bars(arr)?
        .iter()
        .enumerate()
        .map(|(i, b)| {
            format!(
                "Bar {} · {} quarter-note beats{}",
                i + 1,
                f64::from(b.length) / 4.0,
                if b.complete { "" } else { " · partial" }
            )
        })
        .collect())
}
pub fn choices(
    attrs: &BTreeMap<String, String>,
    selected: &[usize],
    pattern: &str,
) -> Result<Vec<Change>, String> {
    let (_, set) = source(attrs)?;
    let arr = arrangement(&set)?;
    let next = bars::regroup(arr, selected, &counts(pattern)?)?;
    let old = old_selected(arr, selected);
    let after: Vec<_> = next
        .iter()
        .filter(|g| selected.contains(&(g.first as usize)))
        .cloned()
        .collect();
    let before = (!old.is_empty()).then(|| grouping(&old));
    let after = grouping(&after);
    let conflict = before.as_ref().is_some_and(|b| b != &after);
    Ok(vec![Change {
        index: selected[0],
        label: format!(
            "Bars {}–{} · grouping",
            selected[0] + 1,
            selected[selected.len() - 1] + 1
        ),
        before,
        after,
        conflict,
        use_update: !conflict,
    }])
}
pub fn attributes(
    attrs: &BTreeMap<String, String>,
    selected: &[usize],
    changes: &[Change],
) -> Result<BTreeMap<String, String>, String> {
    let (mut raw, set) = source(attrs)?;
    let arr = arrangement(&set)?;
    let change = changes
        .first()
        .filter(|_| changes.len() == 1)
        .ok_or("Preview one bar grouping at a time")?;
    let expected = choices(attrs, selected, &change.after)?;
    if change.index != expected[0].index || change.before != expected[0].before {
        return Err("Bar grouping changed; review a fresh preview".into());
    }
    if !change.use_update {
        return Ok(attrs.clone());
    }
    let groups = bars::regroup(arr, selected, &counts(&change.after)?)?;
    if groups == arr.bar_groups {
        return Ok(attrs.clone());
    }
    let alignment = raw
        .pointer_mut("/set/progression_arrangement/alignment")
        .and_then(Value::as_object_mut)
        .ok_or("Missing source arrangement")?;
    alignment.insert(
        "bar_groups".into(),
        serde_json::to_value(groups).map_err(|e| e.to_string())?,
    );
    let mut result = attrs.clone();
    result.insert(
        SOURCE.into(),
        serde_json::to_string(&raw).map_err(|e| e.to_string())?,
    );
    Ok(result)
}
fn span(s: woodshedding::rehearsal::arrangement::Span) -> Span {
    Span {
        start: s.start,
        length: s.length,
    }
}
pub fn scene(
    address: &str,
    revision: &str,
    attrs: &BTreeMap<String, String>,
) -> Result<SceneInput, String> {
    let (_, set) = source(attrs)?;
    let arr = arrangement(&set)?;
    let bars = bars::bars(arr)?;
    let mut groups = Vec::new();
    for (i, g) in arr.bar_groups.iter().enumerate() {
        let first = &bars[g.first as usize];
        let last = &bars[(g.first + g.count - 1) as usize];
        groups.push(TemplateGroup {
            key: format!("group-{i}"),
            lane: 0,
            span: Span {
                start: first.start,
                length: last.start + last.length - first.start,
            },
            parent: None,
            label: format!("Bars {}–{}", g.first + 1, g.first + g.count),
        });
    }
    for (i, b) in bars.iter().enumerate() {
        let parent = arr
            .bar_groups
            .iter()
            .position(|g| g.first as usize <= i && i < ((g.first + g.count) as usize))
            .map(|g| format!("group-{g}"));
        groups.push(TemplateGroup {
            key: format!("bar-{i}"),
            lane: 0,
            span: Span {
                start: b.start,
                length: b.length,
            },
            parent,
            label: format!(
                "Bar {}{}",
                i + 1,
                if b.complete { "" } else { " (partial)" }
            ),
        });
    }
    let mut occurrences = Vec::new();
    for card in &set.cards {
        let binding = arr.cards.iter().find(|c| c.card == card.id);
        occurrences.push(TemplateOccurrence {
            key: format!("card-{}", card.id.0),
            source: SourceRef::new("woodshed.card", format!("{address}#card={}", card.id.0)),
            label: card.label.clone(),
            detail: "Woodshed Card · supplied timing and voicing retained in source".into(),
            placement: binding
                .and_then(|c| c.placement.as_ref())
                .map(|p| Placement {
                    lane: p.lane,
                    span: span(p.span),
                }),
            slot: None,
        });
    }
    for rest in &arr.rests {
        occurrences.push(TemplateOccurrence {
            key: format!("rest-{}", rest.key),
            source: SourceRef::new("woodshed.rest", format!("{address}#rest={}", rest.key)),
            label: "Rest".into(),
            detail: "Explicit authored rest".into(),
            placement: Some(Placement {
                lane: rest.placement.lane,
                span: span(rest.placement.span),
            }),
            slot: None,
        });
    }
    let input = SceneInput {
        version: BACKDROP_SCENE_VERSION,
        source: SourceRef::new("woodshed.arrangement", address),
        revision: revision.into(),
        title: arr.name.clone(),
        facts: SceneFacts::TemplatedSequence(TemplatedSequence {
            template: SequenceTemplate {
                lanes: vec!["Woodshed arrangement".into()],
                extent: arr.extent,
                ticks_per_unit: 4,
                unit_label: "Quarter-note beats · source time-signature map".into(),
                groups,
            },
            slots: vec![],
            occurrences,
            relations: vec![],
        }),
    };
    input.validate().map_err(|e| e.to_string())?;
    Ok(input)
}
/// A readable, intentionally lossy chart. It never becomes the edit source:
/// exact durations, voicings, identities and settings remain in the Set JSON.
pub fn plaintext(attrs: &BTreeMap<String, String>) -> Result<String, String> {
    let (_, set) = source(attrs)?;
    let arr = arrangement(&set)?;
    let bars = bars::bars(arr)?;
    if bars.is_empty() {
        return Err("Author a time-signature map before exporting bar lines".into());
    }
    let mut events = Vec::new();
    for card in &set.cards {
        let p = arr
            .cards
            .iter()
            .find(|c| c.card == card.id)
            .and_then(|c| c.placement.as_ref())
            .ok_or("Plaintext export needs alignment for every Card")?;
        if !matches!(card.material, Material::Chord { .. }) {
            return Err("Plaintext chart currently accepts chord Cards and explicit rests".into());
        }
        events.push((
            p.span.start,
            p.span.start + p.span.length,
            card.label.clone(),
        ));
    }
    for r in &arr.rests {
        events.push((
            r.placement.span.start,
            r.placement.span.start + r.placement.span.length,
            "r".into(),
        ));
    }
    events.sort_by_key(|e| e.0);
    if events.windows(2).any(|w| w[0].1 > w[1].0) {
        return Err("Overlapping events need a richer notation than this plaintext chart".into());
    }
    let clean = |s: &str| s.replace(['|', '\n', '\r'], " ");
    let mut result = String::from(
        "# Readable export; structured Woodshed source retains exact timing and voicings.\n# r = authored rest; ? = unspecified time; ~ = continuation across a bar line.\n",
    );
    for (i, b) in bars.iter().enumerate() {
        if let Some(m) = arr.time_signatures.iter().find(|m| m.start == b.start) {
            result.push_str(&format!("\n{}/{} ", m.numerator, m.denominator));
        } else if i == 0 {
            result.push('\n');
        }
        result.push_str("| ");
        let mut cursor = b.start;
        for e in events
            .iter()
            .filter(|e| e.0 < b.start + b.length && e.1 > b.start)
        {
            if e.0 > cursor {
                result.push_str("? ");
            }
            if e.0 < b.start {
                result.push('~');
            }
            result.push_str(&clean(&e.2));
            result.push(' ');
            cursor = e.1.min(b.start + b.length);
        }
        if cursor < b.start + b.length {
            result.push_str("? ");
        }
        if !b.complete {
            result.push_str("[partial] ");
        }
    }
    result.push_str("|\n");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_editing::{Document, Preview};
    use cambium::TextCommand;
    fn attrs() -> BTreeMap<String, String> {
        BTreeMap::from([
            ("class".into(), "progression".into()),
            (
                SOURCE.into(),
                serde_json::from_str::<Value>(include_str!(
                    "../../support/woodshed-block-example.json"
                ))
                .unwrap()
                .to_string(),
            ),
            ("custom".into(), "keep me".into()),
        ])
    }
    #[test]
    fn write_notation_is_explicit_atomic_and_preserves_source_fields() {
        let mut a = attrs();
        let mut raw: Value = serde_json::from_str(&a[SOURCE]).unwrap();
        raw["set"]["future"] = serde_json::json!({"retained": true});
        a.insert(SOURCE.into(), raw.to_string());
        assert!(music(&a).unwrap().is_none());
        let written = write_music(&a, 108).unwrap();
        let score = music(&written).unwrap().unwrap();
        assert_eq!(score.tempo, 108);
        assert_eq!(score.measures.len(), 4);
        assert!(score.measures[1].events[1].rest);
        assert_eq!(
            score.measures[1].events[0].value,
            woodshedding::notation::NoteValue::Half
        );
        let mut next: Value = serde_json::from_str(&written[SOURCE]).unwrap();
        next["set"].as_object_mut().unwrap().remove("music");
        assert_eq!(next, raw);
        assert_eq!(a["custom"], written["custom"]);
        assert!(write_music(&written, 96).is_err());
        let source = illume::blocks::wrap(&a, "Keep body bytes α\n");
        let mut doc = Document::new("score", &source);
        let target = doc.blocks().remove(0);
        let preview = Preview::new(&source, target, vec![]).unwrap();
        doc.apply(&preview, written).unwrap();
        let saved = doc.editor.source().to_string();
        let reopened = Document::new("score", &saved);
        assert_eq!(
            music(&reopened.blocks()[0].attributes).unwrap(),
            Some(score)
        );
        doc.editor.apply(TextCommand::Undo);
        assert_eq!(doc.editor.source(), source);
        doc.editor.apply(TextCommand::Redo);
        assert_eq!(doc.editor.source(), saved);
    }

    #[test]
    fn write_notation_accepts_full_written_tempo_range() {
        let a = attrs();
        let before = a.clone();
        for bpm in [1, 19, 20, 400] {
            let written = write_music(&a, bpm).unwrap();
            assert_eq!(music(&written).unwrap().unwrap().tempo, bpm);
        }
        for bpm in [0, 401, u16::MAX] {
            assert!(
                write_music(&a, bpm)
                    .unwrap_err()
                    .contains("whole number from 1 to 400 BPM")
            );
        }
        assert_eq!(a, before);
    }

    #[test]
    fn unsupported_material_or_corrupt_music_refuses_without_replacing_source() {
        let a = attrs();
        let mut raw: Value = serde_json::from_str(&a[SOURCE]).unwrap();
        raw["set"]["cards"][0]["setting"]["voicing_fingerprint"] = Value::String("changed".into());
        let mut bad = a.clone();
        bad.insert(SOURCE.into(), raw.to_string());
        let before = bad.clone();
        assert!(write_music(&bad, 96).unwrap_err().contains("fret pattern"));
        assert_eq!(bad, before);
        raw["set"]["music"] = serde_json::json!({"musicxml":"<broken>"});
        bad.insert(SOURCE.into(), raw.to_string());
        assert!(music(&bad).is_err());
        assert!(write_music(&bad, 96).is_err());
        assert!(write_music(&a, 0).is_err());
    }

    #[test]
    fn saved_music_reading_does_not_require_legacy_card_or_arrangement_metadata() {
        use woodshedding::notation::{MusicDocument, Score};
        let score = Score::rhythm_example();
        let raw = serde_json::json!({
            "version": 1,
            "set": {
                "music": MusicDocument::from_score(&score).unwrap(),
                "cards": "unrecognized historical Card metadata",
                "progression_arrangement": {"version": 999},
                "future": {"keep": true}
            }
        });
        let attrs = BTreeMap::from([(SOURCE.into(), raw.to_string())]);
        let before = attrs.clone();
        assert_eq!(music(&attrs).unwrap(), Some(score));
        assert!(rows(&attrs).is_err());
        assert!(write_music(&attrs, 96).is_err());
        assert_eq!(attrs, before);
    }

    #[test]
    fn meter_patch_keeps_other_source_fields_and_refuses_invalid_boundaries() {
        let a = attrs();
        assert_eq!(
            meter_attributes(&a, &time_signatures(&a).unwrap(), &[]).unwrap(),
            a
        );
        let map = [
            TimeSignatureChange {
                start: 0,
                numerator: 4,
                denominator: 4,
            },
            TimeSignatureChange {
                start: 32,
                numerator: 3,
                denominator: 4,
            },
        ];
        let next = meter_attributes(&a, &map, &[]).unwrap();
        let original: Value = serde_json::from_str(&a[SOURCE]).unwrap();
        let mut raw: Value = serde_json::from_str(&next[SOURCE]).unwrap();
        raw["set"]["progression_arrangement"]["alignment"]
            .as_object_mut()
            .unwrap()
            .remove("time_signatures");
        raw["set"]["progression_arrangement"]["alignment"]
            .as_object_mut()
            .unwrap()
            .remove("bar_groups");
        let mut expected = original.clone();
        expected["set"]["progression_arrangement"]["alignment"]
            .as_object_mut()
            .unwrap()
            .remove("time_signatures");
        assert_eq!(raw, expected);
        assert_eq!(next["custom"], a["custom"]);
        let mut bad = map;
        bad[1].start = 24;
        assert!(meter_attributes(&a, &bad, &[]).is_err());
    }

    #[test]
    fn grouping_edits_only_group_metadata_and_uses_one_document_history() {
        let mut attrs = attrs();
        let mut raw: Value = serde_json::from_str(&attrs[SOURCE]).unwrap();
        raw["set"]["future_field"] = serde_json::json!({"keep":"unknown content"});
        attrs.insert(SOURCE.into(), raw.to_string());
        let selected = [0, 1, 2, 3];
        let changes = choices(&attrs, &selected, "2 + 2").unwrap();
        assert!(!changes[0].conflict);
        let updated = attributes(&attrs, &selected, &changes).unwrap();
        let mut new: Value = serde_json::from_str(&updated[SOURCE]).unwrap();
        let groups = new
            .pointer_mut("/set/progression_arrangement/alignment")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("bar_groups")
            .unwrap();
        assert_eq!(
            groups,
            serde_json::json!([{"first":0,"count":2},{"first":2,"count":2}])
        );
        assert_eq!(new, raw);
        assert_eq!(updated["custom"], "keep me");
        let source = illume::blocks::wrap(&attrs, "Woodshed arrangement\n");
        let mut doc = Document::new("test", &source);
        let div = doc.blocks().remove(0);
        let preview = Preview::new(&source, div, changes).unwrap();
        doc.apply(&preview, updated).unwrap();
        let applied = doc.editor.source().to_string();
        assert_ne!(applied, source);
        doc.editor.apply(TextCommand::Undo);
        assert_eq!(doc.editor.source(), source);
        doc.editor.apply(TextCommand::Redo);
        assert_eq!(doc.editor.source(), applied);
        assert!(doc.apply(&preview, attrs).is_err());
    }
    #[test]
    fn conflicting_groups_keep_local_until_accepted_and_noop_keeps_exact_bytes() {
        let a = attrs();
        let selected = [0, 1, 2, 3];
        let a = attributes(&a, &selected, &choices(&a, &selected, "2 2").unwrap()).unwrap();
        let mut changes = choices(&a, &selected, "1 3").unwrap();
        assert!(changes[0].conflict);
        assert!(!changes[0].use_update);
        assert_eq!(attributes(&a, &selected, &changes).unwrap(), a);
        changes[0].use_update = true;
        let b = attributes(&a, &selected, &changes).unwrap();
        assert_ne!(b, a);
        assert!(choices(&b, &[0, 1], "2").is_err());
    }
    #[test]
    fn scene_and_plaintext_use_actual_set_with_explicit_rest_and_meter() {
        let a = attrs();
        assert_eq!(rows(&a).unwrap().len(), 4);
        let input = scene("document#passage", "revision", &a).unwrap();
        input.validate().unwrap();
        let SceneFacts::TemplatedSequence(sequence) = input.facts else {
            panic!()
        };
        assert_eq!(sequence.occurrences.len(), 5);
        assert_eq!(sequence.template.groups.len(), 4);
        let text = plaintext(&a).unwrap();
        assert!(text.contains("4/4 |"));
        assert!(text.contains(" r |"));
        assert_eq!(text.matches('|').count(), 5);
        let mut raw: Value = serde_json::from_str(&a[SOURCE]).unwrap();
        raw["set"]["progression_arrangement"]["alignment"]["time_signatures"] =
            serde_json::json!([]);
        let mut missing = a;
        missing.insert(SOURCE.into(), raw.to_string());
        assert!(rows(&missing).unwrap().is_empty());
        assert!(plaintext(&missing).is_err());
        assert!(choices(&missing, &[], "2 2").is_err());
    }
}
