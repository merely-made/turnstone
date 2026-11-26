// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The authoring loop (mod projects MP6): a pass snapshots the sources, the row
//! diffs and counts outside edits, and rollback restores the component only.

use super::*;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mod-projects")
}

fn passing() -> (ModProjectHost, tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let host = ModProjectHost::open(dir.path());
    host.create(None).unwrap();
    host.run_fixtures("notes-mod").unwrap();
    let root = dir.path().join(PROJECTS_DIR).join("notes-mod");
    (host, dir, root)
}

fn row(host: &ModProjectHost) -> ProjectRow {
    host.listing().rows.remove(0)
}

fn edit(root: &Path, file: &str, from: &str, to: &str) {
    let path = root.join(file);
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("\r\n", "\n");
    assert!(text.contains(from), "{file} lacks {from}");
    std::fs::write(path, text.replace(from, to)).unwrap();
}

#[test]
fn a_mod_project_pass_snapshots_the_text_files_and_nothing_else() {
    let (host, _dir, root) = passing();
    for file in [
        "mod.toml",
        "Cargo.toml",
        "document.txt",
        "src/lib.rs",
        "fixtures/set.toml",
    ] {
        assert!(root.join("last-good").join(file).is_file(), "{file}");
    }
    assert!(!root.join("last-good/component.wasm").exists());
    let now = row(&host);
    assert_eq!(now.changes, "Changed since the last pass: 0 files");
    assert_eq!(now.component, "component.wasm matches last-good.wasm");
    assert!(now.diff.is_empty() && now.can_rollback);
}

#[test]
fn a_mod_project_row_says_no_pass_yet_before_the_first_one() {
    let dir = tempfile::tempdir().unwrap();
    let host = ModProjectHost::open(dir.path());
    host.create(None).unwrap();
    let now = row(&host);
    assert!(
        now.changes.starts_with("No passing build yet"),
        "{}",
        now.changes
    );
    assert!(!now.can_rollback);
}

#[test]
fn a_mod_project_diff_is_a_line_diff_and_counts_the_files_that_moved() {
    let (host, _dir, root) = passing();
    edit(
        &root,
        "mod.toml",
        "capabilities = []",
        "capabilities = [\"network\"]",
    );
    edit(
        &root,
        "src/lib.rs",
        "notes-mod: activated",
        "notes-mod: woke up",
    );
    std::fs::write(root.join("src/extra.rs"), "// new\n").unwrap();
    edit(
        &root,
        "fixtures/set.toml",
        "payload = \"Changed\"",
        "payload = \"Altered\"",
    );
    std::fs::copy(
        fixtures().join("alt/quiet.wasm"),
        root.join("component.wasm"),
    )
    .unwrap();
    // Nothing runs unasked: the row still holds the last scan until an action or pane open.
    assert!(row(&host).changes.contains(": 0 files"));
    host.refresh_changes();
    let now = row(&host);
    assert_eq!(now.changes, "Changed since the last pass: 4 files");
    assert_eq!(now.component, "component.wasm differs from last-good.wasm");
    let has = |line: &str| now.diff.iter().any(|l| l == line);
    assert!(has("mod.toml: -capabilities = []"), "{:?}", now.diff);
    assert!(has("mod.toml: +capabilities = [\"network\"]"));
    assert!(has("src/extra.rs: +// new"));
    assert!(has("fixtures/set.toml: -payload = \"Changed\""));
    assert!(
        now.diff
            .iter()
            .any(|l| l.starts_with("src/lib.rs: +") && l.contains("woke up"))
    );
}

#[test]
fn a_mod_project_row_counts_edits_when_an_action_runs() {
    let (host, _dir, root) = passing();
    edit(&root, "document.txt", "Hello", "Howdy");
    host.check("notes-mod").unwrap();
    let now = row(&host);
    assert_eq!(now.changes, "Changed since the last pass: 1 file");
    assert_eq!(
        now.diff,
        [
            "document.txt: -    #text \"Hello\"",
            "document.txt: +    #text \"Howdy\""
        ]
    );
}

