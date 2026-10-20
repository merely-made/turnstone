// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Bounded structural edits over Graphshell's existing local Knot editor.
//!
//! A preview records the exact editor source and one parser-produced Djot div.
//! Applying it replaces only that div's attribute opening through the same
//! `KnotEditor` buffer and undo journal used for ordinary text edits.

use cambium::{CaretAffinity, CaretPosition, CaretSelection, TextCommand};
use illume::blocks;
use knot_editor_host::KnotEditor;
use std::collections::BTreeMap;

pub use illume::blocks::{Div, wrap};

pub mod bars;
pub mod rhyme;
pub mod store;

/// Maximum source size admitted by preview and structural apply operations.
pub const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub index: usize,
    pub label: String,
    pub before: Option<String>,
    pub after: String,
    pub conflict: bool,
    pub use_update: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    /// Exact source snapshot from which this preview was derived.
    pub source: String,
    /// Parser-produced target div.
    pub block: Div,
    /// Provider-suggested changes. User decisions are supplied separately to
    /// [`Document::apply`] as the complete desired attribute map.
    pub changes: Vec<Change>,
}

impl Preview {
    /// Bind proposals to a target that exists exactly in the given snapshot.
    pub fn new(source: &str, block: Div, changes: Vec<Change>) -> Result<Self, String> {
        ensure_bounded(source)?;
        if !contains_div(&blocks::divs(source), &block) {
            return Err("preview target does not match a div in its source".into());
        }
        Ok(Self {
            source: source.to_owned(),
            block,
            changes,
        })
    }
}

pub struct Document {
    pub id: String,
    pub editor: KnotEditor,
}

impl Document {
    pub fn new(id: &str, source: &str) -> Self {
        Self {
            id: id.to_owned(),
            editor: KnotEditor::scratch(id, source),
        }
    }

    /// Return all parser-produced divs in document preorder.
    pub fn blocks(&self) -> Vec<Div> {
        if self.editor.source().len() > MAX_DOCUMENT_BYTES {
            return Vec::new();
        }
        let roots = blocks::divs(self.editor.source());
        let mut flattened = Vec::new();
        flatten(&roots, &mut flattened);
        flattened
    }

    /// Wrap selected complete lines through the existing text edit history.
    pub fn wrap_selection(&mut self, attributes: BTreeMap<String, String>) -> Result<(), String> {
        let source = self.editor.source();
        ensure_bounded(source)?;
        blocks::validate_attributes(&attributes)?;
        let (a, b) = self.editor.input().selection_bytes();
        if a == b || a > b || b > source.len() {
            return Err("Select composition text in the source editor first".into());
        }
        if !source.is_char_boundary(a) || !source.is_char_boundary(b) {
            return Err("Selection does not align with UTF-8 source boundaries".into());
        }
        let start = source[..a].rfind('\n').map_or(0, |i| i + 1);
        let end = if source.as_bytes().get(b - 1) == Some(&b'\n') {
            b
        } else {
            source[b..].find('\n').map_or(source.len(), |i| b + i + 1)
        };
        let body = &source[start..end];
        if body.trim().is_empty() {
            return Err("Select nonempty source text to wrap".into());
        }
        let suffix = &source[end..];
        let mut replacement = String::new();
        if start > 0 && !source[..start].ends_with("\n\n") && !source[..start].ends_with("\r\n\r\n")
        {
            replacement.push('\n');
        }
        replacement.push_str(&blocks::wrap(&attributes, body));
        if !suffix.is_empty() && !suffix.starts_with('\n') && !suffix.starts_with("\r\n") {
            replacement.push('\n');
        }
        if source.len() - (end - start) + replacement.len() > MAX_DOCUMENT_BYTES {
            return Err("Document exceeds one MiB".into());
        }
        self.editor.apply(TextCommand::SetSelection(CaretSelection {
            anchor: CaretPosition {
                byte: start,
                affinity: CaretAffinity::Downstream,
            },
            focus: CaretPosition {
                byte: end,
                affinity: CaretAffinity::Upstream,
            },
        }));
        self.editor.apply(TextCommand::Insert(replacement));
        Ok(())
    }

