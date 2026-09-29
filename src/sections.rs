// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The section-provider registry: named, id-addressable list sections any pane
//! can compose (the gloss-composite design, design_docs/2026-07-20_gloss_
//! composite_pane.md). A provider is a pure `fn(&App) -> Vec<SectionRow>` — the
//! same currency as a swatch [`ProjectionPreset`](crate::swatch_pane::
//! ProjectionPreset)'s `gather`, so a pane pulls presets and sections the same
//! way. That parallel is deliberate: the swatch half's author left it as the
//! seam for this half.
//!
//! Slice 1 (2026-07-22): the providers + display rows, and the Gloss pane
//! renders them below its minimap. A row's activation (click to navigate /
//! recover), the per-frisket-leaf config (which sections a pane shows), and the
//! add/remove UI (the right-click palette scoped to the active pane) are the
//! follow-on slices the design records.

use crate::app::App;

/// What clicking a section row means. Data, like a swatch node's activation:
/// providers declare it and the host lowers it through the spine, so a new
/// provider needs no handler code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SectionActivate {
    /// Navigate to this url (a Recent row).
    Open(String),
    /// Recover this removed node by its ORIGINAL id (a Removed row).
    Recover(uuid::Uuid),
}

/// One row of a composed section: its display text plus what a click does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionRow {
    pub text: String,
    /// `None` renders an inert row (an empty-state hint).
    pub activate: Option<SectionActivate>,
}

/// A named list-section provider. The stable `id` is what a pane's config
/// addresses (`"recent"`, `"removed"`); `title` is the rendered header;
/// `gather` reads app truth, pure.
#[derive(Clone, Copy)]
pub struct SectionProvider {
    pub id: &'static str,
    pub title: &'static str,
    pub gather: fn(&App) -> Vec<SectionRow>,
}

/// Graph-wide recently-visited urls, newest first — the Trail's Recent section,
/// now composable into any pane.
pub const RECENT_SECTION: SectionProvider = SectionProvider {
    id: "recent",
    title: "Recent",
    gather: gather_recent,
};

/// The recycle bin's removed nodes (records whose node is absent from the
/// graph) — the Trail's Removed section, composable. A recovered node is
/// present again and derives out, exactly as in the Trail.
pub const REMOVED_SECTION: SectionProvider = SectionProvider {
    id: "removed",
    title: "Removed",
    gather: gather_removed,
};

/// The graph's nodes, most-recently-visited first — the Roster's bucket, made
/// composable. This is the design's own example ("a roster bucket beside the
/// minimap"): the section a pane borrows from a pane it is not.
pub const NODES_SECTION: SectionProvider = SectionProvider {
    id: "nodes",
    title: "Nodes",
    gather: gather_nodes,
};

/// Durable downloads, composable beside navigation in Gloss.
pub const DOWNLOADS_SECTION: SectionProvider = SectionProvider {
    id: "downloads",
    title: "Downloads",
    gather: gather_downloads,
};

/// Every provider, for id lookup (the config resolves an id to its provider).
pub const ALL: &[SectionProvider] = &[
    RECENT_SECTION,
    REMOVED_SECTION,
    NODES_SECTION,
    DOWNLOADS_SECTION,
];

/// The provider with this id, if any.
pub fn by_id(id: &str) -> Option<&'static SectionProvider> {
    ALL.iter().find(|p| p.id == id)
}

/// Resolve a pane config's section ids to providers, in the config's order.
/// An id this build does not know is SKIPPED, not an error: a layout written
/// by a newer build (or one whose provider was retired) degrades to the
/// sections that still exist rather than failing the pane.
pub fn resolve(ids: &[String]) -> Vec<SectionProvider> {
    ids.iter().filter_map(|id| by_id(id).copied()).collect()
}

