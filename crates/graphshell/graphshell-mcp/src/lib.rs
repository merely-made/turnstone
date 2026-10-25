// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A stdio Model Context Protocol server over one admitted Graphshell route.
//!
//! An agent harness (Claude Code, Codex) launches this as a subprocess and
//! reads and writes a mere through it. The server is an adapter and nothing
//! more: it holds no key, mints no grant and decides nothing. It speaks to the
//! route through any [`chirograph::Carrier`], and the route's owner (djinn, on
//! the owner-only application door) admits the app by id, records which routes
//! that app may reach, and attributes each accepted edit to the persona *via*
//! that app. A refusal there is a refusal here. Doctrine Stack 2: identity and
//! trust come from the stack.
//!
//! - Each projection the route offers is a **resource**, read as one JSON
//!   document ([`view`]).
//! - Writes ride one stable **tool**, `mere_invoke`, an envelope of target,
//!   intent, payload and the revision the caller observed, so the tool surface
//!   does not change when the route's actions do (the ENVELOPE lane's reason).
//!   `mere_actions` lists what the route advertises, with each payload schema.
//!   [`Config::writes`] off leaves only the read tools.
//!
//! The wire is MCP 2026-07-28 and, because harnesses still open with
//! `initialize`, the handshake-era revisions ([`rpc`]).

mod rpc;
mod tools;
pub mod view;

use std::io::{self, BufRead, Write};

use chirograph::{
    CapabilityProfile, Carrier, CarrierError, CarrierRequestBody, CarrierResponseBody, ContentHash,
    EndpointDescriptor, PresentationCapability, PresentationOffer, ProjectionOffer,
    ProjectionSession, ProtocolVersion, ResourceAssembly, ResourceChunkRequest, SessionOpen,
};
use rpc::{Era, RpcError};
use serde_json::{Value, json};

/// How the server presents itself and how much it will do.
#[derive(Clone, Debug)]
pub struct Config {
    /// The name in `serverInfo`.
    pub name: String,
    /// Names the route in resource URIs: `mere://<scope>/<projection>`.
    pub scope: String,
    /// Offer `mere_invoke`. Off, the server reads and never writes.
    pub writes: bool,
    /// The largest card or text body fetched into a resource, in bytes.
    pub max_content_bytes: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: "mere-mcp".into(),
            scope: "mere".into(),
            writes: true,
            max_content_bytes: 256 * 1024,
        }
    }
}

const INSTRUCTIONS: &str = "Reads and writes a mere as a granted participant. \
Resources are the route's projections: instances with their cards and text. \
To change something, call mere_actions to see what is advertised (each action \
has a payload schema) and the epoch and revision it was read at, then \
mere_invoke with that target, intent, payload and observed epoch and revision. \
Every accepted write is attributed to the app this server was admitted as. A \
refusal is the owner's grant speaking; do not try to route around it.";

/// One MCP server over one carrier. Single-threaded: one request at a time, in
/// the order the client sent them.
pub struct Server<C> {
    pub(crate) carrier: C,
    pub(crate) config: Config,
    opened: Option<Vec<ProjectionOffer>>,
    legacy_open: bool,
    gone: bool,
}

impl<C: Carrier> Server<C> {
    pub fn new(carrier: C, config: Config) -> Self {
        Self {
            carrier,
            config,
            opened: None,
            legacy_open: false,
            gone: false,
        }
    }

    /// The carrier, for a host that also drives it.
    pub fn carrier(&self) -> &C {
        &self.carrier
    }

    /// True once the carrier has disconnected; a host should stop serving and
    /// let the client restart the process, which is how a stateless server
    /// recovers.
    pub fn finished(&self) -> bool {
        self.gone
    }

