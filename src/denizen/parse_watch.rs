// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// The address a pack asks to wake on: the first `-- @watch <url>` line.
///
/// A comment so an undeclared pack is still ordinary Lua, and a header so it
/// is visible in the first screenful of the file a reviewer reads.
pub fn parse_watch(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let rest = line
            .trim()
            .strip_prefix("--")?
            .trim()
            .strip_prefix("@watch")?;
        let url = rest.trim();
        (!url.is_empty()).then(|| url.to_string())
    })
}
