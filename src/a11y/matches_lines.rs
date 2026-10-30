// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// A single substring, or an ordered subsequence separated by ` >> `.
pub(crate) fn matches_lines(lines: &[String], query: &str) -> bool {
    let mut remaining = lines.iter();
    query
        .split(" >> ")
        .all(|part| !part.is_empty() && remaining.any(|line| line.contains(part)))
}
