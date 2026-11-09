// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Acceptance through the production native hooks with real file writes.

use std::{cell::RefCell, path::PathBuf, rc::Rc};

use cambium::{FileEvent, FileRequest};
use cambium_genet_winit_host::{CloseRequest, Harness, KeyPress, Modifiers, read_file};
use cambium_rootstock::{FileAnswer, FileChooser};
use tabard_desktop::{
    WorkshopHarness, hooks_with_exporter, host_options, initialize_with_commands,
};
use tabard_workshop::{ExportArtifact, ExportFormat, WorkshopState};
use taproot::Selector;

fn action(name: &str) -> Selector {
    Selector::role("button").with_attr("data-action", name)
}

#[track_caller]
fn click(host: &mut WorkshopHarness, selector: &Selector) {
    assert!(
        host.click_on(selector),
        "mounted control must paint: {selector:?}"
    );
}

fn mount(
    state: WorkshopState,
    destination: PathBuf,
    captures: Rc<RefCell<Vec<ExportArtifact>>>,
) -> WorkshopHarness {
    let hooks = hooks_with_exporter(move |artifact| {
        captures.borrow_mut().push(artifact.clone());
        Some(destination.clone())
    });
    let mut host = Harness::with_command_init(
        |commands| initialize_with_commands(state, commands),
        hooks,
        host_options(),
    );
    host.layout_at(1180.0, 800.0);
    host
}

fn replace_native_text(host: &mut WorkshopHarness, field: &str, value: &str) {
    click(
        host,
        &Selector::role("textbox").with_attr("data-field", field),
    );
    host.press_key(&KeyPress::character("a").with_modifiers(Modifiers {
        ctrl: true,
        ..Modifiers::NONE
    }));
    host.key_injected(value);
    assert_eq!(host.state().text_field(field).unwrap().text(), value);
}

#[test]
fn export_replacement_writes_the_captured_unsaved_draft_and_keeps_undo_history() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("themes.json");
    let exported = directory.path().join("theme.json");
    std::fs::write(&exported, "existing file").unwrap();
    let captures = Rc::new(RefCell::new(Vec::new()));
    let mut host = mount(
        WorkshopState::load(&library).unwrap(),
        exported.clone(),
        captures.clone(),
    );
    let original = host.state().draft_theme().clone();

    replace_native_text(&mut host, "name", "Native export specimen");
    replace_native_text(&mut host, "seed-hex", "#2F7FFF");
    click(&mut host, &action("apply-hex"));
    click(&mut host, &action("toggle-stylesheet"));
    let css = "body { color: rgb(9, 8, 7); }";
    replace_native_text(&mut host, "mode-sheet", css);
    click(&mut host, &action("apply-stylesheet"));
    let captured_theme = host.state().draft_theme().clone();
    click(&mut host, &action("export"));
    assert_eq!(captures.borrow().len(), 1);
    let artifact = captures.borrow()[0].clone();
    assert_eq!(artifact.format, ExportFormat::ThemeJson);
    assert_eq!(host.state().replacement_path(), Some(exported.as_path()));
    assert_eq!(std::fs::read_to_string(&exported).unwrap(), "existing file");
    assert!(!library.exists());
    assert!(host.state().is_dirty());

    // Replacement can be confirmed after further edits. It publishes the
    // artifact captured by the export action, rather than the later draft.
    replace_native_text(&mut host, "name", "Later unsaved name");
    click(&mut host, &action("replace-export"));
    assert_eq!(
        std::fs::read_to_string(&exported).unwrap(),
        artifact.contents
    );
    assert_eq!(host.state().draft_theme().name, "Later unsaved name");
    assert!(host.state().is_dirty());
    let mut imported = WorkshopState::in_memory();
    imported.import_theme_json(&artifact.contents);
    assert_eq!(imported.draft_theme(), &captured_theme);
    assert!(!library.exists());

    click(&mut host, &action("undo"));
    assert_eq!(host.state().draft_theme(), &captured_theme);
    for _ in 0..6 {
        if host.state().draft_theme() == &original {
            break;
        }
        click(&mut host, &action("undo"));
    }
    assert_eq!(
        host.state().draft_theme(),
        &original,
        "export must retain the earlier authoring history"
    );
    click(&mut host, &action("redo"));
    assert_ne!(host.state().draft_theme(), &original);
}

