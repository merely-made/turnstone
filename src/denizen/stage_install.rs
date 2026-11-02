// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Stage a `.lua` file as a pending install: read it, derive the subject,
/// and surface the review. `Err` is a human-readable refusal (unreadable
/// file, empty source).
pub fn stage_install(path: &Path) -> Result<PendingInstall, String> {
    let bytes = std::fs::read(path).map_err(|err| format!("unreadable pack: {err}"))?;
    if bytes.is_empty() {
        return Err("the pack is empty".to_string());
    }
    // The subject is the bytes' blake3 either way: the same pack is the same
    // participant whichever lane runs it, and an edited pack faces a fresh review.
    let subject = Subject::new(*blake3::hash(&bytes).as_bytes());
    let is_component = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("wasm"));
    let body = if is_component {
        PackBody::Component(bytes)
    } else {
        let source = String::from_utf8(bytes).map_err(|_| {
            "the pack is not valid UTF-8 (a component must end in .wasm)".to_string()
        })?;
        if source.trim().is_empty() {
            return Err("the pack is empty".to_string());
        }
        PackBody::Scenario(source)
    };
    let (watch_url, deadband) = match &body {
        PackBody::Scenario(source) => (parse_watch(source), parse_deadband(source)?),
        // A component declares no watch yet: its bytes carry no comment lane.
        PackBody::Component(_) => (None, None),
    };
    let label = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("participant")
        .to_string();
    Ok(PendingInstall {
        path: path.to_path_buf(),
        label,
        body,
        subject,
        rings: default_rings(),
        watch_url,
        deadband,
    })
}
