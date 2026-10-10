// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Per-view control contracts on the resource-aware supplier.
use super::*;
use crate::action::FetchedPage;
use crate::surface_controls::{self, Control};
use mere::kernel::graph::{Graph, apply::add_node};
use uuid::Uuid;
fn fixture() -> (App, Uuid, Uuid, Uuid) {
    let mut graph = Graph::new();
    let first = Uuid::from_u128(701);
    let second = Uuid::from_u128(702);
    let a = add_node(
        &mut graph,
        Some(first),
        "https://podcast.test/feed.xml".into(),
        Default::default(),
    );
    let b = add_node(
        &mut graph,
        Some(second),
        "https://podcast.test/feed.xml".into(),
        Default::default(),
    );
    let resource = graph.shown_resource_id(a).unwrap();
    assert_eq!(graph.shown_resource_id(b), Some(resource));
    let mut app = App::test_stub();
    app.graph_runtimes.set_graph(graph);
    let pane = app.default_graph_pane();
    assert!(app.graph_pane_select_member(pane, first));
    (app, first, second, resource)
}
#[test]
fn surface_controls_keep_is_local_idempotent_and_durable() {
    let (mut app, first, second, resource) = fixture();
    assert_eq!(
        app.update(Action::KeepNode { member: first }),
        vec![Effect::SaveSession, Effect::Redraw]
    );
    assert!(app.node_is_kept(first));
    assert!(!app.node_is_kept(second));
    assert_eq!(
        app.update(Action::KeepNode { member: first }),
        vec![Effect::Redraw]
    );
    assert!(
        app.graph_runtimes
            .graph()
            .resource_tag_labels(resource)
            .is_empty()
    );
    let root = tempfile::tempdir().unwrap();
    session::save_session_graph(root.path(), app.graph_runtimes.graph());
    session::save_node_facets(root.path(), app.graph_runtimes.facets());
    let reloaded = session::load_session_graph(root.path()).unwrap();
    assert!(surface_controls::read(&reloaded, first, Control::Keep));
    assert!(!surface_controls::read(&reloaded, second, Control::Keep));
    assert_eq!(
        reloaded.nodes().count(),
        2,
        "Keep does not add neighboring content"
    );
}
#[test]
fn surface_controls_resource_label_is_not_membership_evidence() {
    use mere::kernel::graph::{
        Author,
        resource_tags::{TagConcept, tag_concept_iri},
    };
    let (mut app, first, second, resource) = fixture();
    let mut graph = Graph::from_snapshot(&app.graph_runtimes.graph().to_snapshot());
    graph.write_as(Author::person("descriptive-label"), |graph| {
        let owner = graph.write_author().asserter_iri();
        graph
            .tag_resource_with_concept(
                resource,
                &tag_concept_iri(&owner, "keep"),
                TagConcept {
                    owner_iri: owner,
                    label: "keep".into(),
                },
            )
            .unwrap();
    });
    app.graph_runtimes.set_graph(graph);
    assert!(!app.node_is_kept(first));
    assert!(!app.node_is_kept(second));
    let root = tempfile::tempdir().unwrap();
    session::save_session_graph(root.path(), app.graph_runtimes.graph());
    let reloaded = session::load_session_graph(root.path()).unwrap();
    assert!(!surface_controls::read(&reloaded, first, Control::Keep));
    assert!(!surface_controls::read(&reloaded, second, Control::Keep));
    assert!(reloaded.resource_tag_labels(resource).contains("keep"));
}
#[test]
fn surface_controls_duplicate_feed_views_have_independent_read_state() {
    let (mut app, first, second, resource) = fixture();
    let pane = app.default_graph_pane();
    let fetched = crate::action::FetchedPage::text(
        Some("application/rss+xml".into()),
        r#"<rss version="2.0"><channel><title>Pod</title><item><guid>episode-1</guid><title>One</title><link>https://podcast.test/one</link></item></channel></rss>"#,
    );
    let mut entries = Vec::new();
    for source in [first, second] {
        assert!(app.graph_pane_select_member(pane, source));
        app.update(Action::SubscribeFocusedFeed {
            period: servitor::Period::Hour,
        });
        app.apply_update(Update::FeedFetched {
            node: source,
            url: "https://podcast.test/feed.xml".into(),
            result: Ok(fetched.clone()),
        });
        entries.push(app.feeds.entry_members(source)[0]);
    }
    assert_eq!(app.feeds.len(), 2);
    assert_ne!(entries[0], entries[1]);
    let graph = app.graph_runtimes.graph();
    let a = graph.get_node_by_id(entries[0]).unwrap().0;
    let b = graph.get_node_by_id(entries[1]).unwrap().0;
    assert_eq!(graph.shown_resource_id(a), graph.shown_resource_id(b));
    assert!(graph.resource_tag_labels(resource).is_empty());
    assert!(app.graph_pane_select_member(pane, entries[0]));
    app.update(Action::MarkFocusedFeedEntryRead);
    assert!(!surface_controls::read(
        app.graph_runtimes.graph(),
        entries[0],
        Control::Unread
    ));
    assert!(surface_controls::read(
        app.graph_runtimes.graph(),
        entries[1],
        Control::Unread
    ));
    app.reconcile_feed_tags();
    assert!(surface_controls::read(
        app.graph_runtimes.graph(),
        entries[1],
        Control::Unread
    ));
    assert!(app.graph_pane_select_member(pane, first));
    app.update(Action::UnsubscribeFocusedFeed);
    assert!(app.node_is_kept(first));
    assert!(!surface_controls::read(
        app.graph_runtimes.graph(),
        first,
        Control::Feed
    ));
    assert!(surface_controls::read(
        app.graph_runtimes.graph(),
        second,
        Control::Feed
    ));
    assert!(surface_controls::read(
        app.graph_runtimes.graph(),
        entries[0],
        Control::FeedEntry
    ));
    assert!(surface_controls::read(
        app.graph_runtimes.graph(),
        entries[1],
        Control::Unread
    ));
    app.reconcile_feed_tags();
    assert_eq!(app.feeds.len(), 1);
    assert_eq!(app.feeds.unread_count(), 1);
    let root = tempfile::tempdir().unwrap();
    app.feeds.save(root.path()).unwrap();
    let mut restored = crate::feed::FeedSubscriptions::load(root.path());
    restored.reconcile(app.graph_runtimes.graph());
    assert_eq!(restored.surface_flags(entries[0]), (false, false, false));
    assert_eq!(restored.surface_flags(entries[1]), (false, true, true));
    assert!(restored.surface_flags(second).0);
}

