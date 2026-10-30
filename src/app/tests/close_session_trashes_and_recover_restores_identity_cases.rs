// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Overmap O3: closing a session moves its whole directory to the manifest
/// trash (the derived removed-sessions record — no parallel bin), and
/// recovery moves it back with identity intact and switches to it.
#[test]
fn close_session_trashes_and_recover_restores_identity() {
    let mut app = App::test_stub();
    app.data_root = std::env::temp_dir().join(format!("turnstone-o3-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    // Two real sessions on disk (manifests bound to the root so trash ops
    // have a home), the second is current.
    app.sessions = pandect::ManifestStore::with_root(session::sessions_root(&app.data_root));
    let keeper = crate::panes::SessionId::new();
    let mut keeper_m = pandect::GraphSessionManifest::new(
        keeper,
        crate::panes::GraphId::from_uuid(uuid::Uuid::from_u128(0xa)),
    );
    keeper_m.display_name = Some("keeper".to_string());
    app.sessions.insert(keeper_m);
    let closing_id = crate::panes::SessionId::new();
    let mut closing_m = pandect::GraphSessionManifest::new(
        closing_id,
        crate::panes::GraphId::from_uuid(uuid::Uuid::from_u128(0xb)),
    );
    closing_m.display_name = Some("expedition".to_string());
    app.sessions.insert(closing_m);
    app.sessions.flush_dirty().unwrap();
    app.session_id = closing_id;

    // Close: the action defers the disk half to the shell-ordered effect
    // (bin release first); apply_trash is that effect's app half.
    let effects = app.update(Action::CloseSession);
    assert!(matches!(
        effects[..],
        [crate::action::Effect::TrashSession { closing, next }]
            if closing == closing_id && next == keeper
    ));
    assert!(app.apply_trash(closing_id));
    assert_eq!(app.trash.len(), 1);
    assert_eq!(app.trash[0].session_id, closing_id);
    assert_eq!(app.trash[0].display_name.as_deref(), Some("expedition"));
    assert!(
        app.sessions.get(closing_id).is_none(),
        "gone from the live set"
    );

    // Recover: the manifest re-lists with the SAME id + graph id, the
    // trash cache empties, and the switch adopts it.
    let effects = app.update(Action::RecoverSession(closing_id));
    assert!(matches!(
        effects[..],
        [crate::action::Effect::SwitchSession { id }] if id == closing_id
    ));
    assert!(app.trash.is_empty(), "the trash entry is consumed");
    let recovered = app.sessions.get(closing_id).expect("re-listed");
    assert_eq!(
        recovered.root_graph_id,
        crate::panes::GraphId::from_uuid(uuid::Uuid::from_u128(0xb)),
        "identity intact"
    );
    assert!(
        app.take_events()
            .iter()
            .any(|e| matches!(e, AppEvent::SessionRecovered(l) if l == "expedition")),
        "the recovery event carries the label"
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

/// Committing a `>` registry row lowers the registry Action through the
/// same spine as everything else, and the palette closes.
#[test]
fn committing_an_action_row_runs_the_action_and_closes() {
    let mut app = App::test_stub();
    for action in [
        Action::OmnibarOpen { command: true },
        Action::OmnibarChar('i'),
        Action::OmnibarChar('s'),
        Action::OmnibarChar('o'),
    ] {
        app.update(action);
    }
    assert!(!app.graph_runtimes.is_isometric());
    let effects = app.update(Action::OmnibarCommit);
    assert!(
        app.graph_runtimes.is_isometric(),
        "the committed toggle ran"
    );
    assert!(!app.omnibar.open, "the palette closed on commit");
    assert!(effects.contains(&Effect::Redraw));
}

/// The content flip lowers through the spine: focused node -> Requested +
/// SpawnContent; the port's honest failure folds back; a failed node
/// retries on the next flip.
#[test]
fn content_flip_lowers_and_fails_honestly() {
    use crate::content::NodeContent;
    let mut app = App::test_stub();
    assert!(
        app.update(Action::ToggleNodeContent).is_empty(),
        "no focus, no-op"
    );
    app.graph_runtimes.visit("https://example.com/page");
    let effects = app.update(Action::ToggleNodeContent);
    let Some(Effect::SpawnContent { node, url }) = effects
        .iter()
        .find(|e| matches!(e, Effect::SpawnContent { .. }))
        .cloned()
    else {
        panic!("flip on a focused node spawns: {effects:?}");
    };
    assert_eq!(url, "https://example.com/page");
    assert_eq!(app.content.get(node), Some(&NodeContent::Requested));
    assert!(
        !app.update(Action::ToggleNodeContent)
            .iter()
            .any(|e| matches!(e, Effect::SpawnContent { .. })),
        "flipping an in-flight node closes, never double-spawns"
    );
    app.content.note_requested(node);
    app.apply_update(Update::ContentFailed {
        node,
        error: "port not wired".into(),
    });
    assert!(
        matches!(app.content.get(node), Some(NodeContent::Failed(_))),
        "failure is a surfaced state"
    );
    assert!(
        app.update(Action::ToggleNodeContent)
            .iter()
            .any(|e| matches!(e, Effect::SpawnContent { .. })),
        "a failed node retries on the next flip"
    );
}

/// The tear-out leaf arm (rung 7 depth): the active pane's leaf leaves
/// the primary tree and joins a lens space — SAME pane id (the retained
/// runner never moves; identity is structural). No lens open spawns one.
#[test]
fn tear_out_moves_the_leaf_and_keeps_its_id() {
    let mut app = App::test_stub();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::ROSTER,
    )));
    let roster_id = app
        .frisket
        .iter_leaves()
        .find(|(_, c, _)| matches!(c, PaneContent::Roster))
        .map(|(id, _, _)| id)
        .expect("summoned");
    let effects = app.update(Action::TearOutActivePane);
    // Departure: the primary tree no longer holds a Roster leaf.
    assert!(
        !app.frisket
            .iter_leaves()
            .any(|(_, c, _)| matches!(c, PaneContent::Roster)),
        "the roster left the primary tree"
    );
    // Arrival: a lens space spawned (no lens was open) and holds the SAME
    // pane id — the leaf moved, nothing was recreated.
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::OpenWindow { .. })),
        "tearing out with no lens spawns one: {effects:?}"
    );
    let lens = app.lenses[0].as_ref().expect("lens space seeded");
    let moved = lens
        .iter_leaves()
        .find(|(_, c, _)| matches!(c, PaneContent::Roster))
        .expect("the roster landed in the lens");
    assert_eq!(moved.0, roster_id, "same pane id across the move");
    // The moved pane STAYS active, so pane-anchored ops follow it: a
    // summon lands beside it IN THE LENS (the window as pane host).
    assert_eq!(
        app.active_pane,
        Some(roster_id),
        "the moved pane stays active"
    );
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::TRAIL,
    )));
    let lens = app.lenses[0].as_ref().unwrap();
    assert!(
        lens.iter_leaves()
            .any(|(_, c, _)| matches!(c, PaneContent::Trail)),
        "summon-beside followed the active pane into the lens"
    );
    assert!(
        !app.frisket
            .iter_leaves()
            .any(|(_, c, _)| matches!(c, PaneContent::Trail)),
        "the summoned trail is not in the primary tree"
    );
    // A PRIMARY pane tearing out reuses the open lens (no window spam).
    app.active_pane = None;
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::GLOSS,
    )));
    let effects = app.update(Action::TearOutActivePane);
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::OpenWindow { .. })),
        "an open lens is reused"
    );
    let lens = app.lenses[0].as_ref().unwrap();
    assert!(
        lens.iter_leaves()
            .any(|(_, c, _)| matches!(c, PaneContent::Gloss(_))),
        "the gloss joined the existing lens"
    );
}

