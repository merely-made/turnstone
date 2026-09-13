// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Native NomadNet page access and the captured Micron presentation projection.
//!
//! A NomadNet address is deliberately not a URL. It keeps the user-facing
//! Reticulum spelling, `destinationhex:/page/path`, and asks Retinue to learn
//! the peer identity by a path request before opening the resource link.

use std::net::SocketAddr;
use std::time::Duration;

use genet_documents::LocalFetcher;
use inker::session_engine::{DocumentSession, SessionEngine, SessionError, SessionSpawnRequest};
use inker::{Block, DocumentDiagnostic, Engine, EngineDocument, EngineInput, InlineSpan};
use mere_document_lanes::{ResourceFetcher, SmolwebDocument, SmolwebDocumentSession, SmolwebTheme};
use netrender::Scene;
use retinue::endpoint::Endpoint;
use retinue::hash::AddressHash;
use retinue::identity::{IDENTITY_LEN, PrivateIdentity};

/// Stable engine id for Turnstone's retained Micron view.
pub const ENGINE_ID: &str = nematic::ENGINE_MICRON;

/// An ordinary NomadNet destination and its opaque absolute page path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NomadNetAddress {
    destination: [u8; 16],
    path: String,
}

impl NomadNetAddress {
    pub fn destination(&self) -> AddressHash {
        AddressHash::from_bytes(self.destination)
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

/// Parse only the native `destinationhex:/absolute/path` spelling.
pub fn parse_address(address: &str) -> Option<NomadNetAddress> {
    let (destination, path) = address.split_once(':')?;
    if destination.len() != 32
        || !destination.bytes().all(|byte| byte.is_ascii_hexdigit())
        || path.is_empty()
        || !path.starts_with('/')
        || path.contains('\0')
    {
        return None;
    }
    let mut bytes = [0_u8; 16];
    for (index, chunk) in destination.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_value(chunk[0])?;
        let low = hex_value(chunk[1])?;
        bytes[index] = (high << 4) | low;
    }
    Some(NomadNetAddress {
        destination: bytes,
        path: path.to_owned(),
    })
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Whether an address belongs to the captured Micron lane.
pub fn is_micron_address(address: &str) -> bool {
    parse_address(address).is_some()
        || address.starts_with("file://")
            && address
                .split_once('?')
                .map_or(address, |(path, _)| path)
                .to_ascii_lowercase()
                .ends_with(".mu")
        || address.starts_with("file://")
            && address
                .split_once('?')
                .map_or(address, |(path, _)| path)
                .to_ascii_lowercase()
                .ends_with(".micron")
}

/// Retained adapter from Nematic's source-preserving Micron engine to document-canvas.
pub struct MicronSessionEngine {
    local_fetcher: LocalFetcher,
}

impl MicronSessionEngine {
    pub fn new() -> Self {
        Self {
            local_fetcher: LocalFetcher,
        }
    }
}

impl Default for MicronSessionEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Native Micron rendering qualifies same-node links with the active
/// destination. Any remaining alias has no node authority in this session.
/// Keep its label visible, but remove the unresolved link before the retained
/// session can offer it to the browser's generic URL resolver.
fn refuse_unresolved_aliases(document: &mut EngineDocument) {
    let mut refused = false;
    for block in &mut document.blocks {
        refused |= refuse_block_aliases(block);
    }
    if refused
        && !document.diagnostics.iter().any(|diagnostic| {
            matches!(diagnostic, DocumentDiagnostic::UnsupportedConstruct(message) if message.contains("document has no NomadNet authority"))
        })
    {
        document.diagnostics.push(DocumentDiagnostic::UnsupportedConstruct(
            "Micron same-node link is inert because the document has no NomadNet authority".into(),
        ));
    }
}

fn refuse_block_aliases(block: &mut Block) -> bool {
    match block {
        Block::Presented { block, .. } => refuse_block_aliases(block),
        Block::Heading { spans, .. } | Block::Paragraph { spans } => refuse_spans(spans),
        Block::Quote { blocks } => {
            let mut refused = false;
            for child in blocks {
                refused |= refuse_block_aliases(child);
            }
            refused
        },
        Block::List { items, .. } => {
            let mut refused = false;
            for item in items {
                for child in item {
                    refused |= refuse_block_aliases(child);
                }
            }
            refused
        },
        Block::Table { header, rows, .. } => {
            let mut refused = false;
            for cell in header {
                refused |= refuse_spans(cell);
            }
            for row in rows {
                for cell in row {
                    refused |= refuse_spans(cell);
                }
            }
            refused
        },
        _ => false,
    }
}

fn refuse_spans(spans: &mut Vec<InlineSpan>) -> bool {
    let mut refused = false;
    let mut retained = Vec::with_capacity(spans.len());
    for mut span in std::mem::take(spans) {
        match &mut span {
            InlineSpan::Link { url, spans, .. } if url.starts_with(":/") => {
                refused = true;
                let mut label = std::mem::take(spans);
                refused |= refuse_spans(&mut label);
                retained.extend(label);
            },
            InlineSpan::Presented { spans: inner, .. }
            | InlineSpan::Emphasis(inner)
            | InlineSpan::Strong(inner)
            | InlineSpan::Submit { spans: inner, .. } => {
                refused |= refuse_spans(inner);
                retained.push(span);
            },
            InlineSpan::Link { spans: inner, .. } => {
                refused |= refuse_spans(inner);
                retained.push(span);
            },
            _ => retained.push(span),
        }
    }
    *spans = retained;
    refused
}

impl SessionEngine<Scene> for MicronSessionEngine {
    fn engine_id(&self) -> &str {
        ENGINE_ID
    }

    fn spawn(
        &self,
        request: &SessionSpawnRequest,
    ) -> Result<Box<dyn DocumentSession<Scene>>, SessionError> {
        let body = match &request.body {
            Some(body) => body.clone(),
            None => ResourceFetcher::fetch(&self.local_fetcher, &request.address)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .ok_or_else(|| {
                    SessionError::SpawnFailed(format!("could not load {}", request.address))
                })?,
        };
        let mut input = EngineInput::new(&request.address, body);
        if let Some(content_type) = &request.content_type {
            input = input.with_content_type(content_type);
        }
        let mut document = nematic::MicronEngine::new()
            .render(&input)
            .map_err(|error| SessionError::SpawnFailed(error.to_string()))?;
        refuse_unresolved_aliases(&mut document);
        let document = SmolwebDocument::from_document_with_theme(document, SmolwebTheme::System);
        Ok(Box::new(SmolwebDocumentSession::new(
            document,
            request.viewport,
        )))
    }
}

/// Fetch a NomadNet page with the endpoint address supplied by the host.
///
/// `TURNSTONE_NOMADNET_TCP` is the local or remote Reticulum TCP interface.
/// The destination's public identity is learned from an authenticated path
/// response, so ordinary navigation never asks users to paste peer keys.
pub async fn fetch_page(address: &str) -> Result<crate::action::FetchedPage, String> {
    let address = parse_address(address)
        .ok_or_else(|| "NomadNet address must be destinationhex:/absolute/page/path".to_string())?;
    let tcp = std::env::var("TURNSTONE_NOMADNET_TCP")
        .map_err(|_| "NomadNet is not configured: set TURNSTONE_NOMADNET_TCP".to_string())?
        .parse::<SocketAddr>()
        .map_err(|error| format!("invalid TURNSTONE_NOMADNET_TCP: {error}"))?;
    let timeout = std::env::var("TURNSTONE_NOMADNET_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(30)
        .clamp(1, 120);
    let max_page_bytes = std::env::var("TURNSTONE_NOMADNET_MAX_PAGE_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(4 * 1024 * 1024)
        .clamp(1, 64 * 1024 * 1024);

    let destination = address.destination();
    let result = tokio::time::timeout(Duration::from_secs(timeout), async {
        // Endpoint owns its interface and its Drop closes it. Keeping the full
        // operation in this future makes cancellation and the host deadline
        // release that transport even during connect or discovery.
        let mut secret = [0_u8; IDENTITY_LEN];
        getrandom::getrandom(&mut secret)
            .map_err(|error| format!("could not create ephemeral Reticulum identity: {error}"))?;
        let endpoint = Endpoint::new(PrivateIdentity::from_secret_bytes(&secret));
        secret.fill(0);
        endpoint
            .attach_tcp_client(tcp)
            .await
            .map_err(|error| format!("could not connect NomadNet interface: {error}"))?;
        endpoint.request_path(destination);
        let peer = loop {
            if let Some(peer) = endpoint.resolve(destination) {
                break peer;
            }
            endpoint
                .next_announcement()
                .await
                .map_err(|error| error.to_string())?;
        };
        let config = retinue::nomadnet::FetchPageConfig {
            max_page_bytes,
            transfer: retinue::endpoint::ResourceTransferConfig {
                timeout: Duration::from_secs(timeout),
                ..retinue::endpoint::ResourceTransferConfig::default()
            },
        };
        retinue::nomadnet::fetch_page_with_config(
            &endpoint,
            destination,
            peer,
            address.path().as_bytes(),
            config,
        )
        .await
        .map_err(|error| format!("NomadNet page request failed: {error}"))
    })
    .await
    .map_err(|_| format!("NomadNet page request timed out after {timeout}s"))?;
    let bytes = result?;
    Ok(crate::action::FetchedPage {
        content_type: None,
        content_disposition: None,
        body: String::from_utf8_lossy(&bytes).into_owned(),
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use inker::session_engine::SessionEngine;

    #[test]
    fn native_address_keeps_destination_and_opaque_path() {
        let address = parse_address("7fc8950ec4e50695be76eddc8e539ee8:/page/turnstone.mu")
            .expect("ordinary NomadNet address parses");
        assert_eq!(
            address.destination().to_string(),
            "7fc8950ec4e50695be76eddc8e539ee8"
        );
        assert_eq!(address.path(), "/page/turnstone.mu");
        assert!(parse_address("nomadnet://7fc8950ec4e50695be76eddc8e539ee8/page").is_none());
        assert!(parse_address("7fc8950ec4e50695be76eddc8e539ee8:page").is_none());
    }

    #[test]
    fn native_projection_qualifies_same_node_page_links() {
        let base = "7fc8950ec4e50695be76eddc8e539ee8:/page/index.mu";
        let mut document = nematic::MicronEngine::new()
            .render(&EngineInput::new(
                base,
                ">Heading\n`[About`:/page/about.mu]\n",
            ))
            .expect("Micron projection renders");
        refuse_unresolved_aliases(&mut document);
        assert_eq!(
            document.outgoing_links(),
            vec!["7fc8950ec4e50695be76eddc8e539ee8:/page/about.mu".to_owned()]
        );
    }

    #[test]
    fn unresolved_aliases_are_rejected_in_every_retained_container() {
        fn alias(label: &str, target: &str) -> InlineSpan {
            InlineSpan::Link {
                url: target.to_owned(),
                title: None,
                spans: vec![InlineSpan::Text(label.to_owned())],
                predicate: None,
            }
        }

        let mut document = EngineDocument {
            address: "https://example.test/turnstone.mu".to_owned(),
            title: None,
            content_type: String::new(),
            lang: None,
            provenance: Default::default(),
            trust: Default::default(),
            diagnostics: Vec::new(),
            blocks: vec![
                Block::Presented {
                    presentation: Default::default(),
                    block: Box::new(Block::Paragraph {
                        spans: vec![InlineSpan::Presented {
                            presentation: Default::default(),
                            spans: vec![alias("styled", ":/page/styled.mu")],
                        }],
                    }),
                },
                Block::Table {
                    alignments: Vec::new(),
                    header: vec![
                        vec![alias("one", ":/page/one.mu")],
                        vec![alias("two", ":/other/two.mu")],
                    ],
                    rows: vec![vec![
                        vec![alias("three", ":/page/three.mu")],
                        vec![alias("four", ":/other/four.mu")],
                    ]],
                },
                Block::Quote {
                    blocks: vec![Block::Paragraph {
                        spans: vec![alias("quoted", ":/page/quoted.mu")],
                    }],
                },
                Block::List {
                    ordered: false,
                    items: vec![vec![Block::Paragraph {
                        spans: vec![alias("listed", ":/page/listed.mu")],
                    }]],
                },
            ],
        };

        refuse_unresolved_aliases(&mut document);
        assert!(document.outgoing_links().is_empty());
        assert!(
            document.to_text().contains("styled"),
            "refusing an unresolved target must preserve its styled label"
        );
        assert!(
            document
                .walk_inline_spans()
                .iter()
                .all(|span| { !matches!(span, InlineSpan::Link { .. }) })
        );
        assert!(document.diagnostics.iter().any(|diagnostic| {
            matches!(diagnostic, DocumentDiagnostic::UnsupportedConstruct(message) if message.contains("document has no NomadNet authority"))
        }));
    }

    #[test]
    fn micron_session_preserves_source_and_produces_scene() {
        let request = SessionSpawnRequest::new("file:///tmp/turnstone.mu")
            .with_body(">Heading\n---\nPlain captured prose\n# unsupported\n")
            .with_viewport(480, 320);
        let mut session = MicronSessionEngine::new()
            .spawn(&request)
            .expect("Micron session spawns from supplied bytes");
        let scene = session.frame(480, 320);
        assert!(
            scene
                .ops
                .iter()
                .any(|operation| matches!(operation, netrender::SceneOp::GlyphRun(_))),
            "captured Micron content reaches the retained scene"
        );
        assert_eq!(
            session.inspect().expect("content report").title.as_deref(),
            Some("Heading")
        );
        assert!(is_micron_address("file:///tmp/turnstone.mu"));
    }
}
