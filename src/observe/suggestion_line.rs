// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// One suggestion row as its display string (the assert/a11y rendering).
pub fn suggestion_line(s: &Suggestion) -> String {
    match s {
        Suggestion::Node { label, host, .. } if !host.is_empty() => {
            format!("{label} \u{00b7} {host}")
        },
        Suggestion::Node { label, .. } => label.clone(),
        Suggestion::Go { url } => format!("go {url}"),
        Suggestion::Recall { url, .. } => format!("recall {url}"),
        Suggestion::Act { label, .. } => format!("\u{203a} {label}"),
        Suggestion::Hint(h) => h.to_string(),
        Suggestion::Prompt(prompt) => prompt.clone(),
    }
}
