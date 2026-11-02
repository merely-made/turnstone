// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl OmnibarState {
    /// The highlighted suggestion, if any commit-able row is present.
    pub fn selection(&self) -> Option<&Suggestion> {
        self.suggestions
            .get(self.selected)
            .filter(|s| !matches!(s, Suggestion::Hint(_) | Suggestion::Prompt(_)))
    }

    pub fn sensitive(&self) -> bool {
        match &self.mode {
            OmnibarMode::SmolwebInput(SmolwebInputPrompt {
                sensitive: true, ..
            })
            | OmnibarMode::SmolwebSubmission(SmolwebSubmissionPrompt {
                stage: SmolwebSubmissionStage::Token,
                ..
            }) => true,
            OmnibarMode::MicronForm(MicronFormPrompt {
                stage: MicronFormStage::Field(index),
                form,
                ..
            }) => form
                .fields()
                .get(*index)
                .is_some_and(|field| field.masked()),
            _ => false,
        }
    }

    /// The line as chrome, accessibility, and observation may expose it.
    pub fn presented_text(&self) -> String {
        if self.sensitive() {
            "\u{2022}".repeat(self.text.chars().count())
        } else {
            self.text.clone()
        }
    }

    /// The caret offset in the presented string. A bullet is three UTF-8
    /// bytes, so the private buffer's byte index cannot be reused directly.
    pub fn presented_cursor(&self) -> usize {
        if self.sensitive() {
            self.text[..self.cursor].chars().count() * '\u{2022}'.len_utf8()
        } else {
            self.cursor
        }
    }

    pub fn presented_preedit(&self) -> Option<String> {
        self.preedit.as_ref().map(|preedit| {
            if self.sensitive() {
                "\u{2022}".repeat(preedit.chars().count())
            } else {
                preedit.clone()
            }
        })
    }

    /// Insert `s` at the caret and advance it.
    pub fn insert_str(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    /// Delete the character before the caret. `false` at the line start.
    pub fn backspace(&mut self) -> bool {
        match self.text[..self.cursor].char_indices().last() {
            Some((i, _)) => {
                self.text.remove(i);
                self.cursor = i;
                true
            },
            None => false,
        }
    }

    /// Delete the character after the caret. `false` at the line end.
    pub fn delete_forward(&mut self) -> bool {
        if self.cursor < self.text.len() {
            self.text.remove(self.cursor);
            true
        } else {
            false
        }
    }

    /// Move the caret, staying on char boundaries.
    pub fn move_caret(&mut self, m: crate::action::CaretMove) {
        use crate::action::CaretMove;
        self.cursor = match m {
            CaretMove::Home => 0,
            CaretMove::End => self.text.len(),
            CaretMove::Left => self.text[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0),
            CaretMove::Right => self.text[self.cursor..]
                .chars()
                .next()
                .map(|c| self.cursor + c.len_utf8())
                .unwrap_or(self.cursor),
        };
    }
}