/// Durable download custody from graph facets, formerly Steward's projection.
/// Rows remain read-only; custody and completion stay on the product spine.
fn gather_downloads(app: &App) -> Vec<SectionRow> {
    let facet = chartulary::FacetId::new(crate::content_classes::DOWNLOAD_FACET);
    let mut rows = app
        .graph_runtimes
        .graph()
        .nodes()
        .filter_map(|(_, node)| {
            let record = app.graph_runtimes.facets().get(&node.id, &facet)?;
            let status = record.get("status")?.as_str().unwrap_or("unknown");
            let received_at_ms = record
                .get("received_at_ms")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let bytes = record
                .get("byte_size")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let detail = record
                .get("destination_path")
                .or_else(|| record.get("error"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_else(|| node.url());
            let title = if node.title.trim().is_empty() {
                node.url()
            } else {
                node.title.trim()
            };
            Some((
                received_at_ms,
                SectionRow {
                    text: format!("{title} - {status} - {bytes} bytes - {detail}"),
                    activate: None,
                },
            ))
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|(received_at_ms, _)| std::cmp::Reverse(*received_at_ms));
    rows.into_iter().map(|(_, row)| row).collect()
}

fn gather_recent(app: &App) -> Vec<SectionRow> {
    app.graph_runtimes
        .graph()
        .recent_visited(8)
        .into_iter()
        .map(|rv| SectionRow {
            text: mere::trail::short_url(&rv.url),
            activate: Some(SectionActivate::Open(rv.url)),
        })
        .collect()
}

fn gather_nodes(app: &App) -> Vec<SectionRow> {
    let graph = app.graph_runtimes.graph();
    let mut rows: Vec<(std::time::SystemTime, SectionRow)> = graph
        .nodes()
        .map(|(key, node)| {
            let url = node.url().to_string();
            (
                graph
                    .node_last_visited(key)
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                SectionRow {
                    text: graph.node_display_label(key),
                    activate: Some(SectionActivate::Open(url)),
                },
            )
        })
        .collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    rows.into_iter().take(8).map(|(_, row)| row).collect()
}

fn gather_removed(app: &App) -> Vec<SectionRow> {
    let graph = app.graph_runtimes.graph();
    let mut seen = std::collections::HashSet::new();
    app.removed
        .iter()
        .filter(|r| graph.get_node_key_by_id(r.node_id).is_none())
        .filter(|r| seen.insert(r.node_id))
        .map(|r| SectionRow {
            // The affordance IS the label, as in the Trail: a Removed row must
            // not read identically to the same url's Recent row, or a
            // text-addressed click cannot tell recover from navigate.
            text: format!("Recover {}", mere::trail::short_url(&r.url)),
            activate: Some(SectionActivate::Recover(r.node_id)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{RemovedRecord, Update};

    #[test]
    fn by_id_resolves_the_registered_providers() {
        assert_eq!(by_id("removed").map(|p| p.title), Some("Removed"));
        assert_eq!(by_id("recent").map(|p| p.title), Some("Recent"));
        assert_eq!(by_id("downloads").map(|p| p.title), Some("Downloads"));
        assert!(by_id("nope").is_none());
    }

    #[test]
    fn removed_section_gathers_absent_bin_records_only() {
        let mut app = App::test_stub();
        let id = uuid::Uuid::new_v4();
        // A record whose node is absent from the graph is a removed row.
        app.apply_update(Update::BinListed {
            records: vec![RemovedRecord {
                node_id: id,
                url: "https://gone.test/page".into(),
                title: None,
                tags: Vec::new(),
                deleted_at_ms: 1,
                nested: None,
                facets: None,
            }],
        });
        let rows = (REMOVED_SECTION.gather)(&app);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].text.contains("gone.test"));

        // Recovering re-mints the node under its ORIGINAL id -> present -> the
        // section derives it away (the Trail's rule, shared). A plain open
        // would NOT, since it mints a new id and the tombstoned one stays.
        app.update(crate::action::Action::RecoverDeletedNode(id));
        assert!((REMOVED_SECTION.gather)(&app).is_empty());
    }
    #[test]
    fn completed_download_projects_from_durable_graph_facets() {
        let mut app = crate::app::App::test_stub();
        let key = app
            .graph_runtimes
            .visit("gemini://capsule.test/archive.bin");
        let node = app.graph_runtimes.graph().get_node(key).unwrap().id;
        crate::content_classes::set_download_record(
            &mut app.graph_runtimes,
            node,
            crate::content_classes::DownloadFacetRecord {
                source_url: "gemini://capsule.test/archive.bin",
                received_at_ms: 42,
                byte_size: 12,
                status: "completed",
                media_type: Some("application/octet-stream"),
                content_disposition: None,
                destination_path: Some("C:\\Downloads\\archive.bin"),
                content_hash: Some(&"22".repeat(32)),
                error: None,
            },
        )
        .unwrap();

        let rows = (DOWNLOADS_SECTION.gather)(&app);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].activate, None, "custody rows remain read-only");
        assert!(rows[0].text.contains("C:\\Downloads\\archive.bin"));
        let mut pane = crate::swatch_pane::SwatchPane::new(crate::swatch_pane::GLOSS_MINIMAP);
        pane.set_sections(vec![DOWNLOADS_SECTION]);
        pane.sync(&app, 640.0, 480.0);
        assert!(
            pane.resolve(
                &taproot::Selector::class("section-row")
                    .containing("archive.bin - completed - 12 bytes"),
                [0.0, 0.0, 640.0, 480.0]
            )
            .is_some()
        );
    }
    #[test]
    fn download_status_and_error_rows_keep_custody_order_and_inert_activation() {
        let mut app = App::test_stub();
        for (name, timestamp, status, error) in [
            ("older.bin", 1, "failed", Some("permission denied")),
            ("pending.bin", 2, "storing", None),
        ] {
            let url = format!("gemini://capsule.test/{name}");
            let key = app.graph_runtimes.visit(&url);
            let node = app.graph_runtimes.graph().get_node(key).unwrap().id;
            crate::content_classes::set_download_record(&mut app.graph_runtimes, node,
                crate::content_classes::DownloadFacetRecord {
                    source_url: &url, received_at_ms: timestamp, byte_size: 12,
                    status, media_type: None, content_disposition: None,
                    destination_path: None, content_hash: None, error,
                }).unwrap();
        }
        let rows = (DOWNLOADS_SECTION.gather)(&app);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].text.contains("pending.bin - storing"));
        assert!(rows[1].text.contains("older.bin - failed"));
        assert!(rows[1].text.contains("permission denied"));
        assert!(rows.iter().all(|row| row.activate.is_none()));
    }
}