    /// Serve newline-delimited JSON-RPC until the input closes or the carrier
    /// is gone. Stdin closing is the client's shutdown signal.
    pub fn serve(&mut self, input: impl BufRead, mut output: impl Write) -> io::Result<()> {
        for line in input.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Some(reply) = self.handle(&line) {
                writeln!(output, "{reply}")?;
                output.flush()?;
            }
            if self.gone {
                break;
            }
        }
        let _ = self.carrier.shutdown();
        Ok(())
    }

    /// Answer one line. `None` for a notification, which gets no reply.
    pub fn handle(&mut self, line: &str) -> Option<String> {
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(Value::Array(batch)) => {
                let replies: Vec<Value> = batch.iter().filter_map(|m| self.message(m)).collect();
                (!replies.is_empty()).then_some(Value::Array(replies))
            },
            Ok(message) => self.message(&message),
            Err(error) => Some(rpc::failure(
                &Value::Null,
                RpcError::new(-32700, format!("parse error: {error}")),
            )),
        };
        reply.map(|reply| reply.to_string())
    }

    fn message(&mut self, message: &Value) -> Option<Value> {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // A response, which a server never asked for; or garbage.
            return message.get("id").is_none().then(|| {
                rpc::failure(
                    &Value::Null,
                    RpcError::new(rpc::INVALID_REQUEST, "no method"),
                )
            });
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let id = message.get("id")?;
        Some(match self.dispatch(method, &params) {
            Ok(result) => rpc::success(id, result),
            Err(error) => rpc::failure(id, error),
        })
    }

    fn dispatch(&mut self, method: &str, params: &Value) -> Result<Value, RpcError> {
        match method {
            "initialize" => {
                self.legacy_open = true;
                Ok(json!({
                    "protocolVersion": rpc::negotiate_legacy(params),
                    "capabilities": self.capabilities(),
                    "serverInfo": self.server_info(),
                    "instructions": INSTRUCTIONS,
                }))
            },
            "ping" => Ok(json!({})),
            "server/discover" => {
                let era = match params.pointer("/_meta/io.modelcontextprotocol~1protocolVersion") {
                    Some(_) => rpc::era_of(params, false)?,
                    None => Era::Modern,
                };
                Ok(rpc::uncacheable(
                    era,
                    json!({
                        "supportedVersions": rpc::supported_versions(),
                        "capabilities": self.capabilities(),
                        "_meta": {"io.modelcontextprotocol/serverInfo": self.server_info()},
                        "instructions": INSTRUCTIONS,
                    }),
                ))
            },
            _ => {
                let era = rpc::era_of(params, self.legacy_open)?;
                match method {
                    "resources/list" => self.list_resources(era),
                    "resources/read" => self.read_resource(era, params),
                    "resources/templates/list" => {
                        Ok(rpc::uncacheable(era, json!({"resourceTemplates": []})))
                    },
                    "tools/list" => Ok(rpc::uncacheable(era, json!({"tools": self.tool_list()}))),
                    "tools/call" => self.call_tool(era, params),
                    _ => Err(RpcError::new(
                        rpc::METHOD_NOT_FOUND,
                        format!("method not found: {method}"),
                    )),
                }
            },
        }
    }

    fn capabilities(&self) -> Value {
        json!({"tools": {}, "resources": {}})
    }

    fn server_info(&self) -> Value {
        json!({"name": self.config.name, "version": env!("CARGO_PKG_VERSION")})
    }

    fn list_resources(&mut self, era: Era) -> Result<Value, RpcError> {
        let resources: Vec<Value> = self
            .projections()?
            .iter()
            .map(|offer| {
                json!({
                    "uri": self.uri_of(&offer.label),
                    "name": offer.label,
                    "description": "A projection of the mere, as the granted participant sees it.",
                    "mimeType": "application/json",
                })
            })
            .collect();
        Ok(rpc::uncacheable(era, json!({"resources": resources})))
    }

    fn read_resource(&mut self, era: Era, params: &Value) -> Result<Value, RpcError> {
        let uri = params
            .get("uri")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::invalid_params("resources/read needs a uri"))?;
        let offer = self.find_projection(uri)?;
        let document = self.read_projection(&offer)?;
        Ok(rpc::uncacheable(
            era,
            json!({"contents": [{
                "uri": uri,
                "mimeType": "application/json",
                "text": document.to_string(),
            }]}),
        ))
    }

    /// The projection document, with card and text bodies fetched through the
    /// carrier (bounded by [`Config::max_content_bytes`]).
    pub(crate) fn read_projection(&mut self, offer: &ProjectionOffer) -> Result<Value, RpcError> {
        let snapshot = self.snapshot(offer)?;
        let (session, limit) = (snapshot.session.clone(), self.config.max_content_bytes);
        Ok(view::digest(&snapshot, &mut |offer: &PresentationOffer| {
            (offer.byte_size <= limit)
                .then(|| self.fetch(&session, offer.resource))
                .flatten()
        }))
    }

    pub(crate) fn snapshot(
        &mut self,
        offer: &ProjectionOffer,
    ) -> Result<chirograph::ProjectionSnapshot, RpcError> {
        match self.ask(CarrierRequestBody::Snapshot(offer.request.clone()))? {
            CarrierResponseBody::Snapshot(snapshot) => Ok(*snapshot),
            other => Err(RpcError::internal(format!(
                "expected a snapshot, got {other:?}"
            ))),
        }
    }

    /// One resource's bytes, or `None` if the endpoint will not give them.
    fn fetch(&mut self, session: &ProjectionSession, resource: ContentHash) -> Option<Vec<u8>> {
        let mut assembly = ResourceAssembly::new();
        loop {
            let body = self
                .carrier
                .request(CarrierRequestBody::ResourceChunk(ResourceChunkRequest {
                    session: session.clone(),
                    resource,
                    offset: assembly.received(),
                    length: chirograph::MAX_RESOURCE_CHUNK_BYTES,
                }))
                .map_err(|error| self.note(error))
                .ok()?;
            let CarrierResponseBody::ResourceChunk(chunk) = body else {
                return None;
            };
            if let Some(bytes) = assembly.accept(&chunk).ok()? {
                return Some(bytes);
            }
        }
    }

    /// The route's projections, opening the session on first use. The
    /// descriptor is kept: a second `Open` could reset a session another
    /// reader depends on.
    pub(crate) fn projections(&mut self) -> Result<Vec<ProjectionOffer>, RpcError> {
        if self.opened.is_none() {
            let open = SessionOpen {
                version: ProtocolVersion::V1,
                capabilities: CapabilityProfile::new([
                    PresentationCapability::PortableCard,
                    PresentationCapability::EditableText,
                ]),
            };
            let opened = match self.ask(CarrierRequestBody::Open(Box::new(open)))? {
                CarrierResponseBody::Opened(opened) => *opened,
                other => {
                    return Err(RpcError::internal(format!(
                        "expected an open, got {other:?}"
                    )));
                },
            };
            let EndpointDescriptor { projections, .. } = opened.descriptor;
            self.opened = Some(projections);
        }
        Ok(self.opened.clone().unwrap_or_default())
    }

    pub(crate) fn uri_of(&self, label: &str) -> String {
        format!("mere://{}/{}", self.config.scope, percent_encode(label))
    }

    /// A projection by label or by its resource URI.
    pub(crate) fn find_projection(&mut self, name: &str) -> Result<ProjectionOffer, RpcError> {
        self.projections()?
            .into_iter()
            .find(|offer| offer.label == name || self.uri_of(&offer.label) == name)
            .ok_or_else(|| {
                RpcError::invalid_params("Resource not found").with_data(json!({"uri": name}))
            })
    }

    pub(crate) fn ask(
        &mut self,
        body: CarrierRequestBody,
    ) -> Result<CarrierResponseBody, RpcError> {
        self.carrier
            .request(body)
            .map_err(|error| RpcError::internal(self.note(error)))
    }

    /// Record what a carrier error means for the process; return its message.
    pub(crate) fn note(&mut self, error: CarrierError) -> String {
        match error {
            CarrierError::Refused(message) => message,
            CarrierError::Disconnected(message) => {
                self.gone = true;
                format!("the resident closed the session: {message}")
            },
        }
    }
}

fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            },
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