#[test]
fn a_mod_project_diff_is_capped_and_says_how_many_lines_it_left_out() {
    let (host, _dir, root) = passing();
    let long: String = (0..25).map(|i| format!("// line {i}\n")).collect();
    std::fs::write(root.join("src/long.rs"), long).unwrap();
    host.refresh_changes();
    let now = row(&host);
    assert_eq!(now.diff.len(), 11);
    assert_eq!(now.diff.last().unwrap(), "... 15 more lines");
}

#[test]
fn a_mod_project_rollback_restores_the_component_and_leaves_the_sources_as_edited() {
    let (host, _dir, root) = passing();
    host.attach("notes-mod").unwrap();
    host.turn("notes-mod", "set", "One").unwrap();
    let starter = std::fs::read(fixtures().join("starter/component.wasm")).unwrap();
    edit(
        &root,
        "src/lib.rs",
        "notes-mod: activated",
        "notes-mod: broken",
    );
    std::fs::copy(
        fixtures().join("alt/trap_turn.wasm"),
        root.join("component.wasm"),
    )
    .unwrap();
    host.reload("notes-mod").unwrap();
    assert!(
        host.turn("notes-mod", "set", "Two")
            .unwrap_err()
            .contains("trapped")
    );
    let trapped = row(&host);
    assert!(!trapped.attached, "a trapped turn detaches (D20)");
    assert!(
        trapped.trap.contains("turn 'set' trapped"),
        "{}",
        trapped.trap
    );
    assert!(trapped.component.contains("differs"));

    let note = host.rollback("notes-mod").unwrap();
    assert_eq!(
        note,
        "restored last-good.wasm; re-admitted; nothing attached"
    );
    assert_eq!(std::fs::read(root.join("component.wasm")).unwrap(), starter);
    let after = row(&host);
    assert!(after.changes.ends_with("1 file") && after.component.contains("matches"));
    assert!(
        after.diff.iter().any(|l| l.contains("notes-mod: broken")),
        "the diff still shows the edit"
    );
    assert!(
        after.trap.contains("turn 'set' trapped"),
        "the trap is kept (D20)"
    );
    assert!(
        std::fs::read_to_string(root.join("src/lib.rs"))
            .unwrap()
            .contains("notes-mod: broken")
    );
    host.attach("notes-mod").unwrap();
    assert_eq!(
        host.turn("notes-mod", "set", "Three").unwrap(),
        "turn set: applied, rev 1"
    );
}

#[test]
fn a_mod_project_rollback_reloads_an_attached_mod_through_the_pin() {
    let (host, _dir, root) = passing();
    host.attach("notes-mod").unwrap();
    host.turn("notes-mod", "set", "One").unwrap();
    let pinned = host.pinned("notes-mod").unwrap();
    std::fs::copy(fixtures().join("alt/net.wasm"), root.join("component.wasm")).unwrap();
    let refused = host.reload("notes-mod").unwrap_err();
    assert!(refused.contains("mere:script/net"), "{refused}");
    assert_eq!(host.pinned("notes-mod").unwrap(), pinned);
    let note = host.rollback("notes-mod").unwrap();
    assert_eq!(
        note,
        "restored last-good.wasm; reloaded on the same document"
    );
    assert_eq!(
        host.turn("notes-mod", "set", "Two").unwrap(),
        "turn set: applied, rev 2"
    );
    assert_eq!(host.pinned("notes-mod").unwrap(), pinned);
}

#[test]
fn a_mod_project_rollback_without_a_passing_build_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let host = ModProjectHost::open(dir.path());
    host.create(None).unwrap();
    let why = host.rollback("notes-mod").unwrap_err();
    assert!(why.contains("no last passing build"), "{why}");
}
