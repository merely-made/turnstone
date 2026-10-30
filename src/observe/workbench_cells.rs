// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The workbench's cells as observation strings: each cell's tab labels (the
/// node's display label off graph truth) joined by `|`, `*` on the active tab.
pub fn workbench_cells(app: &App) -> Vec<String> {
    let label_of = |member: uuid::Uuid| -> String {
        app.graph_runtimes
            .graph()
            .nodes()
            .find(|(_, n)| n.id == member)
            .map(|(_, n)| {
                if n.title.trim().is_empty() {
                    n.url().to_string()
                } else {
                    n.title.clone()
                }
            })
            .unwrap_or_else(|| member.to_string())
    };
    app.active_workbench()
        .into_iter()
        .flat_map(|workbench| workbench.slot_views())
        .map(|slot| {
            slot.members
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let label = label_of(*m);
                    if i == slot.active {
                        format!("{label}*")
                    } else {
                        label
                    }
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}