#[test]
fn unchanged_feed_repairs_missing_binding_as_a_new_read_view() {
    let mut app = App::test_stub();
    let url = "gemini://binding-repair.test/feed";
    app.update(Action::OpenAddress(url.into()));
    let source = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::SubscribeFocusedFeed {
        period: servitor::Period::Hour,
    });
    let fetched = FetchedPage::text(Some("text/gemini".into()), "=> /entry 2026-10-08 Entry");
    app.apply_update(Update::FeedFetched {
        node: source,
        url: url.into(),
        result: Ok(fetched.clone()),
    });
    let entry = app.feeds.entry_members(source)[0];
    app.feeds.mark_read(entry);
    app.feeds.forget_member(entry);
    app.reconcile_feed_tags();
    // The old equal-URL view survives as an unrelated sibling.
    let count = app.graph_runtimes.graph().node_count();
    app.apply_update(Update::FeedFetched {
        node: source,
        url: url.into(),
        result: Ok(fetched),
    });
    let replacement = app.feeds.entry_members(source)[0];
    assert_ne!(replacement, entry);
    assert_eq!(app.graph_runtimes.graph().node_count(), count + 1);
    assert!(!surface_controls::read(
        app.graph_runtimes.graph(),
        replacement,
        Control::Unread
    ));
    assert!(!surface_controls::read(
        app.graph_runtimes.graph(),
        entry,
        Control::Unread
    ));
}
