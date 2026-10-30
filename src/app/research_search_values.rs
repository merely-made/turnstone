// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::research_search_state::{Preferences, State};

pub(super) fn apply_value(
    state: &mut State,
    root: &std::path::Path,
    field: &str,
    value: &str,
) -> Result<(), String> {
    if field == "date_from" || field == "date_to" {
        let parsed = if value.trim().is_empty() {
            None
        } else {
            let text = value.trim();
            let date: jiff::civil::Date = text
                .parse()
                .map_err(|_| "Enter a date as YYYY-MM-DD".to_string())?;
            if date.to_string() != text {
                return Err("Enter a date as YYYY-MM-DD".into());
            }
            let boundary = if field == "date_to" {
                date.tomorrow().map_err(|_| "Date is out of range")?
            } else {
                date
            };
            let millis = boundary
                .to_zoned(jiff::tz::TimeZone::UTC)
                .map_err(|_| "Date is out of range")?
                .timestamp()
                .as_millisecond();
            Some(if field == "date_to" {
                millis.checked_sub(1).ok_or("Date is out of range")?
            } else {
                millis
            })
        };
        let (from, to) = if field == "date_from" {
            (parsed, state.query.date_to)
        } else {
            (state.query.date_from, parsed)
        };
        if from.zip(to).is_some_and(|(a, b)| a > b) {
            return Err("Start date is later than end date".into());
        }
        state.query.date_from = from;
        state.query.date_to = to;
        return Ok(());
    }
    let mut prefs = Preferences {
        version: 1,
        settings: state.preferences.settings.clone(),
        aliases: state.preferences.aliases.clone(),
        comparison: state.preferences.comparison.clone(),
        records: state.preferences.records.clone(),
    };
    match field {
        "aliases" => {
            prefs.aliases = if value.trim().is_empty() {
                vec![]
            } else {
                value
                    .lines()
                    .map(|line| {
                        let (term, expansions) = line
                            .split_once('=')
                            .ok_or("Use one term = alternative, alternative per line")?;
                        Ok(eidetic_search::query::Alias {
                            term: term.trim().into(),
                            expansions: expansions
                                .split(',')
                                .map(|v| v.trim().to_owned())
                                .collect(),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?
            };
        },
        "weight_url" | "weight_title" | "weight_text" => {
            let n: f32 = value
                .trim()
                .parse()
                .map_err(|_| "Enter a positive finite weight")?;
            match field {
                "weight_url" => prefs.settings.weights.url = n,
                "weight_title" => prefs.settings.weights.title = n,
                _ => prefs.settings.weights.text = n,
            }
        },
        _ => {
            let n: usize = value
                .trim()
                .parse()
                .map_err(|_| "Enter a nonnegative whole number")?;
            match field {
                "record_max_sources" => prefs.records.max_sources = n,
                "record_max_records" => prefs.records.max_records = n,
                "record_max_work" => prefs.records.max_work = n,
                "record_max_validation_bytes" => prefs.records.max_validation_bytes = n,
                "record_max_rows" => prefs.records.max_rows = n,
                "record_max_input_bytes" => prefs.records.max_input_bytes = n,
                "record_max_output_bytes" => prefs.records.max_output_bytes = n,
                "compare_max_input_bytes" => prefs.comparison.max_input_bytes = n,
                "compare_max_identity_bytes" => prefs.comparison.max_identity_bytes = n,
                "compare_max_diff_work" => prefs.comparison.max_diff_work = n,
                "compare_max_output_spans" => prefs.comparison.max_output_spans = n,
                "limit" => prefs.settings.limit = n,
                "snippet_context" => prefs.settings.snippet_context = n,
                "max_sources" => prefs.settings.max_sources = n,
                "max_corpus_chars" => prefs.settings.max_corpus_chars = n,
                "max_metadata_chars" => prefs.settings.max_metadata_chars = n,
                "max_query_chars" => prefs.settings.max_query_chars = n,
                _ => return Err("Unknown search setting".into()),
            }
        },
    }
    prefs.save(root)?;
    state.preferences = prefs;
    Ok(())
}
