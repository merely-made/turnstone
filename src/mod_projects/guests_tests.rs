// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Parser and viewer projects (mod projects MP7).

use super::*;

const NOTES: &str = "application/x-fixture-notes";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mod-projects")
}

fn host() -> (ModProjectHost, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    (ModProjectHost::open(dir.path()), dir)
}

fn parser(host: &ModProjectHost) -> String {
    let id = host.create(Some(&fixtures().join("parser-notes"))).unwrap();
    host.check(&id).unwrap();
    id
}

#[test]
fn a_parser_project_is_checked_passes_its_fixtures_and_is_a_parser_row() {
    let (host, _dir) = host();
    let id = parser(&host);
    assert_eq!(id, "notes-parser");
    assert_eq!(host.run_fixtures(&id).unwrap(), "2 of 2 fixtures passed");
    let row = host.listing().rows.remove(0);
    assert_eq!(row.kind, "parser");
    assert_eq!(row.check, "check passed");
    assert_eq!(row.media_types.len(), 8);
    assert!(row.media_types.iter().all(|(_, claimed, on)| !claimed && !on));
    assert!(host.attach(&id).unwrap_err().contains("not an attach"));
}

#[test]
fn a_viewer_project_is_checked_passes_its_fixtures_and_shows_a_scene() {
    let (host, _dir) = host();
    let id = host.create(Some(&fixtures().join("viewer-nodes"))).unwrap();
    host.check(&id).unwrap();
    assert_eq!(host.run_fixtures(&id).unwrap(), "1 of 1 fixtures passed");
    assert_eq!(host.listing().rows[0].kind, "viewer");
    let graph = br#"{"snapshot":{"nodes":[{"label":"A"},{"label":"B"},{"label":"C"}]},"facets":{}}"#;
    let (title, scene) = host.view_graph(&id, graph, 300.0).unwrap();
    assert_eq!(title, "Fixture nodes");
    assert_eq!(scene.region_labels(), ["Node A", "Node B", "Node C"]);
}

#[test]
fn check_refuses_a_component_whose_media_types_differ_from_mod_toml_naming_both() {
    let (host, dir) = host();
    host.create(Some(&fixtures().join("parser-notes"))).unwrap();
    let path = dir.path().join(PROJECTS_DIR).join("notes-parser/mod.toml");
    let text = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    std::fs::write(&path, text.replace("    \"application/x-fixture-chatty\",\n", "    \"text/markdown\",\n")).unwrap();
    host.reload("notes-parser").unwrap_err();
    let why = host.listing().rows[0].check.clone();
    assert!(why.contains("the component reports media types [application/x-fixture-badversion,"), "{why}");
    assert!(why.contains("but mod.toml declares ["), "{why}");
    assert!(why.contains("text/markdown"), "{why}");
}

#[test]
fn a_unclaimed_type_routes_and_a_claimed_one_only_when_handed_over() {
    let (host, dir) = host();
    let id = parser(&host);
    assert_eq!(host.route_for(NOTES, false).as_deref(), Some("notes-parser"));
    assert_eq!(host.route_for("Application/X-Fixture-Notes; charset=utf-8", false).as_deref(), Some("notes-parser"));
    assert_eq!(host.route_for("text/plain", false), None);
    // Pretend a built-in claims the type: the parser takes it only when handed over.
    assert_eq!(host.route_for(NOTES, true), None);
    host.set_claimed(&id, NOTES, true).unwrap();
    assert_eq!(host.route_for(NOTES, true).as_deref(), Some("notes-parser"));
    assert!(host.listing().rows[0].media_types.iter().any(|(m, _, on)| m == NOTES && *on));
    assert!(host.set_claimed(&id, "text/gemini", true).unwrap_err().contains("does not declare"));
    // The choice survives a restart.
    let reopened = ModProjectHost::open(dir.path());
    assert_eq!(reopened.route_for(NOTES, true).as_deref(), Some("notes-parser"));
    host.set_claimed(&id, NOTES, false).unwrap();
    assert_eq!(host.route_for(NOTES, true), None);
}

#[test]
fn a_page_is_parsed_with_the_hosts_address_and_a_refusal_names_the_mod() {
    let (host, _dir) = host();
    let id = parser(&host);
    let page = host.parse_page(&id, NOTES, b"# Hi\nbody\n", "http://127.0.0.1/a.notes").unwrap();
    assert_eq!(page.document.title.as_deref(), Some("Hi"));
    assert_eq!(page.document.address, "http://127.0.0.1/a.notes");
    assert_eq!(page.document.provenance.source_kind.as_deref(), Some("mod:notes-parser"));
    assert!(!page.foreign_images, "no network grant");
    let refusal = host
        .parse_page(&id, "application/x-fixture-refuse", b"x", "http://127.0.0.1/r")
        .unwrap_err();
    assert_eq!(refusal.mod_id, "notes-parser");
    let text = refusal.to_string();
    assert!(text.contains("Notes parser") && text.contains("notes-parser") && text.contains("fixture refuses this input"), "{text}");
}

