// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The dispatching engine for parser mods (mod projects MP7, D38 to D43).
//!
//! One engine, registered at startup, serves every installed parser: at spawn
//! it finds the project that takes the page's media type, hands it the fetched
//! bytes, and renders the `EngineDocument` it returns as any other smolweb
//! document. Installing a parser takes effect without a restart, because the
//! lookup is at spawn.

use std::sync::Arc;

use inker::{
    Block, DocumentNavigation, DocumentProvenance, DocumentSession, DocumentTrustState, EngineDocument,
    InlineSpan, SessionEngine, SessionError, SessionSpawnRequest,
};
use mere_document_lanes::{SmolwebDocument, SmolwebDocumentSession, SmolwebTheme};
use netrender::Scene;

use crate::mod_projects::{ModProjectHost, ParseRefusal};

/// The one engine id every mod-parsed page routes to.
pub const ENGINE_ID: &str = "turnstone.mod-parser";

/// Media types the built-in engines take whatever the content-type rules say.
const HTML_TYPES: [&str; 2] = ["text/html", "application/xhtml+xml"];

/// Whether a built-in engine already claims `media_type` (D39): a content-type
/// rule of the app's policy, or HTML. A parser mod takes such a type only when
/// the user turned it on for that type.
pub fn builtin_claims(media_type: &str) -> bool {
    let Some(media) = mod_host::normalize_media_type(media_type) else {
        return false;
    };
    HTML_TYPES.contains(&media.as_str())
        || crate::shell::routing::route_policy()
            .rules
            .iter()
            .any(|rule| rule.matches_content_type(&media))
}

/// The installed parser to route a page of `content_type` to, if one takes it.
pub fn route(mods: &ModProjectHost, content_type: Option<&str>) -> Option<String> {
    let media = mod_host::normalize_media_type(content_type?)?;
    mods.route_for(&media, builtin_claims(&media))
}

pub(crate) struct ModParserEngine {
    mods: Option<Arc<ModProjectHost>>,
}

impl ModParserEngine {
    pub(crate) fn new(mods: Option<Arc<ModProjectHost>>) -> Self {
        Self { mods }
    }
}

impl SessionEngine<Scene> for ModParserEngine {
    fn engine_id(&self) -> &str {
        ENGINE_ID
    }

    fn spawn(
        &self,
        request: &SessionSpawnRequest,
    ) -> Result<Box<dyn DocumentSession<Scene>>, SessionError> {
        let fail = |why: String| SessionError::SpawnFailed(why);
        let mods = self
            .mods
            .as_ref()
            .ok_or_else(|| fail("mod projects are not available".into()))?;
        let media = request
            .content_type
            .as_deref()
            .and_then(mod_host::normalize_media_type)
            .ok_or_else(|| fail("a mod parser needs the page's media type".into()))?;
        let id = mods
            .route_for(&media, builtin_claims(&media))
            .ok_or_else(|| fail(format!("no installed parser mod takes {media}")))?;
        let bytes = request
            .body_bytes()
            .ok_or_else(|| fail("a mod parser requires a host-fetched body".into()))?;
        let page = match mods.parse_page(&id, &media, bytes, &request.address) {
            Ok(page) => page,
            // A refusal or a trap is a page that says which mod and why (D41).
            Err(refusal) => {
                let verb = if refusal.why.starts_with("trapped") { "trapped in" } else { "refused by" };
                mods.note_parsed(
                    &request.address,
                    format!("{verb} mod {}", refusal.display_name),
                    format!("{refusal} ({media})"),
                );
                let document = SmolwebDocument::from_document_with_theme(
                    error_document(&refusal, &request.address, &media),
                    SmolwebTheme::default(),
                );
                return Ok(Box::new(SmolwebDocumentSession::new(document, request.viewport)));
            },
        };
        let mut document = page.document;
        restrict_images(&mut document.blocks, &request.address, page.foreign_images);
        let n = page.diagnostics.len();
        mods.note_parsed(
            &request.address,
            format!("parsed by mod {}", page.display_name),
            format!(
                "parsed by mod {} ({}) as {media}, {n} diagnostic{}",
                page.display_name,
                page.mod_id,
                if n == 1 { "" } else { "s" }
            ),
        );
        let document = SmolwebDocument::from_document_with_theme(document, SmolwebTheme::default());
        Ok(Box::new(SmolwebDocumentSession::new(document, request.viewport)))
    }
}

