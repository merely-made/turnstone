// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::collections::BTreeSet;

use genet_static_dom::{StaticDocument, StaticNodeKind};
use mere::{ContextCache, dataset::View, formats::parse::parse_jsonld};
use oxrdf::{NamedOrBlankNode, Term};
use url::Url;

use super::{Claim, MAX_CLAIMS, MAX_HTML_BYTES, Page};
use crate::content::FetchedDocument;

const MAX_JSON_LD_BYTES: usize = 64 * 1024;

pub(super) fn document_base(document: &StaticDocument, address: &Url) -> Url {
    let mut pending = vec![document.document_node()];
    while let Some(id) = pending.pop() {
        let node = document.node(id);
        if let StaticNodeKind::Element { name, attrs, .. } = node.kind()
            && &*name.ns == "http://www.w3.org/1999/xhtml"
            && &*name.local == "base"
            && let Some(href) = attrs
                .iter()
                .find(|attr| attr.name.ns.is_empty() && &*attr.name.local == "href")
        {
            return address
                .join(&href.value)
                .unwrap_or_else(|_| address.clone());
        }
        pending.extend(node.children().iter().rev().copied());
    }
    address.clone()
}

pub(super) fn extract(document: &FetchedDocument, hash: &str) -> Page {
    let Some(address) = document
        .effective_url
        .as_deref()
        .and_then(|value| Url::parse(value).ok())
    else {
        return Page {
            partial: true,
            ..Page::default()
        };
    };
    let mut page = Page {
        address: address.to_string(),
        ..Page::default()
    };
    if document.body.len() > MAX_HTML_BYTES {
        page.partial = true;
        return page;
    }
    let dom = StaticDocument::parse(&document.body);
    let base = document_base(&dom, &address);
    page.label = fleece::extract_title(&dom);
    let blocks = fleece::extract_json_ld_blocks(&dom);
    let max_links = if blocks.is_empty() {
        MAX_CLAIMS
    } else {
        MAX_CLAIMS / 2
    };
    for (position, link) in fleece::extract_links(&dom).into_iter().enumerate() {
        if page.claims.len() == max_links {
            page.partial = true;
            break;
        }
        let Some(target) = web_target(&base, &link.href) else {
            continue;
        };
        let relation = link.rel.filter(|value| !value.trim().is_empty());
        let rel = relation
            .map(|value| format!(" · rel={value}"))
            .unwrap_or_default();
        page.claims.push(Claim {
            from: page.address.clone(),
            to: target.to_string(),
            predicate: "Links to".into(),
            source: page.address.clone(),
            evidence: format!(
                "{} · HTML link {}{rel} · {} · Capture {hash}",
                page.address,
                position + 1,
                excerpt(&link.text),
            ),
        });
    }
    for block in blocks {
        if page.claims.len() == MAX_CLAIMS {
            page.partial = true;
            break;
        }
        if block.dom_text.len() > MAX_JSON_LD_BYTES {
            page.partial = true;
            continue;
        }
        let Ok(dataset) = parse_jsonld(
            block.dom_text.as_bytes(),
            ContextCache::full(),
            Some(base.as_str()),
            Default::default(),
        ) else {
            page.partial = true;
            continue;
        };
        let evidence = format!(
            "{} · JSON-LD block at DOM position {} · Capture {hash}",
            page.address, block.document_order,
        );
        let mut subjects = BTreeSet::new();
        for quad in dataset.quads(View::Live) {
            let (NamedOrBlankNode::NamedNode(from), Term::NamedNode(to)) =
                (quad.subject, quad.object)
            else {
                continue;
            };
            if page.claims.len() == MAX_CLAIMS {
                page.partial = true;
                break;
            }
            if from.as_str() != page.address && subjects.insert(from.as_str().to_owned()) {
                page.claims.push(Claim {
                    from: page.address.clone(),
                    to: from.as_str().into(),
                    predicate: "Contains statements about".into(),
                    evidence: evidence.clone(),
                    source: page.address.clone(),
                });
            }
            if page.claims.len() == MAX_CLAIMS {
                page.partial = true;
                break;
            }
            let statement_evidence = if quad.graph_name.is_default_graph() {
                evidence.clone()
            } else {
                format!("{evidence} · RDF graph {}", quad.graph_name)
            };
            page.claims.push(Claim {
                from: from.as_str().into(),
                to: to.as_str().into(),
                predicate: quad.predicate.as_str().into(),
                evidence: statement_evidence,
                source: page.address.clone(),
            });
        }
    }
    page
}

pub(super) fn web_target(base: &Url, href: &str) -> Option<Url> {
    let target = base.join(href).ok()?;
    matches!(
        target.scheme(),
        "http" | "https" | "gemini" | "ipfs" | "ipns"
    )
    .then_some(target)
}

pub(super) fn excerpt(text: &str) -> String {
    let mut chars = text.chars();
    let mut label: String = chars.by_ref().take(120).collect();
    if chars.next().is_some() {
        label.push('…');
    }
    label
}
