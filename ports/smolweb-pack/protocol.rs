// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(crate) const COMMAND: &str = "--gemtext-v1";
pub(crate) const REQUEST_LIMIT: u64 = 4 * 1024 * 1024;
pub(crate) const REPLY_LIMIT: u64 = 16 * 1024 * 1024;

pub(crate) fn accepts(input: &inker::EngineInput) -> bool {
    input.address.starts_with("gemini://")
        && input.content_type.as_deref().is_none_or(|mime| {
            mime.split(';')
                .next()
                .is_some_and(|essence| essence.trim().eq_ignore_ascii_case("text/gemini"))
        })
}
