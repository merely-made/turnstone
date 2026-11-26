// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use std::sync::Arc;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mod-projects")
}

fn host() -> (ModProjectHost, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    (ModProjectHost::open(dir.path()), dir)
}

fn project_dir(dir: &Path) -> PathBuf {
    dir.join(PROJECTS_DIR).join("notes-mod")
}

/// `mod.toml` rewritten to declare `network`.
fn declare_network(dir: &Path) {
    let path = project_dir(dir).join("mod.toml");
    let text = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    std::fs::write(path, text.replace("capabilities = []", "capabilities = [\"network\"]")).unwrap();
}

/// The project's component swapped for one of the `alt/` fixtures.
fn swap(dir: &Path, alt: &str) {
    let to = dir.join(PROJECTS_DIR).join("notes-mod/component.wasm");
    std::fs::copy(fixtures().join("alt").join(alt), to).unwrap();
}

fn row(host: &ModProjectHost) -> ProjectRow {
    host.listing().rows.remove(0)
}

#[test]
fn a_starter_is_created_checked_attached_and_passes_its_fixture() {
    let (host, _dir) = host();
    assert_eq!(host.create(None).unwrap(), "notes-mod");
    host.check("notes-mod").unwrap();
    assert_eq!(row(&host).check, "check passed");
    host.attach("notes-mod").unwrap();
    assert_eq!(row(&host).document, ["body", "  p", "    #text \"Hello\""]);
    assert_eq!(host.run_fixtures("notes-mod").unwrap(), "1 of 1 fixtures passed");
    let turn = host.turn("notes-mod", "set", "One").unwrap();
    assert_eq!(turn, "turn set: applied, rev 1");
    assert_eq!(row(&host).document[2], "    #text \"One\"");
}

#[test]
fn a_fixture_folder_is_copied_in_and_a_second_create_is_refused() {
    let (host, dir) = host();
    assert_eq!(host.create(Some(&fixtures().join("starter"))).unwrap(), "notes-mod");
    assert!(dir.path().join(PROJECTS_DIR).join("notes-mod/mod.toml").is_file());
    assert!(host.create(None).unwrap_err().contains("already exists"));
}

#[test]
fn reload_changes_behaviour_on_the_same_document_without_a_restart() {
    let (host, dir) = host();
    host.create(None).unwrap();
    host.attach("notes-mod").unwrap();
    host.turn("notes-mod", "set", "One").unwrap();
    swap(dir.path(), "quiet.wasm");
    // A conformant component pinned at admission is refused until re-admitted.
    assert!(host.run_fixtures("notes-mod").unwrap_err().contains("0 of 1"));
    assert_eq!(host.reload("notes-mod").unwrap(), "reloaded on the same document");
    let after = row(&host);
    assert_eq!(after.document[2], "    #text \"One\"", "the document stays");
    assert_eq!(after.status, "Attached, revision 1");
    let turn = host.turn("notes-mod", "set", "Two").unwrap();
    assert_eq!(turn, "turn set: conflict, current rev 1");
    let failed = host.run_fixtures("notes-mod").unwrap_err();
    assert!(failed.contains("0 of 1"), "{failed}");
    assert!(row(&host).fixtures.iter().any(|l| l.contains("- 1 set: applied, rev 1")));
}

#[test]
fn an_edited_component_is_refused_at_attach_until_it_is_reloaded() {
    let (host, dir) = host();
    host.create(None).unwrap();
    swap(dir.path(), "quiet.wasm");
    let refused = host.attach("notes-mod").unwrap_err();
    assert!(refused.contains("changed since admission"), "{refused}");
    assert_eq!(host.reload("notes-mod").unwrap(), "re-admitted; nothing attached");
    host.attach("notes-mod").unwrap();
}

#[test]
fn an_import_without_a_grant_is_refused_by_name_and_the_old_instance_stays() {
    let (host, dir) = host();
    host.create(None).unwrap();
    host.attach("notes-mod").unwrap();
    swap(dir.path(), "net.wasm");
    let refused = host.reload("notes-mod").unwrap_err();
    assert!(refused.contains("import mere:script/net is not granted"), "{refused}");
    assert_eq!(host.turn("notes-mod", "set", "Still").unwrap(), "turn set: applied, rev 1");
    assert!(host.status_lines()[0].contains("mere:script/net"));
    assert!(row(&host).check.contains("mere:script/net"));
}

#[test]
fn a_trap_is_named_and_detaches_the_script() {
    let (host, dir) = host();
    host.create(None).unwrap();
    swap(dir.path(), "quiet.wasm");
    host.reload("notes-mod").unwrap();
    host.attach("notes-mod").unwrap();
    let trapped = host.turn("notes-mod", "loop", "").unwrap_err();
    assert!(trapped.contains("trapped"), "{trapped}");
    let after = row(&host);
    assert!(!after.attached);
    assert!(after.trap.starts_with("turn 'loop' trapped"), "{}", after.trap);
}

