// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use crate::action::Effect;
use crate::app::App;
use std::collections::HashMap;
use uuid::Uuid;

struct PendingReader {
    url: String,
    session: crate::panes::SessionId,
    generation: Uuid,
    graph: crate::panes::GraphId,
    had_pane: bool,
}

#[derive(Default)]
pub(super) struct ReaderRecovery {
    pending: HashMap<Uuid, PendingReader>,
}

fn reader_panes(app: &App, node: Uuid, graph: crate::panes::GraphId) -> Vec<crate::panes::PaneId> {
    app.frisket
        .iter_leaves()
        .chain(
            app.lenses
                .iter()
                .flatten()
                .flat_map(|layout| layout.iter_leaves()),
        )
        .filter_map(|(pane, content, pane_graph)| {
            (pane_graph == graph
                && match content {
                    crate::panes::PaneContent::Workbench => app
                        .workbench_for_pane(pane)
                        .is_some_and(|workbench| workbench.has_tile(node)),
                    crate::panes::PaneContent::Tile(member) => *member == node,
                    _ => false,
                })
            .then_some(pane)
        })
        .collect()
}

impl ReaderRecovery {
    pub(super) fn defer(&mut self, app: &mut App, node: Uuid, url: &str) -> bool {
        let Some(graph) = app.graph_runtimes.graph_containing_member(node) else {
            return false;
        };
        let Some(runtime) = app.graph_runtimes.get(graph) else {
            return false;
        };
        if runtime.session != Some(app.session_id)
            || !crate::browse::still_current(&runtime.canvas, node, url)
        {
            return false;
        }
        self.pending.insert(
            node,
            PendingReader {
                url: url.to_owned(),
                session: app.session_id,
                generation: app.source_capture_generation,
                graph,
                had_pane: !reader_panes(app, node, graph).is_empty(),
            },
        );
        app.content.note_awaiting_custody(node);
        true
    }

    pub(super) fn prune(&mut self, app: &mut App) {
        let mut abandoned = Vec::new();
        self.pending.retain(|node, pending| {
            let keep = pending.session == app.session_id
                && pending.generation == app.source_capture_generation
                && matches!(
                    app.content.get(*node),
                    Some(crate::content::NodeContent::AwaitingCustody)
                )
                && app
                    .graph_runtimes
                    .get(pending.graph)
                    .is_some_and(|runtime| {
                        runtime.session == Some(pending.session)
                            && crate::browse::still_current(&runtime.canvas, *node, &pending.url)
                    })
                && (!pending.had_pane || !reader_panes(app, *node, pending.graph).is_empty());
            if !keep {
                abandoned.push(*node);
            }
            keep
        });
        for node in abandoned {
            Self::cancel_wait(app, node);
        }
    }

    fn cancel_wait(app: &mut App, node: Uuid) {
        if matches!(
            app.content.get(node),
            Some(crate::content::NodeContent::AwaitingCustody)
        ) {
            app.content.note_closed(node);
        }
    }

    pub(super) fn remove(&mut self, app: &mut App, node: Uuid) {
        if self.pending.remove(&node).is_some() {
            Self::cancel_wait(app, node);
        }
    }

    pub(super) fn failed(&mut self, app: &mut App) -> Vec<Uuid> {
        self.prune(app);
        self.pending.drain().map(|(node, _)| node).collect()
    }

    pub(super) fn clear(&mut self, app: &mut App) {
        for (node, _) in self.pending.drain() {
            Self::cancel_wait(app, node);
        }
    }