/// The page a refusal or a trap leaves behind: it names the mod, says why, and
/// gives the address and media type the mod was handed.
fn error_document(refusal: &ParseRefusal, address: &str, media: &str) -> EngineDocument {
    let text = |t: String| vec![InlineSpan::Text(t)];
    EngineDocument {
        address: address.to_string(),
        title: Some(format!("Parser mod {} could not read this page", refusal.display_name)),
        content_type: "text/plain".into(),
        lang: None,
        provenance: DocumentProvenance::for_engine(ENGINE_ID, address),
        trust: DocumentTrustState::Unknown,
        diagnostics: Vec::new(),
        navigation: DocumentNavigation::default(),
        blocks: vec![
            Block::Heading {
                level: 1,
                spans: text(format!("Parser mod {} could not read this page", refusal.display_name)),
            },
            Block::Paragraph { spans: text(refusal.to_string()) },
            Block::MetadataRow { label: "Mod".into(), value: refusal.mod_id.clone() },
            Block::MetadataRow { label: "Media type".into(), value: media.to_string() },
            Block::MetadataRow { label: "Page".into(), value: address.to_string() },
        ],
    }
}

/// D43: an image on the page's own origin loads under the usual caps; one on
/// another origin loads only when the mod holds the network grant, and
/// otherwise reads as its alt text. A URL that does not resolve is foreign.
pub(crate) fn restrict_images(blocks: &mut [Block], page: &str, foreign_allowed: bool) {
    let base = url::Url::parse(page).ok();
    for block in blocks {
        restrict_block(block, base.as_ref(), foreign_allowed);
    }
}

fn same_origin(base: Option<&url::Url>, image: &str) -> bool {
    let Some(base) = base else { return false };
    base.join(image)
        .is_ok_and(|resolved| resolved.origin() == base.origin() && resolved.origin().is_tuple())
}