#[test]
fn a_trap_drops_the_instance_keeps_the_trap_and_the_next_page_attaches_afresh() {
    let (host, _dir) = host();
    let id = parser(&host);
    let refusal = host
        .parse_page(&id, "application/x-fixture-trap", b"x", "http://127.0.0.1/t")
        .unwrap_err();
    assert!(refusal.why.contains("trapped and was detached"), "{}", refusal.why);
    assert!(host.listing().rows[0].trap.contains("trapped"));
    assert!(host.parse_page(&id, NOTES, b"# again\n", "http://127.0.0.1/ok").is_ok());
}

#[test]
fn a_parser_and_a_document_project_share_the_list() {
    let (host, _dir) = host();
    host.create(None).unwrap();
    parser(&host);
    let kinds: Vec<String> = host.listing().rows.into_iter().map(|r| r.kind).collect();
    assert_eq!(kinds, ["document", "parser"]);
}

#[test]
fn other_origins_load_only_for_a_parser_holding_the_network_grant() {
    let (host, dir) = host();
    let id = parser(&host);
    let path = dir.path().join(PROJECTS_DIR).join("notes-parser/mod.toml");
    let text = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    std::fs::write(&path, format!("capabilities = [\"network\"]\n{text}")).unwrap();
    host.reload(&id).unwrap();
    let page = |host: &ModProjectHost| host.parse_page(&id, NOTES, b"# t\n", "http://127.0.0.1/a").unwrap();
    assert!(!page(&host).foreign_images, "declared but not granted");
    host.set_grant(&id, "network", true).unwrap();
    assert!(page(&host).foreign_images);
    host.set_grant(&id, "network", false).unwrap();
    assert!(!page(&host).foreign_images);
}

#[test]
fn uninstalling_a_parser_returns_its_claimed_types_and_keeps_its_files() {
    let (host, dir) = host();
    let id = parser(&host);
    host.run_fixtures(&id).unwrap();
    host.set_claimed(&id, NOTES, true).unwrap();
    assert_eq!(host.route_for(NOTES, true).as_deref(), Some("notes-parser"));
    host.uninstall(&id).unwrap();
    // Off the list, out of every route: nothing takes the type, claimed or not.
    assert!(host.listing().rows.is_empty());
    assert_eq!((host.route_for(NOTES, true), host.route_for(NOTES, false)), (None, None));
    assert!(host.uninstall(&id).unwrap_err().contains("no mod project"));
    // Out of the install record too: a restart does not bring it back.
    assert!(ModProjectHost::open(dir.path()).listing().rows.is_empty());
    // The folder and the last good build stay, and the folder is offered again.
    let root = dir.path().join(PROJECTS_DIR).join("notes-parser");
    assert!(root.join("mod.toml").is_file() && root.join(LAST_GOOD).is_file());
    assert_eq!(host.listing().available, [root.clone()]);
    // Opening it again reinstalls it, with the claim and grants it had dropped.
    assert_eq!(host.open_project(&root, Level::Conformant).unwrap(), "notes-parser");
    assert_eq!(host.route_for(NOTES, false).as_deref(), Some("notes-parser"));
    assert_eq!(host.route_for(NOTES, true), None);
}

#[test]
fn uninstalling_a_viewer_takes_it_off_the_list_and_its_folder_stays() {
    let (host, dir) = host();
    let id = host.create(Some(&fixtures().join("viewer-nodes"))).unwrap();
    host.check(&id).unwrap();
    assert_eq!(host.listing().rows[0].title, "Nodes viewer");
    host.uninstall(&id).unwrap();
    assert!(host.listing().rows.is_empty());
    let graph = br#"{"snapshot":{"nodes":[]},"facets":{}}"#;
    assert!(host.view_graph(&id, graph, 300.0).unwrap_err().contains("no mod project"));
    let root = dir.path().join(PROJECTS_DIR).join("nodes-viewer");
    assert!(root.join("mod.toml").is_file());
    host.open_project(&root, Level::Conformant).unwrap();
    assert_eq!(host.listing().rows[0].kind, "viewer");
}