#[test]
fn summon_enforces_registry_multiplicity_in_one_space() {
    let mut app = App::test_stub();
    for _ in 0..2 {
        app.update(Action::SummonPane(crate::panes::PaneKindId::new(
            crate::panes::kind::ROSTER,
        )));
    }
    assert_eq!(
        app.frisket
            .iter_leaves()
            .filter(|(_, content, _)| matches!(content, PaneContent::Roster))
            .count(),
        1,
        "Roster is per-space-and-context"
    );

    for _ in 0..2 {
        app.update(Action::SummonPane(crate::panes::PaneKindId::new(
            crate::panes::kind::GLOSS,
        )));
    }
    assert_eq!(
        app.frisket
            .iter_leaves()
            .filter(|(_, content, _)| matches!(content, PaneContent::Gloss(_)))
            .count(),
        2,
        "Gloss permits many pane instances"
    );
}

/// The rename flow through the omnibar: BeginRenameSession opens the bar in
/// rename mode seeded with the current label; commit lowers RenameSession
/// and the label updates. An empty name clears back to the derived/uuid
/// fallback.
#[test]
fn rename_session_through_the_omnibar_mode() {
    use crate::ui::OmnibarMode;

    let mut app = App::test_stub();
    let id = app.session_id;
    app.sessions
        .insert(pandect::GraphSessionManifest::new(id, GraphId::nil()));
    // The default label is the uuid prefix.
    assert_eq!(app.session_label(id), id.as_uuid().to_string()[..8]);

    app.update(Action::BeginRenameSession);
    assert!(app.omnibar.open);
    assert!(matches!(app.omnibar.mode, OmnibarMode::RenameSession(rid) if rid == id));

    // Type a new name over the seeded label and commit.
    app.omnibar.text = "Research".to_string();
    app.update(Action::OmnibarCommit);
    assert_eq!(app.session_label(id), "Research");
    assert!(!app.omnibar.open, "commit closes the bar");
    assert!(
        matches!(app.omnibar.mode, OmnibarMode::Address),
        "mode resets"
    );

    // An empty rename clears back to the uuid fallback.
    app.update(Action::RenameSession {
        id,
        name: "   ".to_string(),
    });
    assert_eq!(app.session_label(id), id.as_uuid().to_string()[..8]);
}