    pub(super) fn ready(&mut self, app: &mut App) -> Vec<Effect> {
        self.prune(app);
        self.pending
            .drain()
            .map(|(node, pending)| Effect::SpawnContent {
                node,
                url: pending.url,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;

    fn app() -> (tempfile::TempDir, App) {
        let root = tempfile::tempdir().unwrap();
        let mut app = App::test_stub_at(root.path().to_path_buf());
        app.sessions =
            crate::session::manifest_store::ManifestStore::with_root(crate::session::sessions_root(root.path()));
        let id = app
            .update(Action::NewSession)
            .into_iter()
            .find_map(|effect| match effect {
                Effect::SwitchSession { id } => Some(id),
                _ => None,
            })
            .unwrap();
        app.adopt_session(id);
        app.identity = None;
        (root, app)
    }

    fn reader(app: &mut App, url: &str) -> Uuid {
        let key = app.graph_runtimes.visit(url);
        let node = app.graph_runtimes.graph().get_node(key).unwrap().id;
        app.content.note_awaiting_custody(node);
        node
    }

    fn saved_url(app: &App) -> String {
        crate::saved_sources::address(&crate::saved_sources::Target {
            key: crate::saved_sources::Key {
                session: app.session_id,
                graph: app.graph_runtimes.active_graph(),
                node: Uuid::new_v4(),
                annotation_manifest: eidetic::Hash::of(b"capture").to_string(),
            },
            text_hash: eidetic::Hash::of(b"text").to_string(),
            selector: Some(crate::saved_sources::Selector { start: 0, end: 1 }),
        })
        .unwrap()
    }

    #[test]
    fn pending_readers_replay_only_after_ready_and_keep_exact_urls() {
        let (_root, mut app) = app();
        let url = saved_url(&app);
        let node = reader(&mut app, &url);
        let mut recovery = ReaderRecovery::default();
        assert!(recovery.defer(&mut app, node, &url));
        assert_eq!(recovery.pending.len(), 1);
        assert!(
            crate::observe::snapshot(&app)
                .content
                .iter()
                .any(|(member, state)| *member == node && state == "awaiting-custody")
        );
        let effects = recovery.ready(&mut app);
        assert!(effects.iter().any(|effect| matches!(effect,
            Effect::SpawnContent { node: actual, url: actual_url }
                if *actual == node && actual_url == &url)));
        assert!(recovery.ready(&mut app).is_empty());
    }

    #[test]
    fn custody_wait_clears_stale_live_facts() {
        use crate::content::{
            CapabilityStatus, ContentFacts, ContentLane, DocumentCapabilityFacts,
        };
        let (_root, mut app) = app();
        let url = saved_url(&app);
        let node = reader(&mut app, &url);
        app.content.note_live(
            node,
            Some(ContentFacts {
                engine: "old".into(),
                lane: ContentLane::Document,
                structure: None,
                lineage: None,
                capabilities: DocumentCapabilityFacts {
                    find_in_page: CapabilityStatus::Supported,
                    page_zoom: CapabilityStatus::Supported,
                    page_capture: CapabilityStatus::Supported,
                    navigation: CapabilityStatus::Supported,
                },
            }),
        );
        assert!(app.content.facts(node).is_some());
        app.content.note_awaiting_custody(node);
        assert!(app.content.facts(node).is_none());
    }

    #[test]
    fn retained_clip_waits_for_ready_with_exact_manifest_url() {
        let (_root, mut app) = app();
        let url = crate::retained_clip::address(eidetic::ManifestId::of_blob(b"clip"));
        let node = reader(&mut app, &url);
        let mut recovery = ReaderRecovery::default();
        assert!(recovery.defer(&mut app, node, &url));
        let effects = recovery.ready(&mut app);
        assert!(effects.iter().any(|effect| matches!(effect,
            Effect::SpawnContent { node: actual, url: actual_url }
                if *actual == node && actual_url == &url)));
    }

    #[test]
    fn closed_workbench_tile_is_not_reopened() {
        let (_root, mut app) = app();
        let url = saved_url(&app);
        let node = reader(&mut app, &url);
        app.update(Action::OpenInWorkbench);
        let pane = app
            .frisket
            .iter_leaves()
            .find_map(|(pane, content, _)| {
                matches!(content, crate::panes::PaneContent::Workbench).then_some(pane)
            })
            .unwrap();
        assert!(app.workbench_for_pane(pane).unwrap().has_tile(node));
        let mut recovery = ReaderRecovery::default();
        assert!(recovery.defer(&mut app, node, &url));
        app.workbench_for_pane_mut(pane).unwrap().close_tile(node);
        assert!(recovery.ready(&mut app).is_empty());
        let graph_pane = app.default_graph_pane();
        app.graph_pane_select_member(graph_pane, node);
        app.focus = crate::surface::FocusTarget::Graph(graph_pane);
        let reopened = app.update(Action::OpenInWorkbench);
        assert!(reopened.iter().any(|effect| matches!(effect,
            Effect::SpawnContent { node: actual, url: actual_url }
                if *actual == node && actual_url == &url)));
    }

    #[test]
    fn explicit_graph_open_waits_without_saved_sidecar() {
        let (_root, mut app) = app();
        let url = saved_url(&app);
        let node = reader(&mut app, &url);
        app.content.note_closed(node);
        app.browser.entry(node).content_on = false;
        let graph_pane = app.default_graph_pane();
        app.graph_pane_select_member(graph_pane, node);
        app.focus = crate::surface::FocusTarget::Graph(graph_pane);
        assert!(app.update(Action::ToggleNodeContent).iter().any(|effect| matches!(effect,
            Effect::SpawnContent { node: actual, url: actual_url } if *actual == node && actual_url == &url)));
        let mut recovery = ReaderRecovery::default();
        assert!(recovery.defer(&mut app, node, &url));
        assert!(recovery.ready(&mut app).iter().any(|effect| matches!(effect,
            Effect::SpawnContent { node: actual, url: actual_url } if *actual == node && actual_url == &url)));
        assert!(recovery.defer(&mut app, node, &url));
        assert!(
            app.update(Action::ToggleNodeContent)
                .iter()
                .any(|effect| matches!(effect,
            Effect::CloseContent { node: actual } if *actual == node))
        );
        recovery.prune(&mut app);
        assert!(app.content.get(node).is_none());
        assert!(recovery.ready(&mut app).is_empty());
    }

    #[test]
    fn tear_out_preserves_pending_reader() {
        let (_root, mut app) = app();
        let url = saved_url(&app);
        let node = reader(&mut app, &url);
        app.update(Action::OpenInWorkbench);
        let mut recovery = ReaderRecovery::default();
        assert!(recovery.defer(&mut app, node, &url));
        app.update(Action::TearOutTile { member: node });
        assert!(app.lenses.iter().flatten().flat_map(|layout| layout.iter_leaves())
            .any(|(_, content, _)| matches!(content, crate::panes::PaneContent::Tile(member) if *member == node)));
        assert!(
            recovery
                .ready(&mut app)
                .iter()
                .any(|effect| matches!(effect,
            Effect::SpawnContent { node: actual, url: actual_url }
                if *actual == node && actual_url == &url))
        );
    }

    #[test]
    fn cancellation_preserves_newer_content_and_clears_owned_waits() {
        let (_root, mut app) = app();
        let url = saved_url(&app);
        let node = reader(&mut app, &url);
        let mut recovery = ReaderRecovery::default();
        assert!(recovery.defer(&mut app, node, &url));
        app.content.note_requested(node);
        recovery.clear(&mut app);
        assert!(matches!(
            app.content.get(node),
            Some(crate::content::NodeContent::Requested)
        ));
        assert!(recovery.defer(&mut app, node, &url));
        assert!(recovery.defer(&mut app, node, &url));
        assert_eq!(recovery.ready(&mut app).len(), 1);
        assert!(recovery.defer(&mut app, node, &url));
        recovery.clear(&mut app);
        assert!(app.content.get(node).is_none());
    }

    #[test]
    fn navigation_and_failed_barrier_discard_pending_readers() {
        let (_root, mut app) = app();
        let url = saved_url(&app);
        let node = reader(&mut app, &url);
        let mut recovery = ReaderRecovery::default();
        assert!(recovery.defer(&mut app, node, &url));
        let graph = app.graph_runtimes.active_graph();
        app.graph_runtimes
            .canvas_mut(graph)
            .unwrap()
            .navigate_member(node, "https://other.example");
        assert!(recovery.ready(&mut app).is_empty());
        let next = reader(&mut app, &url);
        assert!(recovery.defer(&mut app, next, &url));
        assert_eq!(recovery.failed(&mut app), vec![next]);
        assert!(recovery.ready(&mut app).is_empty());
    }

    #[test]
    fn foreign_graph_session_is_never_replayed() {
        let (_root, mut app) = app();
        let url = saved_url(&app);
        let node = reader(&mut app, &url);
        let mut recovery = ReaderRecovery::default();
        assert!(recovery.defer(&mut app, node, &url));
        let graph = app.graph_runtimes.active_graph();
        app.graph_runtimes.get_mut(graph).unwrap().session = Some(crate::panes::SessionId::new());
        assert!(recovery.ready(&mut app).is_empty());
        assert!(!recovery.defer(&mut app, node, &url));
    }

    #[test]
    fn stale_closed_and_changed_generation_do_not_replay() {
        let (_root, mut app) = app();
        let url = saved_url(&app);
        let node = reader(&mut app, &url);
        let mut recovery = ReaderRecovery::default();
        assert!(recovery.defer(&mut app, node, &url));
        app.content.note_closed(node);
        assert!(recovery.ready(&mut app).is_empty());
        assert!(recovery.defer(&mut app, node, &url));
        app.source_capture_generation = Uuid::new_v4();
        assert!(recovery.ready(&mut app).is_empty());
        assert!(recovery.defer(&mut app, node, &url));
        app.session_id = crate::panes::SessionId::new();
        assert!(recovery.ready(&mut app).is_empty());
    }
}
