//! The browse lane: address opening, fetch, metadata enrichment, favicon
//! discovery — and the fetch actor's adapter, converting the service's
//! concrete types into the app-owned [`Update`] messages so the vocabulary
//! stays port-agnostic. The folding itself is pure (testable; the app update
//! never blocks). Live content (the engine registry, document lifecycle,
//! verso-tile, content frames) is the separate `content` module born at
//! obviation rung 4: notes, gemini documents, local media, and HTML are all
//! content, while only some of it arrives by web fetching.
//!
//! Correlation-over-URLs (the architecture plan's recorded decision, landed
//! at this enrichment touch): effects carry the REQUESTING node's id, the
//! shell notes it in [`PendingFetches`] before commanding the actor, and the
//! adapter reattaches it on the way back — the fetch crate's wire types stay
//! untouched (meerkat shares them). Two nodes fetching the same URL are
//! interchangeable attributions (same URL, same payload), so the pending
//! table keys by URL and pops one requester per completion. What the id
//! buys is the STAMP side: enrichment lands on the exact requester via the
//! canvas's member-keyed setters, and a late result against a node that
//! navigated away drops explicitly instead of repainting the wrong page's
//! face (`get_node_by_url` first-match was the old, wrong answer).

use std::collections::HashMap;

use fetch::{FetchCommand, FetchUpdate};
use mere::canvas::Canvas;
use uuid::Uuid;

use crate::action::{Effect, FetchedPage, Update};