#[test]
fn export_format_controls_reach_native_destinations_for_the_selected_mode() {
    let directory = tempfile::tempdir().unwrap();
    for format in [ExportFormat::Css, ExportFormat::Dtcg] {
        let captures = Rc::new(RefCell::new(Vec::new()));
        let destination = directory
            .path()
            .join(format!("appearance.{}", format.extension()));
        let mut host = mount(
            WorkshopState::in_memory(),
            destination.clone(),
            captures.clone(),
        );
        click(
            &mut host,
            &Selector::role("button").with_attr("data-mode", "hc_dark"),
        );
        replace_native_text(&mut host, "name", "Unsaved portable appearance");
        let authored = host.state().draft_theme().clone();
        let expected = match format {
            ExportFormat::Css => authored
                .css_custom_properties_for_mode(host.state().mode())
                .unwrap(),
            ExportFormat::Dtcg => authored
                .design_tokens_json_for_mode(host.state().mode())
                .unwrap(),
            ExportFormat::ThemeJson => unreachable!(),
        };
        click(
            &mut host,
            &Selector::role("button").with_attr("data-export-format", format.as_key()),
        );
        click(&mut host, &action("export"));
        assert_eq!(captures.borrow()[0].format, format);
        assert_eq!(std::fs::read_to_string(destination).unwrap(), expected);
        assert_eq!(host.state().draft_theme(), &authored);
        assert!(host.state().is_dirty());
        click(&mut host, &action("undo"));
        assert_ne!(host.state().draft_theme(), &authored);
    }
}

struct ReturningFile {
    path: PathBuf,
    requests: Rc<RefCell<Vec<FileRequest>>>,
}

impl FileChooser for ReturningFile {
    fn open(&mut self, request: &FileRequest, answer: FileAnswer) {
        self.requests.borrow_mut().push(request.clone());
        answer.send(FileEvent {
            files: vec![read_file(&self.path).expect("real import fixture file")],
        });
    }
}

#[test]
fn native_import_chooser_returns_a_colliding_file_without_replacing_the_saved_theme() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("themes.json");
    let exported = directory.path().join("existing-theme.json");
    let captures = Rc::new(RefCell::new(Vec::new()));
    let mut host = mount(
        WorkshopState::load(&library).unwrap(),
        exported.clone(),
        captures,
    );
    click(&mut host, &action("save"));
    let saved = host.state().draft_theme().clone();
    click(&mut host, &action("export"));
    assert!(exported.is_file());
    let requests = Rc::new(RefCell::new(Vec::new()));
    host.set_file_chooser(Box::new(ReturningFile {
        path: exported,
        requests: requests.clone(),
    }));
    click(&mut host, &action("import"));
    assert!(
        host.deliver_files(),
        "the real file request must deliver its chosen file"
    );
    assert_eq!(requests.borrow().len(), 1);
    assert!(!host.state().import_requested());
    let imported = host.state().draft_theme().clone();
    assert_ne!(imported.id, saved.id);
    assert_eq!(imported.seeds, saved.seeds);
    assert_eq!(imported.mode_sheets, saved.mode_sheets);
    assert_eq!(host.state().registry().theme_def(&saved.id), Some(&saved));
    assert!(host.state().registry().theme_def(&imported.id).is_none());

    // Even an unchanged import is unpublished user work and needs a close
    // decision before its new identity can be discarded.
    host.request_close(CloseRequest::Native);
    host.relayout();
    assert!(host.state().close_requested());
    assert!(!host.close_requested());
    click(&mut host, &action("cancel-close"));
    click(&mut host, &action("new-copy"));
    assert_eq!(host.state().draft_theme(), &imported);
    click(&mut host, &action("save"));
    let reopened = WorkshopState::load(&library).unwrap();
    assert_eq!(reopened.registry().theme_def(&saved.id), Some(&saved));
    assert_eq!(reopened.registry().theme_def(&imported.id), Some(&imported));
}

