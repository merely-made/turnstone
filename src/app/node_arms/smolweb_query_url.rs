// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Replace a smolweb input target's query with one UTF-8, percent-encoded
/// answer. Form-style `+` encoding is wrong here: Gemini input is a URL query,
/// where a space is `%20`.
pub(crate) fn smolweb_query_url(input_url: &str, answer: &str) -> Option<String> {
    use std::fmt::Write as _;

    let mut parsed = url::Url::parse(input_url).ok()?;
    parsed.set_query(None);
    parsed.set_fragment(None);
    let mut target = parsed.to_string();
    target.push('?');
    for byte in answer.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            target.push(*byte as char);
        } else {
            let _ = write!(target, "%{byte:02X}");
        }
    }
    Some(target)
}
