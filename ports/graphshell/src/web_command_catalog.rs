// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Command records shared by Graphshell presentation paths.
use cambium::catalogue::{self, ids};
use cambium::{Command, CommandSet, EffectClass};

/// A shared command's label: the verbs other hosts offer too take the
/// catalogue's id and name, and keep the page's surface (SE50).
fn shared(id: &str) -> &'static str {
    catalogue::label(id).expect("a catalogue id")
}

/// The commands the page offers through its menu, by surface.
pub(super) fn page_commands() -> CommandSet {
    let mut set = CommandSet::new().with_defaults([
        ids::VIEW_ZOOM_IN,
        ids::VIEW_ZOOM_OUT,
        ids::VIEW_FIT,
        ids::NODE_NEW,
        "open-projection-editor",
        ids::SESSION_UNDO,
        ids::SESSION_REDO,
        "save-scene",
    ]);
    for (id, label, category) in [
        (ids::NODE_EDIT, shared(ids::NODE_EDIT), "node"),
        ("invoke-action", "Open selected object", "node"),
        ("select-web", "Select research object", "canvas"),
        (ids::VIEW_ZOOM_IN, shared(ids::VIEW_ZOOM_IN), "canvas"),
        (ids::VIEW_ZOOM_OUT, shared(ids::VIEW_ZOOM_OUT), "canvas"),
        (ids::VIEW_FIT, shared(ids::VIEW_FIT), "canvas"),
        ("pan-left", "Pan left", "canvas"),
        ("pan-right", "Pan right", "canvas"),
        ("pan-up", "Pan up", "canvas"),
        ("pan-down", "Pan down", "canvas"),
        (ids::NODE_NEW, shared(ids::NODE_NEW), "canvas"),
        (ids::PHYSICS_TOGGLE, shared(ids::PHYSICS_TOGGLE), "canvas"),
        (ids::SESSION_UNDO, shared(ids::SESSION_UNDO), "session"),
        (ids::SESSION_REDO, shared(ids::SESSION_REDO), "session"),
        ("session-local", "Show the local mere", "session"),
        ("session-remote", "Show the remote mount", "session"),
        ("save-scene", "Save scene", "session"),
        ("reopen-scene", "Reopen saved scene", "session"),
        ("export-codicil", "Export codicil", "session"),
        ("open-codicil", "Open exported codicil", "session"),
        ("open-projection-editor", "Edit projection", "projection"),
        (
            "close-projection-editor",
            "Close projection editor",
            "projection",
        ),
        ("save-projection", "Save projection", "projection"),
        ("reload-projection", "Reload saved projection", "projection"),
        ("undo-projection", "Undo projection edit", "projection"),
        ("redo-projection", "Redo projection edit", "projection"),
        ("undo-projection-save", "Undo projection save", "projection"),
        ("redo-projection-save", "Redo projection save", "projection"),
        (
            "load-practice-projection",
            "Open practice projection",
            "projection",
        ),
        (
            "reopen-practice-projection",
            "Reopen saved practice projection",
            "projection",
        ),
        ("projection-grid", "Projection: grid", "projection"),
        ("compare-projection", "Compare arrangements", "projection"),
        (
            "compare-projection-dynamics",
            "Compare dynamics",
            "projection",
        ),
        ("projection-motion", "Pause or resume preview", "projection"),
        (
            "projection-compare-left",
            "Earlier arrangements",
            "projection",
        ),
        (
            "projection-compare-right",
            "Later arrangements",
            "projection",
        ),
        (
            "projection-compare-up",
            "Earlier comparison rows",
            "projection",
        ),
        (
            "projection-compare-down",
            "Later comparison rows",
            "projection",
        ),
        (
            "projection-scatter",
            "Projection: order by tempo",
            "projection",
        ),
    ] {
        let mut command = catalogue::command(id).unwrap_or_else(|| {
            let effect = match id {
                "save-scene"
                | "reopen-scene"
                | "export-codicil"
                | "open-codicil"
                | "save-projection"
                | "reload-projection"
                | "load-practice-projection"
                | "reopen-practice-projection"
                | "invoke-action" => EffectClass::ExternalEffect,
                // The remaining host-owned verbs edit or navigate the projection/view.
                "select-web"
                | "pan-left"
                | "pan-right"
                | "pan-up"
                | "pan-down"
                | "session-local"
                | "session-remote"
                | "open-projection-editor"
                | "close-projection-editor"
                | "undo-projection"
                | "redo-projection"
                | "undo-projection-save"
                | "redo-projection-save"
                | "projection-grid"
                | "compare-projection"
                | "compare-projection-dynamics"
                | "projection-motion"
                | "projection-compare-left"
                | "projection-compare-right"
                | "projection-compare-up"
                | "projection-compare-down"
                | "projection-scatter" => EffectClass::Curation,
                _ => unreachable!("page command {id} needs an effect declaration"),
            };
            Command::new(id, label, category, effect)
        });
        command.label = label.into();
        command.category = category.into();
        set.register(command);
    }
    set
}
