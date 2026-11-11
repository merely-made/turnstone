// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! The pane's source text widget and its messages to the document's editor.
//!
//! The pane owns the [`TextInput`]; the editor ([`cambium::BlockEditor`]) hears of its
//! edits as `TextCommand`s and is read back through `source` and `selection`.

use super::*;
use cambium::BlockEditor;

impl Pane {
    /// The source as the person sees it: the widget's text.
    pub(super) fn source(&self) -> &str {
        self.source_input.text()
    }
    /// Whether the document has unsaved edits, counting typing not yet sent.
    pub(super) fn dirty(&self) -> bool {
        self.source() != self.document.editor.source() || self.document.editor.is_dirty()
    }
    /// A widget over the editor's source and caret. It logs its commands for the
    /// editor and keeps no undo history of its own.
    pub(super) fn fresh_input(editor: &dyn BlockEditor) -> TextInput {
        let mut input = TextInput::new(editor.source().to_owned());
        input.set_caret_selection(editor.selection());
        input.record_commands(true);
        input
    }
    /// Start the widget over from the editor's source and caret.
    pub(super) fn reset_input(&mut self) {
        self.source_input = Self::fresh_input(self.document.editor.as_ref());
    }
    /// Whether the person has edited since the editor last heard.
    pub(super) fn has_pending_input(&self) -> bool {
        self.source_input.has_commands()
    }
    /// Replay the widget's commands on the editor, in order, so its one journal
    /// groups typing as the widget did and Ctrl+Z / Ctrl+Y run there.
    pub(super) fn push_input(&mut self) {
        for command in self.source_input.take_commands() {
            self.document.editor.apply(command);
        }
    }
    /// Take the editor's source back into the widget when a structural edit,
    /// undo or redo changed it.
    pub(super) fn pull_input(&mut self) {
        if self.source() != self.document.editor.source() {
            self.reset_input();
        }
    }
    /// Send then take back: what the frame does after the person's keys.
    pub(super) fn sync_input(&mut self) {
        self.push_input();
        self.pull_input();
    }
    /// Run `change` on the document with the widget's edits sent first and the
    /// result taken back after.
    pub(super) fn edit<R>(&mut self, change: impl FnOnce(&mut Document) -> R) -> R {
        self.push_input();
        let result = change(&mut self.document);
        self.pull_input();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::super::editor_tests::pane;
    use super::*;

    fn type_text(pane: &mut Pane, text: &str) {
        for c in text.chars() {
            pane.source_input.apply(TextCommand::Insert(c.to_string()));
        }
    }

    #[test]
    fn widget_undo_runs_through_the_editors_one_journal_at_typing_granularity() {
        let mut pane = pane(None);
        let before = pane.source().to_owned();
        // A run of typing is one step in the editor's journal, as before.
        type_text(&mut pane, "abc");
        assert!(pane.dirty());
        pane.source_input.apply(TextCommand::Undo); // Ctrl+Z in the widget
        assert!(pane.has_pending_input());
        pane.sync_input();
        assert_eq!(pane.source(), before, "one Ctrl+Z took back the whole run");
        assert_eq!(pane.document.editor.source(), before);
        assert!(!pane.dirty(), "back at the saved text reads clean");
        pane.source_input.apply(TextCommand::Redo);
        pane.sync_input();
        assert!(pane.source().contains("abc"));
        // Moving the caret ends a run: two runs are two steps.
        pane.source_input.apply(TextCommand::Move {
            movement: cambium::CaretMove::BufferHome,
            extend: false,
        });
        type_text(&mut pane, "xy");
        pane.source_input.apply(TextCommand::Undo);
        pane.sync_input();
        assert!(pane.source().contains("abc") && !pane.source().contains("xy"));
    }

    #[test]
    fn a_structural_edit_then_widget_undo_is_one_history() {
        let mut pane = pane(None);
        let before = pane.source().to_owned();
        type_text(&mut pane, "!");
        // A structural-style edit through the editor, as `Document::apply` does.
        pane.edit(|d| {
            let start = CaretPosition { byte: 0, affinity: CaretAffinity::Downstream };
            d.editor.apply(TextCommand::SetSelection(CaretSelection { anchor: start, focus: start }));
            d.editor.apply(TextCommand::Insert("?".into()))
        });
        assert!(pane.source().contains('?') && pane.source().contains('!'));
        pane.source_input.apply(TextCommand::Undo);
        pane.sync_input();
        assert!(!pane.source().contains('?'), "the edit made through the editor is undone first");
        assert!(pane.source().contains('!'));
        pane.source_input.apply(TextCommand::Undo);
        pane.sync_input();
        assert_eq!(pane.source(), before);
    }
}