#[test]
fn the_opened_set_and_levels_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let host = ModProjectHost::open(dir.path());
    host.create(None).unwrap();
    host.set_level("notes-mod", Level::Dev).unwrap();
    let again = ModProjectHost::open(dir.path());
    assert_eq!(again.listing().rows[0].level, Level::Dev);
}

#[test]
fn without_a_toolchain_build_is_hidden_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let host = Arc::new(ModProjectHost::open_with(dir.path(), "no-such-cargo-here"));
    host.create(None).unwrap();
    assert!(!row(&host).can_build);
    let why = host.build("notes-mod").unwrap_err();
    assert!(why.contains("no toolchain"), "{why}");
    let blocked = host.build_blocking("notes-mod").unwrap_err();
    assert!(blocked.contains("no-such-cargo-here"), "{blocked}");
    assert!(row(&host).build.contains("no-such-cargo-here"));
}

#[test]
#[ignore = "builds the starter to wasm32-wasip2; run with --ignored where the toolchain exists"]
fn the_starter_builds_where_the_toolchain_exists() {
    let (host, _dir) = host();
    if !host.toolchain_present() {
        eprintln!("skipped: cargo was not found");
        return;
    }
    host.create(None).unwrap();
    let built = host.build_blocking("notes-mod").unwrap();
    assert!(built.starts_with("built "), "{built}");
    host.reload("notes-mod").unwrap();
    host.check("notes-mod").unwrap();
}

#[test]
fn a_grant_toggle_links_net_at_the_next_reload_and_persists() {
    let (host, dir) = host();
    host.create(None).unwrap();
    host.attach("notes-mod").unwrap();
    assert!(host.set_grant("notes-mod", "network", true).unwrap_err().contains("does not declare"));
    swap(dir.path(), "net.wasm");
    declare_network(dir.path());
    let refused = host.reload("notes-mod").unwrap_err();
    assert!(refused.contains("import mere:script/net is not granted"), "{refused}");
    assert_eq!(row(&host).capabilities, [("network".to_string(), false)]);
    host.set_grant("notes-mod", "network", true).unwrap();
    assert_eq!(host.reload("notes-mod").unwrap(), "reloaded on the same document");
    assert_eq!(host.turn("notes-mod", "set", "Net").unwrap(), "turn set: applied, rev 1");
    // The grant is in installed.json, and the same at the other level.
    let again = ModProjectHost::open(dir.path());
    assert_eq!(again.listing().rows[0].capabilities, [("network".to_string(), true)]);
    again.set_level("notes-mod", Level::Dev).unwrap();
    again.attach("notes-mod").unwrap();
    again.set_grant("notes-mod", "network", false).unwrap();
    again.detach("notes-mod").unwrap();
    assert!(again.attach("notes-mod").unwrap_err().contains("mere:script/net"));
}

#[test]
fn a_reload_that_fails_at_activate_leaves_the_pin_on_the_running_component() {
    let (host, dir) = host();
    host.create(None).unwrap();
    host.attach("notes-mod").unwrap();
    let before = host.pinned("notes-mod").unwrap();
    assert_eq!(before, document_host::mod_loader::hash_bytes(&std::fs::read(
        fixtures().join("starter/component.wasm")).unwrap()));
    swap(dir.path(), "trap.wasm");
    let failed = host.reload("notes-mod").unwrap_err();
    assert!(failed.contains("activate") && failed.contains("the old instance stays attached"), "{failed}");
    assert_eq!(host.pinned("notes-mod").unwrap(), before, "the pin names the running component");
    assert_eq!(host.turn("notes-mod", "set", "Still").unwrap(), "turn set: applied, rev 1");
    // A good component re-admits after the failure.
    std::fs::copy(fixtures().join("starter/component.wasm"), project_dir(dir.path()).join("component.wasm")).unwrap();
    host.reload("notes-mod").unwrap();
    assert_eq!(host.pinned("notes-mod").unwrap(), before);
}

#[test]
fn the_last_passing_component_is_kept_as_last_good() {
    let (host, dir) = host();
    host.create(None).unwrap();
    let good = project_dir(dir.path()).join("last-good.wasm");
    host.check("notes-mod").unwrap();
    assert!(!good.exists(), "a check alone does not pass a project that has fixtures");
    host.run_fixtures("notes-mod").unwrap();
    let starter = std::fs::read(fixtures().join("starter/component.wasm")).unwrap();
    assert_eq!(std::fs::read(&good).unwrap(), starter);
    swap(dir.path(), "quiet.wasm");
    host.reload("notes-mod").unwrap();
    host.run_fixtures("notes-mod").unwrap_err();
    assert_eq!(std::fs::read(&good).unwrap(), starter, "a failing run keeps the earlier one");
}

#[test]
fn with_no_fixtures_a_passing_check_is_enough_for_last_good() {
    let (host, dir) = host();
    host.create(None).unwrap();
    std::fs::remove_dir_all(project_dir(dir.path()).join("fixtures")).unwrap();
    host.check("notes-mod").unwrap();
    assert!(project_dir(dir.path()).join("last-good.wasm").is_file());
}
