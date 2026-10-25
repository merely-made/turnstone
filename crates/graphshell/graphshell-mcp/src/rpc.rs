// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! JSON-RPC framing and the two MCP eras.
//!
//! The current revision (2026-07-28) is stateless: every request carries its
//! protocol version in `params._meta`, and `server/discover` replaces the
//! handshake. Claude Code and Codex still open with `initialize`, so the server
//! is dual-era (versioning page, "Backward Compatibility"): an `initialize`
//! request selects legacy semantics for the process, and a request carrying
//! the version key is served statelessly. Neither era changes what a method
//! means here, only the envelope.

use serde_json::{Value, json};

pub const MODERN: &str = "2026-07-28";
/// Handshake-era revisions this server will name in `initialize`, newest first.
pub const LEGACY: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";

pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL: i64 = -32603;
pub const UNSUPPORTED_VERSION: i64 = -32022;

/// A JSON-RPC error, before it is given an id.
#[derive(Debug)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(INVALID_PARAMS, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(INTERNAL, message)
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// Which envelope a request used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Era {
    Modern,
    Legacy,
}

/// The era of a non-handshake request, or the error that refuses it.
///
/// `legacy_open` is whether `initialize` has been answered. A request with no
/// version key is served only after that; before it, the error names both ways
/// in, since a client that cannot get a diagnostic out of a refusal has no
/// fall-forward.
pub fn era_of(params: &Value, legacy_open: bool) -> Result<Era, RpcError> {
    match params
        .pointer("/_meta")
        .and_then(|meta| meta.get(VERSION_KEY))
    {
        Some(Value::String(version)) if version == MODERN => Ok(Era::Modern),
        Some(requested) => Err(
            RpcError::new(UNSUPPORTED_VERSION, "Unsupported protocol version").with_data(json!({
                "supported": supported_versions(),
                "requested": requested,
            })),
        ),
        None if legacy_open => Ok(Era::Legacy),
        None => Err(RpcError::invalid_params(format!(
            "no protocol version: send `initialize`, or put \"{VERSION_KEY}\" in params._meta \
             (supported: {})",
            supported_versions().join(", ")
        ))),
    }
}

pub fn supported_versions() -> Vec<&'static str> {
    std::iter::once(MODERN).chain(LEGACY).collect()
}

/// The version `initialize` answers: the client's own if this server speaks
/// it, else the newest handshake-era one.
pub fn negotiate_legacy(params: &Value) -> &'static str {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    LEGACY
        .iter()
        .copied()
        .find(|version| Some(*version) == asked)
        .unwrap_or(LEGACY[0])
}

/// A result in the shape the request's era expects.
pub fn complete(era: Era, mut result: Value) -> Value {
    if era == Era::Modern
        && let Some(object) = result.as_object_mut()
    {
        object.insert("resultType".into(), json!("complete"));
    }
    result
}

/// Whether a result may be cached by the client. Nothing here may: a mere's
/// projection changes under other participants and the grant can lapse.
pub fn uncacheable(era: Era, mut result: Value) -> Value {
    if era == Era::Modern
        && let Some(object) = result.as_object_mut()
    {
        object.insert("ttlMs".into(), json!(0));
        object.insert("cacheScope".into(), json!("private"));
    }
    complete(era, result)
}

pub fn success(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

pub fn failure(id: &Value, error: RpcError) -> Value {
    let mut body = json!({"code": error.code, "message": error.message});
    if let Some(data) = error.data {
        body["data"] = data;
    }
    json!({"jsonrpc": "2.0", "id": id, "error": body})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_modern_request_is_served_and_an_unknown_version_is_refused_with_the_list() {
        let modern = json!({"_meta": {VERSION_KEY: MODERN}});
        assert_eq!(era_of(&modern, false).unwrap(), Era::Modern);
        let other = json!({"_meta": {VERSION_KEY: "1900-01-01"}});
        let error = era_of(&other, true).err().unwrap();
        assert_eq!(error.code, UNSUPPORTED_VERSION);
        assert_eq!(error.data.unwrap()["requested"], "1900-01-01");
    }

    #[test]
    fn a_bare_request_needs_the_handshake_first() {
        assert!(era_of(&json!({}), false).is_err());
        assert_eq!(era_of(&json!({}), true).unwrap(), Era::Legacy);
    }

    #[test]
    fn initialize_echoes_a_known_version_and_names_the_newest_otherwise() {
        assert_eq!(
            negotiate_legacy(&json!({"protocolVersion": "2025-03-26"})),
            "2025-03-26"
        );
        assert_eq!(
            negotiate_legacy(&json!({"protocolVersion": "2031-01-01"})),
            LEGACY[0]
        );
    }
}