/// The recycle-bin round trip, app-level (the port simulated by folding
/// its answers): delete stages the focused node's record — ORIGINAL id,
/// title, tags — and drops the node; the Trail derives it into Removed;
/// recover re-mints under the SAME id and Removed derives it away.
#[test]
fn delete_stages_into_the_bin_and_recover_restores_identity() {
    use crate::trail_view::{RowAction, trail_rows};

    let mut app = App::test_stub();
    let url = "https://example.com/gone".to_string();
    app.update(Action::OpenAddress(url.clone()));
    let original = app
        .graph_runtimes
        .focused_member()
        .expect("the opened node is focused");
    assert!(app.graph_runtimes.tag_node(original, "saved-label"));

    let fx = app.update(Action::DeleteFocusedNode);
    assert!(
        app.graph_runtimes.graph().get_node_by_url(&url).is_none(),
        "the node left the graph"
    );
    // The record leaves through the bin port carrying the identity.
    let record = fx
        .iter()
        .find_map(|e| match e {
            Effect::RecordDeleted { record } => Some(record.clone()),
            _ => None,
        })
        .expect("delete stages a bin record: {fx:?}");
    assert_eq!(
        record.node_id, original,
        "the record carries the ORIGINAL id"
    );
    assert_eq!(record.url, url);
    assert_eq!(record.tags, vec!["saved-label".to_string()]);
    assert!(
        fx.iter().any(|e| matches!(e, Effect::CloseContent { .. })),
        "its content session is closed: {fx:?}"
    );

    // The port answers with the refreshed list (folded as the drain would).
    app.apply_update(Update::BinListed {
        records: vec![record],
    });
    assert!(
        trail_rows(&app)
            .iter()
            .any(|r| matches!(&r.action, RowAction::Recover(id) if id == &original.to_string())),
        "the staged node derives into the Trail's Removed section"
    );

    // Recover BY IDENTITY: same uuid, and Removed derives it away with the
    // record still in the bin (append-only until athanor's pass).
    app.update(Action::RecoverDeletedNode(original));
    assert_eq!(
        app.graph_runtimes.focused_member(),
        Some(original),
        "the node is back under its ORIGINAL id, selected"
    );
    assert!(
        app.graph_runtimes.graph().get_node_by_url(&url).is_some(),
        "the url resolves again"
    );
    assert!(
        !trail_rows(&app)
            .iter()
            .any(|r| matches!(&r.action, RowAction::Recover(_))),
        "Removed derives away once the node is present (record still staged)"
    );
    assert!(!app.removed.is_empty(), "the bin record itself remains");
    let key = app
        .graph_runtimes
        .graph()
        .get_node_by_id(original)
        .unwrap()
        .0;
    assert!(
        crate::content_tags::content_tags(app.graph_runtimes.graph(), key).contains("saved-label")
    );
}

