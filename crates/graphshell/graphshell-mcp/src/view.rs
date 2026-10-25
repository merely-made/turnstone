// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! What an agent reads: a projection snapshot as one JSON document.
//!
//! The scene's geometry is for a renderer and costs tokens for nothing here, so
//! the document keeps what a reader can use: each instance with its source
//! identity, the presentation offers bound to it (with the endpoint's own
//! label, role and advertised actions) and the cards and text the endpoint
//! disclosed, plus the relations between instances. Nothing is invented;
//! everything is the endpoint's answer to the same snapshot request any
//! Graphshell client makes, so what the grant withholds is absent.

use chirograph::{PresentationCodec, PresentationOffer, ProjectionSnapshot};
use sceno::InstanceId;
use serde_json::{Value, json};

/// The decoded body of an offer the endpoint disclosed, when it is words.
pub fn decode_content(offer: &PresentationOffer, bytes: &[u8]) -> Option<Value> {
    match offer.codec {
        PresentationCodec::PortableCardV1 => serde_json::from_slice(bytes).ok(),
        PresentationCodec::EditableTextV1 => Some(json!({"text": String::from_utf8_lossy(bytes)})),
        _ => None,
    }
}

/// Whether an offer's body is worth fetching for a reader.
pub fn is_readable(offer: &PresentationOffer) -> bool {
    matches!(
        offer.codec,
        PresentationCodec::PortableCardV1 | PresentationCodec::EditableTextV1
    )
}

fn offers(snapshot: &ProjectionSnapshot, index: usize) -> &[PresentationOffer] {
    snapshot
        .presentation
        .offers_for(InstanceId(index as u32))
        .unwrap_or(&[])
}

/// The snapshot as an agent reads it. `content` supplies a readable offer's
/// bytes through the carrier, or `None` when it is too large or unavailable.
pub fn digest(
    snapshot: &ProjectionSnapshot,
    content: &mut dyn FnMut(&PresentationOffer) -> Option<Vec<u8>>,
) -> Value {
    let tables = &snapshot.scene.tables;
    let items: Vec<Value> = tables
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, slot)| {
            let item = slot.as_ref()?;
            let source = tables
                .sources
                .get(item.source.0 as usize)
                .and_then(Option::as_ref);
            let offers: Vec<Value> = offers(snapshot, index)
                .iter()
                .map(|offer| {
                    let mut body = serde_json::to_value(offer).unwrap_or(Value::Null);
                    let decoded = is_readable(offer)
                        .then(|| content(offer))
                        .flatten()
                        .and_then(|bytes| decode_content(offer, &bytes));
                    if let Some(decoded) = decoded {
                        body["content"] = decoded;
                    }
                    body
                })
                .collect();
            Some(json!({
                "instance": index,
                "source": source,
                "visible": item.visible,
                "offers": offers,
            }))
        })
        .collect();
    let relations: Vec<Value> = tables
        .relations
        .iter()
        .flatten()
        .map(
            |relation| json!({"from": relation.from.0, "to": relation.to.0, "kind": relation.kind}),
        )
        .collect();
    json!({
        "session": snapshot.session.0,
        "epoch": snapshot.scene.epoch.0,
        "revision": snapshot.scene.revision.0,
        "items": items,
        "relations": relations,
    })
}

/// Every action the endpoint advertises in this snapshot, one row each, with
/// the instance it targets. Discoverability is not permission: the endpoint
/// still decides each invocation against the caller's grant.
pub fn actions(snapshot: &ProjectionSnapshot) -> Vec<Value> {
    let tables = &snapshot.scene.tables;
    let mut rows = Vec::new();
    for (index, slot) in tables.items.iter().enumerate() {
        let Some(item) = slot else { continue };
        let source = tables
            .sources
            .get(item.source.0 as usize)
            .and_then(Option::as_ref);
        for offer in offers(snapshot, index) {
            for action in &offer.semantics.actions {
                rows.push(json!({
                    "instance": index,
                    "source": source,
                    "intent": action.intent.0,
                    "label": action.label,
                    "explanation": action.explanation,
                    "payload_schema": action.payload_schema,
                    "input_form": action.input_form,
                    "effect": action.effect,
                }));
            }
        }
    }
    rows
}
