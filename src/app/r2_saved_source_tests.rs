// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Private corpus admission and opening-generation controls.

use super::App;
use crate::action::{Action, Effect, Update};
use crate::saved_sources::{Admission, Content, Entry, Key, Selector, Target};
use crate::{session, source_capture};
use chartulary::FacetId;
use mere::canvas::Canvas;
use std::path::Path;

fn app_at(root: &Path) -> App {
    let mut app = App::test_stub_at(root.to_path_buf());
    app.sessions = pandect::ManifestStore::with_root(session::sessions_root(root));
    let id = app
        .update(Action::NewSession)
        .into_iter()
        .find_map(|effect| match effect {
            Effect::SwitchSession { id } => Some(id),
            _ => None,
        })
        .expect("a new test session is persisted before adoption");
    app.adopt_session(id);
    app
}

fn visit(app: &mut App, url: &str) -> uuid::Uuid {
    let key = app.graph_runtimes.visit(url);
    app.graph_runtimes.graph().get_node(key).unwrap().id
}

fn attach(app: &mut App, graph: crate::panes::GraphId, node: uuid::Uuid, manifest: &str) {
    crate::graph_facets::edit_canvas_facets(
        &mut app.graph_runtimes.get_mut(graph).unwrap().canvas,
        |facets| source_capture::append_reference(facets, node, manifest.to_owned()),
    )
    .unwrap()
    .unwrap();
}

fn remove_refs(app: &mut App, graph: crate::panes::GraphId, node: uuid::Uuid) {
    crate::graph_facets::edit_canvas_facets(
        &mut app.graph_runtimes.get_mut(graph).unwrap().canvas,
        |facets| {
            facets.remove(
                &node,
                &FacetId::new(source_capture::SOURCE_CAPTURE_REFERENCES_FACET),
            )
        },
    )
    .unwrap();
}

fn target(key: Key, text: &str) -> (Target, Content) {
    let content = Content {
        title: "Captured page title".into(),
        origin: "https://origin-gone.example/source".into(),
        captured_at_ms: 1_700_000_000_000,
        text: text.to_owned(),
        text_hash: eidetic::Hash::of(text.as_bytes()).to_string(),
        raw_manifest: "blake3:raw".into(),
    };
    let selected = text.find("passage").unwrap();
    let start = text[..selected].chars().count();
    let target = Target {
        key,
        text_hash: content.text_hash.clone(),
        selector: Some(Selector {
            start,
            end: start + "passage".chars().count(),
        }),
    };
    (target, content)
}

#[path = "r2_saved_source_tests/admissions_follow_live_session_graph_facets_and_preserve_cases.rs"]
mod admissions_follow_live_session_graph_facets_and_preserve_cases;
#[path = "r2_saved_source_tests/opening_saved_source_uses_the_focused_panes_graph_cases.rs"]
mod opening_saved_source_uses_the_focused_panes_graph_cases;

#[path = "r2_saved_source_tests/model_free_comparison_cases.rs"]
mod model_free_comparison_cases;
