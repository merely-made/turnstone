// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Persist the three watch tables and the actuation deadband table.
///
/// **Persisted rather than re-derived from the pack sources**, which would
/// also have worked and would have kept one source of truth. The reason is
/// what re-deriving loses: a rebuilt graph watch restarts its cursor at zero
/// and would re-wake on the whole journal it had already considered, and a
/// rebuilt schedule restarts its period, so a daily behavior reinstalled by a
/// restart never fires for anyone who reopens their session each morning.
/// Cursors and phase are state, not declaration.
///
/// Tagged lines, because the four tables share one file and their records are
/// not distinguishable by shape alone.
pub fn save_watches(
    session_dir: &Path,
    graph: &servitor::WatchTable,
    app: &servitor::WatchTable,
    time: &servitor::TimeWatchTable,
    deadbands: &servitor::DeadbandTable,
) {
    let mut lines: Vec<String> = Vec::new();
    lines.extend(
        graph
            .to_wire_lines()
            .into_iter()
            .map(|l| format!("graph {l}")),
    );
    lines.extend(app.to_wire_lines().into_iter().map(|l| format!("app {l}")));
    lines.extend(
        time.to_wire_lines()
            .into_iter()
            .map(|l| format!("time {l}")),
    );
    lines.extend(
        deadbands
            .to_wire_lines()
            .into_iter()
            .map(|l| format!("deadband {l}")),
    );
    let target = watches_path(session_dir);
    let result = (|| -> std::io::Result<()> {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            &target,
            lines.join(
                "
",
            ),
        )
    })();
    if let Err(err) = result {
        tracing::warn!(%err, path = ?target, "failed to persist participant watches");
    }
}
