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

impl Pane {
    /// The source as the person sees it: the widget's text.
    pub(super) fn source(&self) -> &str {
        self.source_input.text()
    }
    /// Whether the document has unsaved edits, counting typing not yet sent.
    pub(super) fn dirty(&self) -> bool {
        self.source() != self.document.editor.source() || self.document.editor.is_dirty()
    }
    /// Start the widget over from the editor's source and caret.
    pub(super) fn reset_input(&mut self) {
        self.source_input = TextInput::new(self.document.editor.source().to_owned());
        self.source_input.set_caret_selection(self.document.editor.selection());
    }
    /// Send the widget's edits to the editor as messages: the smallest
    /// replacement, then the caret.
    pub(super) fn push_input(&mut self) {
        let (start, end, replacement) =
            minimal_replacement(self.document.editor.source(), self.source_input.text());
        let typed = start != end || !replacement.is_empty();
        if typed {
            let at = |byte| CaretPosition { byte, affinity: CaretAffinity::Downstream };
            let range = CaretSelection { anchor: at(start), focus: at(end) };
            self.document.editor.apply(TextCommand::SetSelection(range));
            self.document.editor.apply(TextCommand::Insert(replacement));
        }
        let caret = self.source_input.caret_selection();
        if typed || self.document.editor.selection() != caret {
            self.document.editor.apply(TextCommand::SetSelection(caret));
        }
    }
    /// Take the editor's source back into the widget when a structural edit,
    /// undo or redo changed it; the widget keeps its own history otherwise.
    pub(super) fn pull_input(&mut self) {
        if self.source() != self.document.editor.source() {
            self.reset_input();
        }
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

    #[test]
    fn typing_in_the_pane_widget_reaches_the_editor_and_undo_comes_back() {
        let mut pane = pane(None);
        let before = pane.source().to_owned();
        pane.source_input.apply(TextCommand::Insert("!".into()));
        assert!(pane.dirty(), "typing not yet sent still counts");
        assert_eq!(pane.document.editor.source(), before, "the editor is told at the next edit");
        pane.edit(|d| d.editor.apply(TextCommand::Undo));
        assert_eq!(pane.source(), before, "the editor's undo took the typing back");
        assert_eq!(pane.document.editor.source(), before);
        assert!(!pane.dirty());
        pane.edit(|d| d.editor.apply(TextCommand::Redo));
        assert!(pane.source().contains('!') && pane.dirty());
    }
}
