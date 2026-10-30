// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Address-shaped input, normalized to an openable URL: a `scheme://` form
/// passes through; a dotted bare host gets `https://`. `None` for anything
/// else (a future search lane decides what non-addresses mean).
pub fn normalize_address(text: &str) -> Option<String> {
    if crate::nomadnet::parse_address(text).is_some() {
        return Some(text.to_string());
    }
    if text.contains("://") {
        return Some(text.to_string());
    }
    let host_like =
        !text.contains(' ') && text.contains('.') && !text.starts_with('.') && !text.ends_with('.');
    host_like.then(|| format!("https://{text}"))
}