#[test]
fn native_close_preserves_invalid_input_until_a_successful_save_and_close() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("themes.json");
    let mut host = mount(
        WorkshopState::load(&library).unwrap(),
        directory.path().join("export.json"),
        Rc::new(RefCell::new(Vec::new())),
    );
    replace_native_text(&mut host, "name", "Close acceptance specimen");
    replace_native_text(&mut host, "seed-hex", "#12");
    host.request_close(CloseRequest::Native);
    host.relayout();
    assert!(host.state().close_requested());
    assert!(!host.close_requested());
    click(&mut host, &action("save-close"));
    assert!(host.state().close_requested());
    assert!(!host.close_requested());
    assert!(!host.state().exit_requested());
    assert_eq!(host.state().text_field("seed-hex").unwrap().text(), "#12");
    assert!(!library.exists());
    click(&mut host, &action("cancel-close"));
    assert!(!host.state().close_requested());
    assert_eq!(host.state().text_field("seed-hex").unwrap().text(), "#12");

    replace_native_text(&mut host, "seed-hex", "#123456");
    click(&mut host, &action("apply-hex"));
    let authored = host.state().draft_theme().clone();
    host.request_close(CloseRequest::Native);
    host.relayout();
    click(&mut host, &action("save-close"));
    assert!(host.close_requested());
    assert!(host.state().exit_requested());
    assert_eq!(
        WorkshopState::load(&library).unwrap().draft_theme(),
        &authored
    );
}

#[test]
fn application_close_failed_save_keeps_the_draft_until_explicit_discard() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("themes.json");
    let mut host = mount(
        WorkshopState::load(&library).unwrap(),
        directory.path().join("export.json"),
        Rc::new(RefCell::new(Vec::new())),
    );
    replace_native_text(&mut host, "name", "Cannot save this close");
    let authored = host.state().draft_theme().clone();
    std::fs::create_dir(&library).unwrap();
    host.commands().close();
    host.after_dispatch();
    host.relayout();
    assert!(host.state().close_requested());
    assert!(!host.close_requested());
    click(&mut host, &action("save-close"));
    assert!(!host.close_requested());
    assert!(!host.state().exit_requested());
    assert!(host.state().status().contains("Could not save"));
    assert_eq!(host.state().draft_theme(), &authored);
    assert!(library.is_dir());
    click(&mut host, &action("discard-close"));
    assert!(host.close_requested());
    assert!(host.state().exit_requested());
    assert!(
        library.is_dir(),
        "explicit close must not replace the failed destination"
    );
}

#[test]
fn shared_caption_close_uses_the_workshops_unsaved_work_policy() {
    use cambium_genet_winit_host::{CaptionLabels, window_caption_controls};
    use tabard_workshop::workshop_view_with_captions;

    let mut host: WorkshopHarness = Harness::with_command_init(
        |commands| {
            let mut init = initialize_with_commands(WorkshopState::in_memory(), commands);
            let commands = commands.clone();
            // Exercise custom captions on every test platform, including macOS
            // where the production adapter retains the native traffic lights.
            init.logic = Box::new(move |state| {
                workshop_view_with_captions(
                    state,
                    window_caption_controls(&commands, &CaptionLabels::default()),
                )
            });
            init
        },
        hooks_with_exporter(|_| None),
        host_options(),
    );
    host.layout_at(1180.0, 800.0);
    replace_native_text(&mut host, "name", "Caption close draft");
    let draft = host.state().draft_theme().clone();
    click(
        &mut host,
        &Selector::role("button").with_attr("data-window-action", "close"),
    );
    host.relayout();
    assert!(host.state().close_requested());
    assert!(!host.close_requested());
    assert_eq!(host.state().draft_theme(), &draft);
    click(&mut host, &action("cancel-close"));
    assert!(!host.state().close_requested());
    assert_eq!(host.state().draft_theme(), &draft);
    click(
        &mut host,
        &Selector::role("button").with_attr("data-window-action", "close"),
    );
    host.relayout();
    click(&mut host, &action("discard-close"));
    assert!(host.close_requested());
}