/// The envelope lane end to end (participant gate B3): a dropped `.wasm`
/// installs as a component participant after the same VISIBLE review — whose
/// row now names its ring profile — and running it lowers exactly the
/// emissions its grant covers, attributed, while the ungranted ring and
/// gate management are refused inside the run.
#[cfg(feature = "wasm")]
#[test]
fn a_component_denizen_acts_only_within_its_reviewed_rings() {
    let mut app = App::test_stub();
    app.data_root =
        std::env::temp_dir().join(format!("turnstone-component-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    let pack = std::path::Path::new("scenarios/fixtures/app_core_guest.wasm");
    assert!(
        pack.exists(),
        "the app-core guest fixture is missing at {}",
        pack.display()
    );

    // Stage: the review names the component and its preselected rings.
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    let review = &app.denizen_actions()[0].0;
    assert!(review.contains("(wasm)"), "the lane is named: {review}");
    for ring in ["navigate", "panes", "dispatch"] {
        assert!(review.contains(ring), "the ask names {ring}: {review}");
    }
    assert!(
        !review.contains("session"),
        "the destructive ring is never preselected: {review}"
    );

    app.update(Action::ConfirmInstallDenizen);
    let (member, subject) = {
        let (&m, r) = app.denizens.residents.iter().next().unwrap();
        (m, r.subject)
    };
    let binding = pandect::read_denizen_binding(app.graph_runtimes.facets(), member).unwrap();
    assert_eq!(binding.kind, pandect::DenizenKind::Pack);
    let expected_author = participant_author(&app.denizens.residents[&member]);
    let file = app
        .graph_runtimes
        .facets()
        .get(
            &member,
            &chartulary::FacetId::new(crate::denizen::COMPONENT_FACET),
        )
        .and_then(|v| v.as_str().map(str::to_string))
        .expect("the component facet points at the stored bytes");
    assert!(
        crate::denizen::component_path(&app.session_dir(), &file).is_file(),
        "the component's bytes live beside the worlds"
    );
    // The grant is exactly the reviewed rings: each ring is its own power,
    // and there is no capability above them that could grant them wholesale.
    let covers = |ring: crate::ring::Ring| {
        servitor::AuthorityProvider::covers(
            &app.denizens.authority,
            subject,
            &ring.cap().expect("a grantable ring"),
            servitor::Mode::Write,
        )
    };
    use crate::ring::Ring;
    assert!(covers(Ring::Navigate) && covers(Ring::Panes) && covers(Ring::Dispatch));
    assert!(!covers(Ring::Session), "an unreviewed ring is ungranted");

    // Run: the guest emits one action per ring. Only the covered ones land.
    let before = app.graph_runtimes.graph().node_count();
    app.update(Action::RunDenizen { member });
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://kept/note")
            .is_some(),
        "the navigate emission lowered through the spine"
    );
    assert_eq!(
        app.graph_runtimes.graph().node_count(),
        before + 1,
        "and nothing else minted a node"
    );
    assert!(
        app.take_events()
            .iter()
            .any(|e| matches!(e, AppEvent::DenizenRan(_))),
        "the run is observable"
    );
    // Attribution: the component's edit reads back under its subject.
    let journal = app.journal.lock().unwrap();
    assert!(
        journal
            .entries()
            .iter()
            .any(|entry| entry.author == expected_author),
        "the component's graph edit is attributed to its subject"
    );
    drop(journal);
    let _ = std::fs::remove_dir_all(&app.data_root);
}
