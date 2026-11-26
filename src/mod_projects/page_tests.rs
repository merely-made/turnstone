// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Mods on a live Livery page (mod projects MP5b): attach copies the page, a
//! turn reaches it through the engine, another change makes the next turn a
//! conflict, and detaching leaves the page as the last turn made it.

use super::*;
use inker::session_engine::{DocumentSession, SessionSpawnRequest};
use netrender::Scene;

const PAGE: &str = "<!doctype html><html><head><meta charset=\"utf-8\"></head><body \
    style=\"margin:8px\"><h1>Before</h1><style>p:empty{display:block;height:20px;\
    background:#2a7f62}</style><div>Owned</div></body></html>";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mod-projects")
}

fn page() -> Box<dyn DocumentSession<Scene>> {
    use inker::session_engine::SessionEngine;
    let engine = genet_documents::LiverySessionEngine::new(genet_documents::LocalFetcher);
    let mut session = engine
        .spawn(&SessionSpawnRequest::new("https://example.test/page").with_body(PAGE).with_viewport(480, 320))
        .expect("a Livery page");
    let _ = session.frame(480, 320);
    session
}

fn binding() -> PageBinding {
    PageBinding { node: uuid::Uuid::nil(), admission: None, session: (1, 1) }
}

fn heading(session: &dyn DocumentSession<Scene>) -> String {
    let dom = livery_dom(session).expect("Livery");
    let view = document_host::dom_view::snapshot(dom, 0);
    view.nodes.iter().find(|n| n.kind == "#text").map(|n| n.text.clone()).unwrap_or_default()
}

fn open_two() -> (ModProjectHost, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let host = ModProjectHost::open(dir.path());
    host.create(Some(&fixtures().join("starter"))).unwrap();
    host.create(Some(&fixtures().join("second"))).unwrap();
    (host, dir)
}

#[test]
fn a_turn_changes_what_the_page_shows_through_the_engine() {
    let (host, _dir) = open_two();
    let mut session = page();
    host.attach_page("notes-mod", binding(), livery_dom(session.as_ref()).unwrap()).unwrap();
    assert!(host.listing().rows[0].status.starts_with("Attached to a page"));

    let line = host.turn_page("notes-mod", "set", "After", session.as_mut()).unwrap();
    assert!(line.starts_with("turn set: applied, rev 1, page restyled "), "{line}");
    assert_eq!(heading(session.as_ref()), "After");

    // A turn that inserts into a styled container reaches the cascade.
    let line = host.turn_page("notes-mod", "append", "", session.as_mut()).unwrap();
    assert!(line.starts_with("turn append: applied, rev 2, page restyled "), "{line}");
    let restyled: usize = line.rsplit(' ').next().unwrap().parse().unwrap();
    assert!(restyled > 0, "an inserted element is styled by the engine: {line}");
}

#[test]
fn a_turn_against_a_page_that_changed_underneath_is_refused() {
    let (host, _dir) = open_two();
    let mut session = page();
    host.attach_page("notes-mod", binding(), livery_dom(session.as_ref()).unwrap()).unwrap();
    host.attach_page("second-mod", binding(), livery_dom(session.as_ref()).unwrap()).unwrap();

    // The second mod changes the page; to the first it is a change that is not its own.
    host.turn_page("second-mod", "set", "Second", session.as_mut()).unwrap();
    let line = host.turn_page("notes-mod", "set", "First", session.as_mut()).unwrap();
    assert!(line.starts_with("turn set: page moved"), "{line}");
    assert_eq!(heading(session.as_ref()), "Second", "the refused turn changed nothing");
}

#[test]
fn detaching_leaves_the_page_as_the_last_turn_made_it_and_navigation_says_why() {
    let (host, _dir) = open_two();
    let mut session = page();
    host.attach_page("notes-mod", binding(), livery_dom(session.as_ref()).unwrap()).unwrap();
    assert!(host.attach_page("notes-mod", binding(), livery_dom(session.as_ref()).unwrap()).unwrap_err().contains("already attached"));
    assert!(host.attach("notes-mod").unwrap_err().contains("already attached"));
    host.turn_page("notes-mod", "set", "Kept", session.as_mut()).unwrap();
    host.detach("notes-mod").unwrap();
    assert_eq!(heading(session.as_ref()), "Kept");
    assert_eq!(host.listing().rows[0].status, "Not attached");

    host.attach_page("notes-mod", binding(), livery_dom(session.as_ref()).unwrap()).unwrap();
    host.detach_page("notes-mod", "its page was replaced");
    assert_eq!(host.listing().rows[0].status, "Not attached (detached: its page was replaced)");
    assert_eq!(heading(session.as_ref()), "Kept");
}

#[test]
fn a_mod_project_rollback_is_refused_while_attached_to_a_page() {
    let (host, dir) = open_two();
    host.run_fixtures("notes-mod").unwrap();
    let session = page();
    host.attach_page("notes-mod", binding(), livery_dom(session.as_ref()).unwrap()).unwrap();
    let component = dir.path().join(PROJECTS_DIR).join("notes-mod/component.wasm");
    std::fs::copy(fixtures().join("alt/quiet.wasm"), &component).unwrap();
    let why = host.rollback("notes-mod").unwrap_err();
    assert!(why.contains("attached to a page; detach it first"), "{why}");
    assert_eq!(
        std::fs::read(&component).unwrap(),
        std::fs::read(fixtures().join("alt/quiet.wasm")).unwrap(),
        "a refused rollback restores nothing"
    );
}
