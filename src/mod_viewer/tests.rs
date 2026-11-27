// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The Mod viewer: the feed, the re-run gate, the failures and the provider (mod projects MP7b).
use super::*;
use crate::contributed_surface::SurfaceProviderRegistry;
use kernel::geometry::PortablePoint;
use kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta};
use uuid::Uuid;

const GROUND: ColorF = ColorF {
    r: 0.97,
    g: 0.97,
    b: 0.95,
    a: 1.0,
};
const THEME: SheetTheme = SheetTheme {
    ink: ColorF {
        r: 0.1,
        g: 0.1,
        b: 0.1,
        a: 1.0,
    },
    accent: ColorF {
        r: 0.1,
        g: 0.4,
        b: 0.9,
        a: 1.0,
    },
    muted: ColorF {
        r: 0.5,
        g: 0.5,
        b: 0.5,
        a: 1.0,
    },
};

fn graph(titles: &[&str]) -> Graph {
    let mut graph = Graph::new();
    for (i, title) in titles.iter().enumerate() {
        let key = add_node(
            &mut graph,
            Some(Uuid::from_u128(i as u128 + 1)),
            format!("https://t.example/{i}"),
            PortablePoint::new(0.0, 0.0),
        );
        apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeTitle {
                key,
                title: (*title).into(),
            },
        );
    }
    graph
}

fn retitle(graph: &mut Graph, from: &str, to: &str) {
    let key = graph
        .nodes()
        .find(|(_, node)| node.title == from)
        .map(|(key, _)| key)
        .unwrap();
    apply_graph_delta(
        graph,
        GraphDelta::SetNodeTitle {
            key,
            title: to.into(),
        },
    );
}

fn viewer_host() -> (Arc<ModProjectHost>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let host = ModProjectHost::open(dir.path());
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mod-projects/viewer-nodes");
    let id = host.create(Some(&fixture)).unwrap();
    host.check(&id).unwrap();
    (Arc::new(host), dir)
}

fn state(mods: &Arc<ModProjectHost>, handles: &Handles) -> State {
    State {
        mod_id: "nodes-viewer".into(),
        mods: mods.clone(),
        handles: handles.clone(),
        ran: None,
        runs: 0,
        theme: None,
        outcome: Outcome::Waiting,
    }
}

#[test]
fn the_feed_is_a_snapshot_and_facets_naming_each_nodes_title() {
    let graph = graph(&["Alpha", "Beta"]);
    let feed = encode_feed(&graph).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&feed).unwrap();
    let titles: Vec<&str> = value["snapshot"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|node| node["title"].as_str())
        .collect();
    assert_eq!(titles, ["Alpha", "Beta"]);
    assert!(value.get("facets").is_some());
}

#[test]
fn a_title_edit_moves_the_key_but_not_the_structural_revision() {
    let mut graph = graph(&["Alpha"]);
    let before = FeedKey::of(&graph);
    retitle(&mut graph, "Alpha", "Gamma");
    let after = FeedKey::of(&graph);
    assert_eq!(
        before.revision, after.revision,
        "a retitle is not structural"
    );
    assert_ne!(
        before, after,
        "and the viewer, which reads titles, still re-runs"
    );
}

#[test]
fn the_viewer_runs_when_the_graph_or_the_width_changes_and_never_otherwise() {
    let (mods, _dir) = viewer_host();
    let handles = Handles::default();
    let mut state = state(&mods, &handles);
    assert!(!state.refresh(300.0), "nothing is published yet");
    assert!(matches!(state.outcome, Outcome::Waiting) && state.runs == 0);

    let mut graph = graph(&["Alpha", "Beta"]);
    handles.publish(&graph, (THEME, GROUND));
    assert!(state.refresh(300.0));
    let Outcome::Shown(shown) = &state.outcome else {
        panic!("shown")
    };
    assert_eq!(shown.title, "Fixture nodes");
    assert_eq!(
        shown
            .regions
            .iter()
            .map(|r| r.label.as_str())
            .collect::<Vec<_>>(),
        ["Node Alpha", "Node Beta"]
    );
    let texts: Vec<&str> = shown.runs.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(
        texts,
        ["Alpha", "Beta"],
        "the sheet's text runs, for the host to draw"
    );
    assert_eq!(state.runs, 1);

    for _ in 0..5 {
        handles.publish(&graph, (THEME, GROUND));
        assert!(
            !state.refresh(300.0),
            "the same graph at the same width is not asked again"
        );
    }
    assert_eq!(state.runs, 1);

    assert!(
        !state.refresh(300.4),
        "a width that rounds alike is the same width"
    );
    assert_eq!(state.runs, 1);
    assert!(state.refresh(420.0));
    assert_eq!(state.runs, 2, "a new width is asked");

    add_node(
        &mut graph,
        None,
        "https://t.example/new".into(),
        PortablePoint::new(0.0, 0.0),
    );
    handles.publish(&graph, (THEME, GROUND));
    assert!(state.refresh(420.0));
    assert_eq!(state.runs, 3, "a new node is asked");
}

