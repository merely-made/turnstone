// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn shared_scenario_from_env() -> Option<taproot::Scenario> {
    let path = std::path::PathBuf::from(std::env::var_os("TURNSTONE_SCENARIO")?);
    let body = std::fs::read_to_string(&path).unwrap_or_default();
    // A parse error becomes a scenario that logs why and fails a step (an
    // assert on a field no snapshot has), so the run reports RESULT fail with the
    // reason rather than timing out — the same courtesy turnstone's own driver pays.
    let parsed = body
        .lines()
        .enumerate()
        .map(|(index, line)| crate::scenario::expand_env(line, index + 1))
        .collect::<Result<Vec<_>, _>>()
        .and_then(|lines| taproot::Scenario::parse(&lines.join("\n")));
    Some(match parsed {
        Ok(sc) => sc,
        Err(err) => {
            let fallback = format!("log parse error: {err}\nassert snap __never__ == 1");
            taproot::Scenario::parse(&fallback).expect("fallback scenario parses")
        },
    })
}
