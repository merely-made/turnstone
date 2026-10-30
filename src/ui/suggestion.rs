// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// One suggestion row.
#[derive(Clone, Debug, PartialEq)]
pub enum Suggestion {
    /// An existing graph node: committing selects it (no fetch).
    Node {
        id: Uuid,
        url: String,
        label: String,
        host: String,
    },
    /// Open this address (mint-or-select + fetch).
    Go { url: String },
    /// An app intent from the `>` lane: the palette registry, plus dynamic
    /// entries (the session switcher) — hence an owned label.
    Act {
        label: String,
        action: crate::action::Action,
    },
    /// A page this persona visited before, recalled from browsing memory and
    /// no longer in the graph. Committing opens it (mint-or-select + fetch),
    /// the same lowering as `Go` — the difference is provenance, not effect.
    Recall {
        url: String,
        title: Option<String>,
        at_ms: u64,
    },
    /// A muted hint row (empty states).
    Hint(&'static str),
    /// A protocol prompt. Dynamic text, but never a committable suggestion.
    Prompt(String),
}
