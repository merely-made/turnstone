// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Relays an explicitly selected, product-redacted Turnstone capture as
//! read-only tooling JSON. The format has no stability promise. The adapter
//! deals only in export bytes, so it cannot fetch a node's text or run actions.

use std::io::{self, Read};

use chirograph::Carrier;
use serde_json::Value;

use crate::Server;

pub(super) const UXTREE_URI: &str = "uxtree://snapshot";
const MAX_SNAPSHOT_BYTES: u64 = 8 * 1024 * 1024;

impl<C: Carrier> Server<C> {
    /// Attach bytes exported by Turnstone's `record-uxtree` scenario step.
    /// The caller selects and trusts the product export. Read it once at
    /// startup so resource requests cannot choose or reopen files.
    pub fn with_uxtree_snapshot(mut self, input: impl Read) -> io::Result<Self> {
        let mut text = String::new();
        input
            .take(MAX_SNAPSHOT_BYTES + 1)
            .read_to_string(&mut text)?;
        if text.len() as u64 > MAX_SNAPSHOT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "uxtree snapshot exceeds 8 MiB",
            ));
        }
        let capture: Value = serde_json::from_str(&text).map_err(io::Error::other)?;
        if capture["kind"] != "turnstone-uxtree-snapshot" || !capture["trees"].is_array() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "expected a Turnstone record-uxtree export",
            ));
        }
        self.uxtree_snapshot = Some(text);
        Ok(self)
    }
}