#[test]
fn a_new_theme_repaints_the_text_without_asking_the_viewer_again() {
    let (mods, _dir) = viewer_host();
    let handles = Handles::default();
    let mut state = state(&mods, &handles);
    let graph = graph(&["Alpha"]);
    handles.publish(&graph, (THEME, GROUND));
    state.refresh(300.0);
    let colour = |state: &State| match &state.outcome {
        Outcome::Shown(shown) => shown.runs[0].colour,
        _ => panic!("shown"),
    };
    let before = colour(&state);
    let dark = SheetTheme {
        ink: ColorF {
            r: 0.9,
            g: 0.9,
            b: 0.9,
            a: 1.0,
        },
        ..THEME
    };
    handles.publish(
        &graph,
        (
            dark,
            ColorF {
                r: 0.1,
                g: 0.1,
                b: 0.1,
                a: 1.0,
            },
        ),
    );
    assert!(state.refresh(300.0));
    assert_ne!(colour(&state), before);
    assert_eq!(state.runs, 1);
}

#[test]
fn a_trap_shows_its_cause_in_the_pane_and_the_row_and_the_pane_recovers() {
    let (mods, _dir) = viewer_host();
    let handles = Handles::default();
    let mut state = state(&mods, &handles);
    let mut graph = graph(&["Alpha", "TRAP"]);
    handles.publish(&graph, (THEME, GROUND));
    state.refresh(300.0);
    let Outcome::Failed(why) = &state.outcome else {
        panic!("a trap")
    };
    assert!(
        why.contains("nodes-viewer") && why.contains("trapped") && why.contains("unreachable"),
        "{why}"
    );
    assert!(!why.contains("backtrace"), "{why}");
    let cause = why
        .strip_prefix("nodes-viewer: ")
        .expect("the pane names the mod")
        .to_string();
    let row = mods.listing().rows.remove(0);
    assert_eq!(
        row.trap, cause,
        "the row is the project's, so it says the cause"
    );
    assert_eq!(row.failure.as_deref(), Some(cause.as_str()));

    assert!(
        !state.refresh(300.0),
        "a trapped graph is not asked again until it changes"
    );
    retitle(&mut graph, "TRAP", "Fine");
    handles.publish(&graph, (THEME, GROUND));
    assert!(state.refresh(300.0));
    assert!(
        matches!(state.outcome, Outcome::Shown(_)),
        "attached afresh and drew"
    );
    assert_eq!(
        mods.listing().rows[0].failure,
        None,
        "the failure clears; the last trap stays"
    );
}

#[test]
fn a_refusal_reaches_the_row_as_a_trap_does() {
    let (mods, _dir) = viewer_host();
    let why = mods
        .view_graph("nodes-viewer", b"not json", 100.0)
        .unwrap_err();
    assert!(why.contains("refused"), "{why}");
    let row = mods.listing().rows.remove(0);
    let cause = why.strip_prefix("nodes-viewer: ").unwrap();
    assert_eq!(row.trap, cause);
    assert_eq!(row.failure.as_deref(), Some(cause));
    assert!(
        mods.view_graph("nope", b"{}", 100.0)
            .unwrap_err()
            .contains("no mod project")
    );
}

#[test]
fn regions_are_read_with_the_item_translate_as_their_centre() {
    let (mods, _dir) = viewer_host();
    let feed = encode_feed(&graph(&["Alpha", "Beta"])).unwrap();
    let (_, viewed) = mods.view_graph("nodes-viewer", &feed, 320.0).unwrap();
    let boxes = region_boxes(&viewed.scene);
    assert_eq!(
        boxes[0],
        RegionBox {
            label: "Node Alpha".into(),
            x: 10.0,
            y: 10.0,
            w: 20.0,
            h: 20.0
        }
    );
    assert_eq!(boxes[1].y, 40.0);
}

#[test]
fn the_provider_admits_its_own_mod_id_and_refuses_a_malformed_or_foreign_source() {
    let (mods, _dir) = viewer_host();
    let handles = Handles::default();
    let mut registry = SurfaceProviderRegistry::new();
    registry
        .register_provider(ModViewerProvider::new(
            mods,
            handles.clone(),
            "nodes-viewer",
            "Nodes viewer",
        ))
        .unwrap();
    let kind = PaneKindId::new(kind_id("nodes-viewer"));
    assert_eq!(kind.as_str(), "mod:nodes-viewer");
    let mut pane = registry.admit(&kind, &source("nodes-viewer")).unwrap();
    assert_eq!(pane.descriptor().label, "Nodes viewer");
    assert_eq!(pane.descriptor().surface_id.as_str(), "mod:nodes-viewer");
    handles.publish(&graph(&["Alpha"]), (THEME, GROUND));
    for _ in 0..3 {
        let _ = pane.scene(400, 300, 1.0);
    }
    assert!(matches!(
        pane.availability(),
        SurfaceAvailability::Available
    ));
    assert_ne!(
        source("nodes-viewer"),
        source("other"),
        "another mod is another source"
    );
    for (version, id) in [(1, ""), (2, "nodes-viewer"), (1, "other")] {
        let bad = PaneSource::Fixed(SourceRef::External {
            schema: SourceSchemaId::new(SOURCE_SCHEMA),
            payload: SerializedSource {
                version,
                payload: serde_json::json!({ "mod": id }),
            },
        });
        assert!(matches!(
            registry.admit(&kind, &bad),
            Err(SurfaceAdmissionError::InvalidPayload { .. })
        ));
    }
}

