// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A suggestion row's display text (the same rendering the hand chrome drew).
pub(crate) fn row_text(s: &Suggestion) -> String {
    match s {
        Suggestion::Node { label, host, .. } if host.is_empty() => label.clone(),
        Suggestion::Node { label, host, .. } => format!("{label}  \u{00b7}  {host}"),
        Suggestion::Go { url } => format!("\u{2192} open {url}"),
        // The clock face marks provenance: this row came from what you
        // visited, not from what is on the canvas or what you just typed.
        Suggestion::Recall { url, title, .. } => match title {
            Some(title) if !title.trim().is_empty() => {
                format!("\u{1f552} {title}  \u{00b7}  {url}")
            },
            _ => format!("\u{1f552} {url}"),
        },
        Suggestion::Act { label, .. } => format!("\u{203a} {label}"),
        Suggestion::Hint(hint) => (*hint).to_string(),
        Suggestion::Prompt(prompt) => prompt.clone(),
    }
}