    /// Apply a selected complete attribute map as one ordinary editor edit.
    /// The exact source snapshot and exact parser target are revalidated at
    /// application time; no body or nested content is serialized.
    pub fn apply(
        &mut self,
        preview: &Preview,
        attributes: BTreeMap<String, String>,
    ) -> Result<(), String> {
        let source = self.editor.source();
        ensure_bounded(source)?;
        if source != preview.source {
            return Err("document changed after this preview was created".into());
        }
        if !contains_div(&blocks::divs(source), &preview.block) {
            return Err("preview target does not match the current document".into());
        }
        if attributes == preview.block.attributes {
            return Ok(());
        }

        let updated = blocks::update_opening(source, &preview.block, &attributes)?;
        ensure_bounded(&updated)?;
        if updated == source {
            return Ok(());
        }

        // Diff to the smallest UTF-8-aligned replacement. Selection changes
        // break typing coalescence but do not create journal entries, so the
        // following single insert is one undo step.
        let (start, old_end, replacement) = minimal_replacement(source, &updated);
        self.editor.apply(TextCommand::SetSelection(CaretSelection {
            anchor: CaretPosition {
                byte: start,
                affinity: CaretAffinity::Downstream,
            },
            focus: CaretPosition {
                byte: old_end,
                affinity: CaretAffinity::Downstream,
            },
        }));
        self.editor.apply(TextCommand::Insert(replacement));
        Ok(())
    }
}

fn ensure_bounded(source: &str) -> Result<(), String> {
    if source.len() > MAX_DOCUMENT_BYTES {
        Err(format!("document exceeds {MAX_DOCUMENT_BYTES} byte limit"))
    } else {
        Ok(())
    }
}

fn contains_div(nodes: &[Div], target: &Div) -> bool {
    nodes
        .iter()
        .any(|node| node == target || contains_div(&node.children, target))
}

fn flatten(nodes: &[Div], output: &mut Vec<Div>) {
    for node in nodes {
        output.push(node.clone());
        flatten(&node.children, output);
    }
}

