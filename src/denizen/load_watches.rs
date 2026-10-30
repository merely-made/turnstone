// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Read back [`save_watches`]. Absent or malformed yields empty tables and
/// says so: behaviors that stop waking are worth a line in the log, and an
/// unreadable file should not stop a session opening.
pub fn load_watches(
    session_dir: &Path,
) -> (
    servitor::WatchTable,
    servitor::WatchTable,
    servitor::TimeWatchTable,
    servitor::DeadbandTable,
) {
    let empty = || {
        (
            servitor::WatchTable::new(),
            servitor::WatchTable::new(),
            servitor::TimeWatchTable::new(),
            servitor::DeadbandTable::new(),
        )
    };
    let target = watches_path(session_dir);
    let Ok(text) = std::fs::read_to_string(&target) else {
        return empty();
    };
    let mut graph_lines = Vec::new();
    let mut app_lines = Vec::new();
    let mut time_lines = Vec::new();
    let mut deadband_lines = Vec::new();
    for line in text.lines() {
        match line.split_once(' ') {
            Some(("graph", rest)) => graph_lines.push(rest.to_string()),
            Some(("app", rest)) => app_lines.push(rest.to_string()),
            Some(("time", rest)) => time_lines.push(rest.to_string()),
            Some(("deadband", rest)) => deadband_lines.push(rest.to_string()),
            _ if line.trim().is_empty() => {},
            _ => {
                tracing::warn!(path = ?target, "unreadable watch record; watches not restored");
                return empty();
            },
        }
    }
    let graph = servitor::WatchTable::from_wire_lines(graph_lines.iter().map(String::as_str));
    let app = servitor::WatchTable::from_wire_lines(app_lines.iter().map(String::as_str));
    let time = servitor::TimeWatchTable::from_wire_lines(time_lines.iter().map(String::as_str));
    let deadbands =
        servitor::DeadbandTable::from_wire_lines(deadband_lines.iter().map(String::as_str));
    match (graph, app, time, deadbands) {
        (Ok(graph), Ok(app), Some(time), Some(deadbands)) => (graph, app, time, deadbands),
        _ => {
            tracing::warn!(path = ?target, "malformed watch table; watches not restored");
            empty()
        },
    }
}