fn viewers(ids: &[&str]) -> Vec<(String, String)> {
    ids.iter()
        .map(|id| ((*id).to_owned(), format!("Title of {id}")))
        .collect()
}

fn palette_labels() -> Vec<String> {
    crate::panes::runtime_palette_entries()
        .into_iter()
        .map(|(label, _)| label)
        .collect()
}

#[test]
fn each_viewer_is_its_own_kind_that_follows_the_project_list() {
    let (mods, _dir) = viewer_host();
    let handles = Handles::default();
    let mut registry = SurfaceProviderRegistry::new();
    let a = PaneKindId::new(kind_id("a"));
    let b = PaneKindId::new(kind_id("b"));
    // Installing two viewers adds two kinds, each with a definition and a provider.
    let synced = sync_kinds(&mut registry, &viewers(&["a", "b"]), &mods, &handles);
    assert!(synced.changed && synced.removed.is_empty());
    assert_eq!(synced.added, [a.clone(), b.clone()]);
    assert_eq!(registry.len(), 2);
    assert_eq!(palette_labels(), ["Open Title of a pane", "Open Title of b pane"]);
    let definition = crate::panes::pane_definition("mod:a").expect("a runtime kind");
    assert_eq!(&*definition.display_name, "Title of a");
    assert_eq!(definition.renderer, crate::panes::PaneRenderer::Contributed);
    assert_eq!(
        definition.multiplicity,
        crate::panes::PaneMultiplicity::PerSpaceAndSource
    );
    // The same list is no change.
    assert!(!sync_kinds(&mut registry, &viewers(&["a", "b"]), &mods, &handles).changed);
    // A retitle renames the definition and the palette row, and the kind keeps its provider.
    let retitled = vec![("a".to_owned(), "Alpha".to_owned()), ("b".to_owned(), "Title of b".to_owned())];
    let synced = sync_kinds(&mut registry, &retitled, &mods, &handles);
    assert!(synced.changed && synced.added.is_empty(), "a retitle does not restart open panes");
    assert_eq!(&*crate::panes::pane_definition("mod:a").unwrap().display_name, "Alpha");
    assert!(registry.contains_pane_kind(&a));
    // Uninstalling one removes its definition, palette row and provider; the other stays.
    let synced = sync_kinds(&mut registry, &viewers(&["b"]), &mods, &handles);
    assert_eq!(synced.removed, [a.clone()]);
    assert!(crate::panes::pane_definition("mod:a").is_none());
    assert!(!registry.contains_pane_kind(&a) && registry.contains_pane_kind(&b));
    assert_eq!(palette_labels(), ["Open Title of b pane"]);
    // Reinstalling brings the kind back, and a pane of it admits again.
    let synced = sync_kinds(&mut registry, &viewers(&["a", "b"]), &mods, &handles);
    assert_eq!(synced.added, [a.clone()]);
    assert!(registry.admit(&a, &source("a")).is_ok());
    // No viewers left: no viewer kind remains, and the single pane is not among them.
    let synced = sync_kinds(&mut registry, &[], &mods, &handles);
    assert_eq!(synced.removed.len(), 2);
    assert!(registry.is_empty() && palette_labels().is_empty());
    assert!(crate::panes::pane_definition("turnstone.mod-viewer").is_none());
}

#[test]
fn an_uninstalled_viewers_pane_reads_that_its_mod_is_not_installed() {
    let label = |kind: &str| crate::unavailable_pane::contributed_label(&PaneKindId::new(kind));
    assert_eq!(
        label("mod:nodes-viewer").as_deref(),
        Some("Viewer mod nodes-viewer is not installed")
    );
    assert_eq!(label("mod:"), None);
    assert_eq!(label("turnstone.settings"), None);
}

#[test]
fn a_viewer_kind_is_told_from_its_mod_id() {
    assert_eq!(kind_id("nodes-viewer"), "mod:nodes-viewer");
    assert_eq!(mod_of_kind("mod:nodes-viewer"), Some("nodes-viewer"));
    assert_eq!(mod_of_kind("turnstone.settings"), None);
}