fn minimal_replacement(before: &str, after: &str) -> (usize, usize, String) {
    let prefix = before
        .chars()
        .zip(after.chars())
        .take_while(|(left, right)| left == right)
        .map(|(character, _)| character.len_utf8())
        .sum::<usize>();

    let before_tail = &before[prefix..];
    let after_tail = &after[prefix..];
    let suffix = before_tail
        .chars()
        .rev()
        .zip(after_tail.chars().rev())
        .take_while(|(left, right)| left == right)
        .map(|(character, _)| character.len_utf8())
        .sum::<usize>();
    let old_end = before.len() - suffix;
    let new_end = after.len() - suffix;
    (prefix, old_end, after[prefix..new_end].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_changes() -> Vec<Change> {
        Vec::new()
    }

    fn attrs(items: &[(&str, &str)]) -> BTreeMap<String, String> {
        items
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn select_bytes(document: &mut Document, start: usize, end: usize) {
        document
            .editor
            .apply(TextCommand::SetSelection(CaretSelection {
                anchor: CaretPosition {
                    byte: start,
                    affinity: CaretAffinity::Downstream,
                },
                focus: CaretPosition {
                    byte: end,
                    affinity: CaretAffinity::Upstream,
                },
            }));
    }

    #[test]
    fn text_edit_then_structural_edit_undoes_independently_and_redoes() {
        let source = "outside α\n\n{#outer owner=human}\n:::: outer\ninside 👩‍🚀\n\n{#inner keep=yes}\n::: child\nbody 漢\n:::\n::::\n";
        let mut document = Document::new("memory:test", source);
        document.editor.apply(TextCommand::Insert("!".into()));
        let edited_source = document.editor.source().to_owned();
        let block = document.blocks()[0].clone();
        let preview = Preview::new(&edited_source, block, empty_changes()).unwrap();
        document
            .apply(
                &preview,
                attrs(&[("id", "outer"), ("owner", "human"), ("status", "ready")]),
            )
            .unwrap();
        let structural_source = document.editor.source().to_owned();
        assert!(
            structural_source.contains("inside 👩‍🚀\n\n{#inner keep=yes}\n::: child\nbody 漢\n:::\n")
        );
        assert!(structural_source.contains("{id=\"outer\" owner=\"human\" status=\"ready\"}"));

        document.editor.apply(TextCommand::Undo);
        assert_eq!(document.editor.source(), edited_source);
        document.editor.apply(TextCommand::Undo);
        assert_eq!(document.editor.source(), source);
        document.editor.apply(TextCommand::Redo);
        assert_eq!(document.editor.source(), edited_source);
        document.editor.apply(TextCommand::Redo);
        assert_eq!(document.editor.source(), structural_source);
    }

    #[test]
    fn stale_forged_and_invalid_targets_are_rejected() {
        let source = "::: card\nbody\n:::\n";
        let mut document = Document::new("memory:test", source);
        let block = document.blocks()[0].clone();
        let preview = Preview::new(source, block.clone(), empty_changes()).unwrap();
        document.editor.apply(TextCommand::Insert("!".into()));
        assert!(document.apply(&preview, attrs(&[("label", "x")])).is_err());

        let mut forged = block;
        forged.attributes.insert("forged".into(), "target".into());
        assert!(Preview::new(source, forged, empty_changes()).is_err());

        let mut fresh = Document::new("memory:test", source);
        let valid = Preview::new(source, fresh.blocks()[0].clone(), empty_changes()).unwrap();
        assert!(fresh.apply(&valid, attrs(&[("bad key", "x")])).is_err());
        assert!(
            fresh
                .apply(&valid, attrs(&[("label", "two\nlines")]))
                .is_err()
        );
    }

    #[test]
    fn no_declarations_do_not_rewrite_body_and_quote_prefix_is_kept() {
        let source = "> ::: card\n> body α\n> :::\n";
        let mut document = Document::new("memory:test", source);
        let preview = Preview::new(source, document.blocks()[0].clone(), empty_changes()).unwrap();
        document.apply(&preview, BTreeMap::new()).unwrap();
        assert_eq!(document.editor.source(), source);

        let preview = Preview::new(source, document.blocks()[0].clone(), empty_changes()).unwrap();
        document
            .apply(&preview, attrs(&[("label", "quoted")]))
            .unwrap();
        assert!(
            document
                .editor
                .source()
                .starts_with("> {label=\"quoted\"}\n> ::: card\n")
        );
        assert!(document.editor.source().contains("> body α\n> :::\n"));
    }

    #[test]
    fn unchanged_attributes_keep_noncanonical_source_bytes_without_undo_entry() {
        let source = "{#card  label=\"old\"}\n::: panel\nbody\n:::\n";
        let mut document = Document::new("memory:test", source);
        let preview = Preview::new(source, document.blocks()[0].clone(), empty_changes()).unwrap();
        let unchanged = preview.block.attributes.clone();
        document.apply(&preview, unchanged).unwrap();
        assert_eq!(document.editor.source(), source);
        assert!(!document.editor.apply(TextCommand::Undo).state_changed);
    }

    #[test]
    fn wraps_selected_unicode_lines_then_rule_edit_undoes_in_chronological_steps() {
        let source = "left α\nfirst 🧪\nsecond 漢\nright 👩‍🚀\n";
        let mut document = Document::new("memory:test", source);
        let start = source.find("first").unwrap();
        let end = source.find("right").unwrap();
        select_bytes(&mut document, start, end);

        document
            .wrap_selection(attrs(&[("id", "selection"), ("class", "verse")]))
            .unwrap();
        let wrapped_source = document.editor.source().to_owned();
        assert_eq!(
            wrapped_source,
            "left α\n\n{class=\"verse\" id=\"selection\"}\n:::\nfirst 🧪\nsecond 漢\n:::\n\nright 👩‍🚀\n"
        );
        let wrapped = document.blocks()[0].clone();
        assert_eq!(
            &wrapped_source[wrapped.body.clone()],
            "first 🧪\nsecond 漢\n"
        );

        let preview = Preview::new(&wrapped_source, wrapped, empty_changes()).unwrap();
        let mut rule_attributes = preview.block.attributes.clone();
        rule_attributes.insert("rule".into(), "authored".into());
        document.apply(&preview, rule_attributes).unwrap();
        let ruled_source = document.editor.source().to_owned();
        assert!(ruled_source.contains("rule=\"authored\""));

        document.editor.apply(TextCommand::Undo);
        assert_eq!(document.editor.source(), wrapped_source);
        document.editor.apply(TextCommand::Undo);
        assert_eq!(document.editor.source(), source);
        document.editor.apply(TextCommand::Redo);
        assert_eq!(document.editor.source(), wrapped_source);
        document.editor.apply(TextCommand::Redo);
        assert_eq!(document.editor.source(), ruled_source);
    }

    #[test]
    fn wrapping_refuses_empty_selection_and_invalid_attrs_without_mutation() {
        let source = "one α\ntwo 漢\n";
        let mut document = Document::new("memory:test", source);
        let start = source.find("one").unwrap();
        select_bytes(&mut document, start, start);
        assert!(document.wrap_selection(BTreeMap::new()).is_err());
        assert_eq!(document.editor.source(), source);

        let end = source.len();
        select_bytes(&mut document, 0, end);
        let selected = document.editor.selection();
        assert!(
            document
                .wrap_selection(attrs(&[("bad key", "value")]))
                .is_err()
        );
        assert!(
            document
                .wrap_selection(attrs(&[("label", "line one\nline two")]))
                .is_err()
        );
        assert_eq!(document.editor.source(), source);
        assert_eq!(document.editor.selection(), selected);
        assert!(!document.editor.apply(TextCommand::Undo).state_changed);

        let mut blank = Document::new("memory:blank", "\n");
        select_bytes(&mut blank, 0, 1);
        assert!(blank.wrap_selection(BTreeMap::new()).is_err());
        assert_eq!(blank.editor.source(), "\n");

        let mut empty = Document::new("memory:empty", "");
        assert!(
            empty
                .wrap_selection(attrs(&[("label", "one\ntwo")]))
                .is_err()
        );
        assert_eq!(empty.editor.source(), "");
    }
}