/// The shell-owned correlation table: which node asked for each in-flight
/// fetch. Port bookkeeping, not app truth (the app's record is the effect it
/// emitted); keyed by URL because that is all the actor echoes back.
#[derive(Debug, Default)]
pub struct PendingFetches {
    pages: HashMap<String, Vec<PendingPage>>,
    /// Keyed by owner page URL (the actor echoes `owner_url`, not the icon URL).
    favicons: HashMap<String, Vec<Uuid>>,
    submissions: HashMap<u64, Option<Uuid>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingPage {
    node: Uuid,
    owner_url: String,
    identity_used: bool,
    kind: PendingPageKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingPageKind {
    Page,
    Feed,
}

impl PendingFetches {
    pub fn note_page(&mut self, url: &str, node: Uuid, owner_url: &str, identity_used: bool) {
        self.pages
            .entry(url.to_string())
            .or_default()
            .push(PendingPage {
                node,
                owner_url: owner_url.to_string(),
                identity_used,
                kind: PendingPageKind::Page,
            });
    }

    pub fn note_feed(&mut self, url: &str, node: Uuid, identity_used: bool) {
        self.pages
            .entry(url.to_string())
            .or_default()
            .push(PendingPage {
                node,
                owner_url: url.to_string(),
                identity_used,
                kind: PendingPageKind::Feed,
            });
    }

    pub fn note_favicon(&mut self, owner_url: &str, node: Uuid) {
        self.favicons
            .entry(owner_url.to_string())
            .or_default()
            .push(node);
    }

    pub fn page_in_flight(&self, url: &str, node: Uuid, owner_url: &str) -> bool {
        self.pages.get(url).is_some_and(|requesters| {
            requesters
                .iter()
                .any(|request| request.node == node && request.owner_url == owner_url)
        })
    }

    /// Whether any page or favicon fetch is still outstanding. The automation
    /// lane's quiescence read (`wait`): a scenario must not assert against a
    /// graph whose fetches have not landed.
    pub fn any_in_flight(&self) -> bool {
        !self.pages.is_empty() || !self.favicons.is_empty() || !self.submissions.is_empty()
    }

    fn take_page(&mut self, url: &str) -> Option<PendingPage> {
        take_one(&mut self.pages, url)
    }

    fn take_favicon(&mut self, owner_url: &str) -> Option<Uuid> {
        take_one(&mut self.favicons, owner_url)
    }
}

fn take_one<T>(map: &mut HashMap<String, Vec<T>>, key: &str) -> Option<T> {
    let list = map.get_mut(key)?;
    let node = list.pop();
    if list.is_empty() {
        map.remove(key);
    }
    node
}

/// Convert one fetch-actor answer into the app vocabulary, reattaching the
/// requesting node from the pending table. `None` for updates the app has no
/// lane for (subresources), or answers nothing asked for (an unmatched
/// completion is logged, not guessed at).
pub fn update_from_fetch(update: FetchUpdate, pending: &mut PendingFetches) -> Option<Update> {
    match update {
        FetchUpdate::Page(outcome) => {
            let Some(request) = pending.take_page(&outcome.url) else {
                // The actor URL may carry a status-11 answer in its query.
                // A correlation miss is diagnosable without copying that
                // transient request address into retained logs.
                tracing::warn!("page completion without a pending requester; dropped");
                return None;
            };
            match outcome.result {
                Ok(fetched) => {
                    let fetched = FetchedPage {
                        content_type: fetched.content_type,
                        body: fetched.body,
                    };
                    Some(match request.kind {
                        PendingPageKind::Page => Update::PageFetched {
                            node: request.node,
                            url: request.owner_url,
                            result: Ok(fetched),
                        },
                        PendingPageKind::Feed => Update::FeedFetched {
                            node: request.node,
                            url: request.owner_url,
                            result: Ok(fetched),
                        },
                    })
                }
                Err(fetch::FetchFailure::InputRequired {
                    url: input_url,
                    prompt,
                    sensitive,
                }) => Some(match request.kind {
                    PendingPageKind::Page => Update::SmolwebInputRequested {
                        node: request.node,
                        url: request.owner_url,
                        input_url,
                        prompt,
                        sensitive,
                    },
                    PendingPageKind::Feed => Update::FeedFetched {
                        node: request.node,
                        url: request.owner_url,
                        result: Err(format!(
                            "feed refresh requires {} input: {prompt}",
                            if sensitive {
                                "sensitive"
                            } else {
                                "interactive"
                            }
                        )),
                    },
                }),
                Err(fetch::FetchFailure::ClientCertificateRequired {
                    url: identity_url,
                    prompt,
                    code,
                }) => {
                    if request.kind == PendingPageKind::Feed {
                        let status = code.map(|code| format!(" ({code})")).unwrap_or_default();
                        Some(Update::FeedFetched {
                            node: request.node,
                            url: request.owner_url,
                            result: Err(format!(
                                "feed refresh requires a client certificate{status}: {prompt}"
                            )),
                        })
                    } else if request.identity_used {
                        let status = code.map(|code| format!(" ({code})")).unwrap_or_default();
                        Some(Update::PageFetched {
                            node: request.node,
                            url: request.owner_url,
                            result: Err(format!("client certificate rejected{status}: {prompt}")),
                        })
                    } else {
                        Some(Update::GeminiIdentityRequested {
                            node: request.node,
                            url: request.owner_url,
                            identity_url,
                            prompt,
                        })
                    }
                }
                Err(fetch::FetchFailure::CertificateChanged {
                    url: fetch_url,
                    target,
                    pinned,
                    seen,
                }) => Some(match request.kind {
                    PendingPageKind::Page => Update::GeminiCertificateChanged {
                        node: request.node,
                        url: request.owner_url,
                        fetch_url,
                        target,
                        pinned,
                        seen,
                    },
                    PendingPageKind::Feed => Update::FeedFetched {
                        node: request.node,
                        url: request.owner_url,
                        result: Err(format!(
                            "Gemini certificate changed for {target}; open the source to review it"
                        )),
                    },
                }),
                Err(fetch::FetchFailure::Failed(error)) => Some(match request.kind {
                    PendingPageKind::Page => Update::PageFetched {
                        node: request.node,
                        url: request.owner_url,
                        result: Err(error),
                    },
                    PendingPageKind::Feed => Update::FeedFetched {
                        node: request.node,
                        url: request.owner_url,
                        result: Err(error),
                    },
                }),
            }
        }
        FetchUpdate::Favicon { owner_url, bytes } => {
            let Some(node) = pending.take_favicon(&owner_url) else {
                tracing::warn!(url = %owner_url, "favicon completion without a pending requester; dropped");
                return None;
            };
            Some(Update::FaviconFetched {
                node,
                owner_url,
                bytes,
            })
        }
        FetchUpdate::Subresource(_) => None,
        FetchUpdate::Submission(outcome) => {
            let Some(source) = pending.submissions.remove(&outcome.request) else {
                tracing::warn!(
                    request = outcome.request,
                    "submission completion without a pending requester; dropped"
                );
                return None;
            };
            let result = outcome
                .result
                .map(|answer| match answer {
                    fetch::SubmissionAnswer::Success(fetched) => {
                        crate::action::SmolwebSubmissionReceipt::Success(FetchedPage {
                            content_type: fetched.content_type,
                            body: fetched.body,
                        })
                    }
                    fetch::SubmissionAnswer::Redirect(target) => {
                        crate::action::SmolwebSubmissionReceipt::Redirect(target)
                    }
                })
                .map_err(|error| error.to_string());
            Some(Update::SmolwebSubmitted {
                request: outcome.request,
                source,
                target: outcome.url,
                result,
            })
        }
    }
}

/// Translate an effect into the fetch actor's command, if it is fetch-shaped.
/// The shell notes the requester in [`PendingFetches`] and commands the
/// actor; the mapping stays beside the enrichment it feeds.
pub fn fetch_command_for(effect: &Effect, pending: &mut PendingFetches) -> Option<FetchCommand> {
    match effect {
        Effect::FetchPage {
            node,
            url,
            owner_url,
            identity,
        } => {
            pending.note_page(url, *node, owner_url, identity.is_some());
            Some(FetchCommand::Page {
                url: url.clone(),
                identity: identity.clone(),
            })
        }
        Effect::FetchFeed {
            node,
            url,
            identity,
        } => {
            pending.note_feed(url, *node, identity.is_some());
            Some(FetchCommand::Page {
                url: url.clone(),
                identity: identity.clone(),
            })
        }
        Effect::FetchFavicon {
            node,
            owner_url,
            url,
        } => {
            pending.note_favicon(owner_url, *node);
            Some(FetchCommand::Favicon {
                owner_url: owner_url.clone(),
                url: url.clone(),
            })
        }
        Effect::SubmitSmolweb {
            request,
            source,
            target,
            protocol,
            body,
            mime,
            token,
            identity,
        } => {
            pending.submissions.insert(*request, *source);
            let submission = match protocol {
                crate::ui::SmolwebSubmissionProtocol::Titan => fetch::SmolwebSubmission::Titan {
                    url: target.clone(),
                    body: body.clone(),
                    mime: mime.clone(),
                    token: token.as_ref().map(|token| token.as_str().to_string()),
                    identity: identity.clone(),
                },
                crate::ui::SmolwebSubmissionProtocol::Spartan => {
                    fetch::SmolwebSubmission::Spartan {
                        url: target.clone(),
                        body: body.clone(),
                    }
                }
            };
            Some(FetchCommand::Submit {
                request: *request,
                submission,
            })
        }
        _ => None,
    }
}

/// Whether `node` still lives at `url` — the staleness gate. Enrichment
/// belongs to the page that was fetched; a node that has navigated away (or
/// died) since the request drops the late result explicitly.
pub(crate) fn still_current(canvas: &Canvas, node: Uuid, url: &str) -> bool {
    canvas
        .graph()
        .get_node_by_id(node)
        .is_some_and(|(_, n)| n.url() == url)
}

/// Fold one completed page fetch into the graph: stamp the response's
/// Content-Type as the requesting node's MIME hint, and for HTML extract the
/// page `<title>` (render-free static parse) so the canvas caption flips from
/// the host fallback to the real title, then chase the page's favicon so the
/// node face wears a real icon. All stamps target the requester by member id.
pub fn apply_page(
    canvas: &mut Canvas,
    node: Uuid,
    url: String,
    result: Result<FetchedPage, String>,
) -> Vec<Effect> {
    if !still_current(canvas, node, &url) {
        tracing::info!(%node, %url, "page result for a superseded node; dropped");
        return Vec::new();
    }
    let mut effects = Vec::new();
    match result {
        Ok(fetched) => {
            let media = fetched
                .content_type
                .as_deref()
                .and_then(|ct| ct.split(';').next())
                .map(|m| m.trim().to_ascii_lowercase());
            tracing::info!(%url, content_type = ?media, bytes = fetched.body.len(), "page fetched");
            canvas.set_node_mime_hint_for(node, media.clone());
            if media.as_deref() == Some("text/html") {
                let doc = genet_static_dom::StaticDocument::parse(&fetched.body);
                if let Some(title) = genet_extract::extract(&doc).title {
                    if canvas.set_node_title_for(node, title.clone()) {
                        tracing::info!(%url, %title, "node title enriched from the page");
                    }
                }
                // Best-effort: chase the page's favicon; the bytes route back
                // as a FaviconFetched update correlated to this node.
                if let Some(icon_url) = favicon_url_for(&url, &doc) {
                    effects.push(Effect::FetchFavicon {
                        node,
                        owner_url: url.clone(),
                        url: icon_url,
                    });
                }
            }
        }
        Err(err) => {
            tracing::warn!(%url, %err, "page fetch failed");
        }
    }
    effects.push(Effect::SaveSession);
    effects.push(Effect::Redraw);
    effects
}

/// A page's favicon arrived: decode it to RGBA for this frame, encode that
/// canonical pixel result as PNG for durability, and stamp its reference on
/// the requesting node if it still lives at the page the icon belongs to.
pub fn apply_favicon(
    canvas: &mut Canvas,
    node: Uuid,
    owner_url: &str,
    bytes: &[u8],
) -> Vec<Effect> {
    if !still_current(canvas, node, owner_url) {
        tracing::info!(%node, url = %owner_url, "favicon for a superseded node; dropped");
        return Vec::new();
    }
    let Some(decoded) = genet_layout::decode_image_bytes(bytes) else {
        return Vec::new();
    };
    let Some(png) = crate::session::encode_rgba_png(&decoded.rgba, decoded.width, decoded.height)
    else {
        return Vec::new();
    };
    // The graph carries a content-addressed reference; the pixels go to the
    // canvas cache (so the icon paints this frame) and canonical PNG bytes to
    // the session's blob directory (so every image role has one durable
    // format). The digest is over exactly what is stored.
    let digest = *eidetic::Hash::of(&png).as_bytes();
    let image = mere::kernel::types::ImageRef::new(digest, decoded.width, decoded.height);
    let changed = canvas.set_node_favicon_for(node, image);
    canvas.register_resolved_image(digest, decoded.rgba, decoded.width, decoded.height);
    let mut effects = vec![Effect::StoreImage {
        hex: image.hex(),
        bytes: png,
    }];
    if changed {
        tracing::info!(url = %owner_url, "node favicon enriched from the page");
        effects.push(Effect::SaveSession);
    }
    effects.push(Effect::Redraw);
    effects
}

/// The favicon URL for a fetched page: the document's declared
/// `<link rel=icon>` href resolved against the page URL, else the well-known
/// `/favicon.ico` for web pages. `None` when neither applies.
fn favicon_url_for(page_url: &str, doc: &genet_static_dom::StaticDocument) -> Option<String> {
    let base = url::Url::parse(page_url).ok()?;
    if let Some(href) = genet_layout::linked_icon_href(doc) {
        if let Ok(resolved) = base.join(&href) {
            return Some(resolved.to_string());
        }
    }
    if matches!(base.scheme(), "http" | "https") {
        if let Ok(fallback) = base.join("/favicon.ico") {
            return Some(fallback.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two nodes share one URL; the stamp lands on the CORRELATED node, not
    /// whichever `get_node_by_url` answers first (the bug this lane fixes).
    #[test]
    fn enrichment_stamps_the_requesting_node_among_url_twins() {
        let mut canvas = Canvas::new();
        let first_key = canvas.visit("https://elsewhere.example");
        let first = canvas.graph().get_node(first_key).unwrap().id;
        let target_key = canvas.visit("https://twin.example");
        let target = canvas.graph().get_node(target_key).unwrap().id;
        // Navigate the first node onto the same URL: two nodes, one address.
        assert!(canvas.navigate_member(first, "https://twin.example"));

        let effects = apply_page(
            &mut canvas,
            target,
            "https://twin.example".to_string(),
            Ok(FetchedPage {
                content_type: Some("text/html".to_string()),
                body: "<html><head><title>Twin B</title></head></html>".to_string(),
            }),
        );
        assert!(!effects.is_empty());
        let titles: Vec<_> = canvas
            .graph()
            .nodes()
            .map(|(_, n)| (n.id, n.title.clone()))
            .collect();
        for (id, title) in titles {
            if id == target {
                assert_eq!(title, "Twin B", "the requester is enriched");
            } else {
                assert_ne!(title, "Twin B", "its URL twin is untouched");
            }
        }
    }

    /// A late result against a node that navigated away drops explicitly.
    #[test]
    fn superseded_node_drops_the_late_result() {
        let mut canvas = Canvas::new();
        let key = canvas.visit("https://before.example");
        let node = canvas.graph().get_node(key).unwrap().id;
        canvas.navigate_member(node, "https://after.example");

        let effects = apply_page(
            &mut canvas,
            node,
            "https://before.example".to_string(),
            Ok(FetchedPage {
                content_type: Some("text/html".to_string()),
                body: "<html><head><title>Stale</title></head></html>".to_string(),
            }),
        );
        assert!(effects.is_empty(), "no stamps, no save, no redraw");
        let (_, n) = canvas.graph().get_node_by_id(node).unwrap();
        assert_ne!(n.title, "Stale", "the stale title never landed");
    }

    /// The pending table pops one requester per completion and never guesses
    /// at an unmatched answer.
    #[test]
    fn pending_table_correlates_and_refuses_to_guess() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let mut pending = PendingFetches::default();
        pending.note_page("https://x.example", a, "https://owner-a.example", false);
        pending.note_page("https://x.example", b, "https://owner-b.example", false);
        assert!(pending.take_page("https://x.example").is_some());
        assert!(pending.take_page("https://x.example").is_some());
        assert!(pending.take_page("https://x.example").is_none());

        let unmatched = update_from_fetch(
            FetchUpdate::Favicon {
                owner_url: "https://nobody.example".to_string(),
                bytes: vec![1, 2, 3],
            },
            &mut pending,
        );
        assert!(
            unmatched.is_none(),
            "an unmatched completion is dropped, not guessed"
        );
    }

    #[test]
    fn input_response_keeps_actor_target_separate_from_graph_owner() {
        let node = Uuid::new_v4();
        let owner = "gemini://capsule.test/login";
        let request = "gemini://capsule.test/login?secret";
        let mut pending = PendingFetches::default();
        pending.note_page(request, node, owner, false);

        let update = update_from_fetch(
            FetchUpdate::Page(fetch::FetchOutcome {
                url: request.into(),
                result: Err(fetch::FetchFailure::InputRequired {
                    url: request.into(),
                    prompt: "Again".into(),
                    sensitive: true,
                }),
            }),
            &mut pending,
        )
        .unwrap();
        assert!(matches!(
            update,
            Update::SmolwebInputRequested {
                node: actual,
                url,
                input_url,
                prompt,
                sensitive: true,
            } if actual == node
                && url == owner
                && input_url == request
                && prompt == "Again"
        ));
    }

    #[test]
    fn certificate_request_prompts_once_then_reports_rejection() {
        let node = Uuid::new_v4();
        let owner = "gemini://capsule.test/account";
        let target = "gemini://capsule.test/private";
        let failure = || fetch::FetchFailure::ClientCertificateRequired {
            url: target.into(),
            prompt: "Identity required".into(),
            code: Some(60),
        };

        let mut anonymous = PendingFetches::default();
        anonymous.note_page(target, node, owner, false);
        let update = update_from_fetch(
            FetchUpdate::Page(fetch::FetchOutcome {
                url: target.into(),
                result: Err(failure()),
            }),
            &mut anonymous,
        )
        .unwrap();
        assert!(matches!(
            update,
            Update::GeminiIdentityRequested {
                node: actual,
                url,
                identity_url,
                prompt,
            } if actual == node
                && url == owner
                && identity_url == target
                && prompt == "Identity required"
        ));

        let mut identified = PendingFetches::default();
        identified.note_page(target, node, owner, true);
        let update = update_from_fetch(
            FetchUpdate::Page(fetch::FetchOutcome {
                url: target.into(),
                result: Err(failure()),
            }),
            &mut identified,
        )
        .unwrap();
        assert!(matches!(
            update,
            Update::PageFetched { result: Err(error), .. }
                if error == "client certificate rejected (60): Identity required"
        ));
    }

    #[test]
    fn changed_certificate_preserves_the_fetch_target_and_both_fingerprints() {
        let node = Uuid::new_v4();
        let owner = "gemini://capsule.test/start";
        let request = "gemini://capsule.test:1966/private";
        let mut pending = PendingFetches::default();
        pending.note_page(request, node, owner, false);

        let update = update_from_fetch(
            FetchUpdate::Page(fetch::FetchOutcome {
                url: request.into(),
                result: Err(fetch::FetchFailure::CertificateChanged {
                    url: request.into(),
                    target: "capsule.test:1966".into(),
                    pinned: "11".repeat(32),
                    seen: "22".repeat(32),
                }),
            }),
            &mut pending,
        )
        .unwrap();

        assert!(matches!(
            update,
            Update::GeminiCertificateChanged {
                node: actual,
                url,
                fetch_url,
                target,
                pinned,
                seen,
            } if actual == node
                && url == owner
                && fetch_url == request
                && target == "capsule.test:1966"
                && pinned == "11".repeat(32)
                && seen == "22".repeat(32)
        ));
    }

    #[test]
    fn favicon_write_stores_png_bytes_under_their_digest() {
        let mut canvas = Canvas::new();
        let url = "https://icon.example/";
        let key = canvas.visit(url);
        let node = canvas.graph().get_node(key).unwrap().id;
        let fetched = crate::session::encode_rgba_png(&[255, 0, 0, 255], 1, 1).unwrap();

        let effects = apply_favicon(&mut canvas, node, url, &fetched);
        let (hex, stored) = effects
            .iter()
            .find_map(|effect| match effect {
                Effect::StoreImage { hex, bytes } => Some((hex, bytes)),
                _ => None,
            })
            .expect("the favicon is durably stored");
        assert_eq!(&stored[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(hex, &eidetic::Hash::of(stored).to_hex());
        assert_eq!(
            canvas
                .graph()
                .get_node(key)
                .unwrap()
                .favicon()
                .unwrap()
                .hex(),
            *hex
        );
    }
}