fn restrict_block(block: &mut Block, base: Option<&url::Url>, foreign_allowed: bool) {
    match block {
        Block::Image { url, alt } => {
            if !foreign_allowed && !same_origin(base, url) {
                let text = if alt.trim().is_empty() { "image".to_string() } else { alt.clone() };
                *block = Block::Paragraph { spans: vec![InlineSpan::Text(text)] };
            } else if let Some(base) = base {
                // The resolved address, so the fetch needs no further base.
                if let Ok(resolved) = base.join(url) {
                    *url = resolved.to_string();
                }
            }
        },
        Block::Presented { block, .. } => restrict_block(block, base, foreign_allowed),
        Block::Quote { blocks } => {
            for b in blocks {
                restrict_block(b, base, foreign_allowed);
            }
        },
        Block::List { items, .. } => {
            for item in items {
                for b in item {
                    restrict_block(b, base, foreign_allowed);
                }
            }
        },
        _ => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inker::DocumentSession;

    fn image(url: &str, alt: &str) -> Block {
        Block::Image { url: url.into(), alt: alt.into() }
    }

    fn text(block: &Block) -> String {
        match block {
            Block::Paragraph { spans } => spans
                .iter()
                .map(|s| match s {
                    InlineSpan::Text(t) => t.clone(),
                    _ => String::new(),
                })
                .collect(),
            other => other.kind_name().into(),
        }
    }

    #[test]
    fn same_origin_images_stay_resolved_and_other_origins_become_alt_text() {
        let page = "http://127.0.0.1:43118/notes/a.notes";
        let mut blocks = vec![
            image("pic.png", "near"),
            image("/up/pic.png", "rooted"),
            image("http://127.0.0.1:43118/abs.png", "absolute"),
            image("http://127.0.0.1:43119/other-port.png", "other port"),
            image("https://example.test/x.png", "far"),
            image("https://example.test/y.png", ""),
            Block::Quote { blocks: vec![image("https://example.test/q.png", "nested")] },
            Block::List { ordered: false, items: vec![vec![image("q.png", "listed")]] },
        ];
        restrict_images(&mut blocks, page, false);
        assert!(matches!(&blocks[0], Block::Image { url, .. } if url == "http://127.0.0.1:43118/notes/pic.png"));
        assert!(matches!(&blocks[1], Block::Image { url, .. } if url == "http://127.0.0.1:43118/up/pic.png"));
        assert!(matches!(&blocks[2], Block::Image { .. }));
        assert_eq!(text(&blocks[3]), "other port");
        assert_eq!(text(&blocks[4]), "far");
        assert_eq!(text(&blocks[5]), "image");
        assert!(matches!(&blocks[6], Block::Quote { blocks } if text(&blocks[0]) == "nested"));
        assert!(matches!(&blocks[7], Block::List { items, .. } if matches!(items[0][0], Block::Image { .. })));
    }

    #[test]
    fn the_network_grant_lets_other_origins_load() {
        let mut blocks = vec![image("https://example.test/x.png", "far")];
        restrict_images(&mut blocks, "http://127.0.0.1/a", true);
        assert!(matches!(&blocks[0], Block::Image { url, .. } if url == "https://example.test/x.png"));
    }

    #[test]
    fn a_page_without_a_usable_address_keeps_no_foreign_image() {
        let mut blocks = vec![image("pic.png", "unknown base")];
        restrict_images(&mut blocks, "not a url", false);
        assert_eq!(text(&blocks[0]), "unknown base");
    }

    #[test]
    fn built_in_claims_are_content_rules_and_html() {
        assert!(builtin_claims("text/html; charset=utf-8"));
        assert!(builtin_claims("text/markdown"));
        assert!(builtin_claims("text/gemini"));
        assert!(!builtin_claims("application/x-fixture-notes"));
        assert!(!builtin_claims("not a type"));
    }

    fn engine_with_parser() -> (ModParserEngine, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let mods = Arc::new(ModProjectHost::open(dir.path()));
        let from = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mod-projects/parser-notes");
        let id = mods.create(Some(&from)).unwrap();
        mods.check(&id).unwrap();
        (ModParserEngine::new(Some(mods)), dir)
    }

    fn request(content_type: &str, bytes: &[u8]) -> SessionSpawnRequest {
        SessionSpawnRequest::new("http://127.0.0.1:43118/page.notes")
            .with_body(String::from_utf8_lossy(bytes))
            .with_body_bytes(bytes.to_vec())
            .with_content_type(content_type)
            .with_viewport(400, 300)
    }

    fn refusal(engine: &ModParserEngine, request: &SessionSpawnRequest) -> String {
        match engine.spawn(request) {
            Ok(_) => String::new(),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn the_engine_parses_a_declared_type_names_the_mod_and_reports_the_title() {
        let (engine, _dir) = engine_with_parser();
        let session = engine
            .spawn(&request("application/x-fixture-notes; charset=utf-8", b"# Field notes
body
"))
            .ok()
            .expect("a session");
        assert_eq!(session.inspect().unwrap().title.as_deref(), Some("Field notes"));
        let mods = engine.mods.as_ref().unwrap();
        let url = "http://127.0.0.1:43118/page.notes";
        assert_eq!(mods.parse_note(url).as_deref(), Some("parsed by mod Notes parser"));
        let detail = mods.parse_detail(url).unwrap();
        assert!(detail.contains("notes-parser") && detail.contains("application/x-fixture-notes"), "{detail}");
    }

    #[test]
    fn a_refusal_or_a_trap_is_a_page_that_names_the_mod_and_why() {
        let (engine, _dir) = engine_with_parser();
        let mods = engine.mods.as_ref().unwrap();
        let url = "http://127.0.0.1:43118/page.notes";
        let session = engine.spawn(&request("application/x-fixture-trap", b"x")).ok().expect("an error page");
        assert_eq!(
            session.inspect().unwrap().title.as_deref(),
            Some("Parser mod Notes parser could not read this page")
        );
        assert_eq!(mods.parse_note(url).as_deref(), Some("trapped in mod Notes parser"));
        let detail = mods.parse_detail(url).unwrap();
        assert!(detail.contains("notes-parser") && detail.contains("trapped and was detached"), "{detail}");
        engine.spawn(&request("application/x-fixture-refuse", b"x")).ok().expect("an error page");
        assert_eq!(mods.parse_note(url).as_deref(), Some("refused by mod Notes parser"));
        assert!(mods.parse_detail(url).unwrap().contains("fixture refuses this input"));
    }

    #[test]
    fn the_engine_refuses_plainly_when_no_parser_takes_the_type_or_there_is_no_body() {
        let (engine, _dir) = engine_with_parser();
        let none = refusal(&engine, &request("application/x-other", b"x"));
        assert!(none.contains("no installed parser mod takes application/x-other"), "{none}");
        let no_bytes = SessionSpawnRequest::new("http://a/").with_content_type(NOTES);
        assert!(refusal(&engine, &no_bytes).contains("host-fetched body"));
        // Installing is at spawn: an engine with no host refuses plainly.
        assert!(refusal(&ModParserEngine::new(None), &request(NOTES, b"x")).contains("not available"));
    }

    const NOTES: &str = "application/x-fixture-notes";
}
