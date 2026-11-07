// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! The browser side of loading a guest block adapter: review, grant, install, calls.
//!
//! Mirrors the applet bridge (`applet.rs`). The supplying host (JavaScript) fetches a signed pack
//! and its component; this module verifies them (`CapsuleMount::review`), grants the one
//! `power:block-adapter` only when the host reports the person approved, and registers the
//! adapter the Worker described. The Worker runs the component; this module never does. A
//! [`Mailbox`] in the page's shared state carries the traffic in both directions: requests the
//! pane's `WorkerAdapter` wants answered go out, replies come back, and the page's frame hook
//! moves them between the mailbox and the pane.
//!
//! Review is the host's job to show: `block_adapter_review` returns what the person should read
//! (name, author key, what the pack asks for, component address and size) and nothing runs
//! until `block_adapter_approve`.

use super::*;
use graphshell::block_editing::adapter::BlockAdapter;
use graphshell::block_editing::worker::{Transport, WorkerAdapter};
use graphshell::capsule_applet::{
    BLOCK_ADAPTER, CapsuleDisclosure, CapsuleMount, MAX_DISCLOSURE_BYTES,
};
use serde_json::{Value, json};

#[derive(Default)]
pub(super) struct Mailbox {
    /// The pack being reviewed, then approved.
    mount: Option<CapsuleMount>,
    receipt: Option<Value>,
    /// Claims the Worker described, waiting for the frame hook to register them.
    installs: Vec<Value>,
    removals: Vec<String>,
    /// `(class, id, reply)` from the Worker, waiting for the frame hook.
    replies: Vec<(String, u64, String)>,
    /// `(class, id, request)` the pane wants answered, waiting for the JS pump.
    requests: Vec<(String, u64, String)>,
    /// Set by the pane's "Review block adapters"; the JS host reads and clears it.
    review_requested: bool,
    /// Classes registered, for the receipt.
    installed: Vec<String>,
    /// Open or close the block pane from outside the page's own controls.
    open_request: Option<bool>,
}

pub(super) type Handle = Rc<RefCell<Mailbox>>;

struct MailboxTransport {
    class: String,
    mailbox: Handle,
}
impl Transport for MailboxTransport {
    fn send(&self, id: u64, request: &str) {
        self.mailbox
            .borrow_mut()
            .requests
            .push((self.class.clone(), id, request.to_owned()));
    }
}

fn with_mailbox<R>(f: impl FnOnce(&mut Mailbox) -> Result<R, String>) -> Result<R, JsValue> {
    TREE.with(|tree| {
        let tree = tree.borrow();
        let tree = tree.as_ref().ok_or("Mount Graphshell's tree first")?;
        let result = f(&mut tree.shared.block_adapters.borrow_mut());
        tree.window.request_redraw();
        result
    })
    .map_err(|e: String| JsValue::from_str(&e))
}

/// Verify a pack and its component and keep the mount for approval. Nothing runs and nothing
/// is granted. Returns the receipt the host shows the person.
#[wasm_bindgen]
pub fn block_adapter_review(
    pack_bytes: &[u8],
    expected_pack_hash: &str,
    component: &[u8],
    disclosure: &str,
) -> Result<String, JsValue> {
    if disclosure.len() > MAX_DISCLOSURE_BYTES {
        return Err(JsValue::from_str("Disclosure exceeds size limit"));
    }
    let disclosure: CapsuleDisclosure =
        serde_json::from_str(disclosure).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let mount = CapsuleMount::review(pack_bytes, expected_pack_hash, component, disclosure)
        .map_err(|e| JsValue::from_str(&e))?;
    if !mount.is_block_adapter() {
        return Err(JsValue::from_str(
            "This pack is not a block adapter: it must ask for power:block-adapter and nothing else",
        ));
    }
    let receipt = json!({
        "name": mount.pack.manifest.name,
        "version": mount.pack.manifest.version,
        "author": mount.pack.manifest.author,
        "asks": mount.pack.manifest.requested_scopes,
        "component_hash": mount.component_hash,
        "component_bytes": component.len(),
        "pack_hash": mount.pack_hash,
        "moot": mount.disclosure.moot,
        "collection": mount.disclosure.collection,
        "revision": mount.disclosure.revision,
    });
    with_mailbox(|m| {
        m.mount = Some(mount);
        m.receipt = Some(receipt.clone());
        Ok(receipt.to_string())
    })
}

/// The person approved the reviewed pack: grant the one power. A new grant needs a new review.
#[wasm_bindgen]
pub fn block_adapter_approve() -> Result<(), JsValue> {
    with_mailbox(|m| {
        m.mount
            .as_mut()
            .ok_or("Review a block adapter first")?
            .approve(vec![BLOCK_ADAPTER.into()])
    })
}

/// The Worker described the approved component: register its claim. Refused unless the pack
/// was approved and the claim names one class.
#[wasm_bindgen]
pub fn block_adapter_install(claim_json: &str) -> Result<String, JsValue> {
    if claim_json.len() > MAX_DISCLOSURE_BYTES {
        return Err(JsValue::from_str("Claim exceeds size limit"));
    }
    let claim: Value =
        serde_json::from_str(claim_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let parsed = WorkerAdapter::claim_from_json(&claim).map_err(|e| JsValue::from_str(&e))?;
    with_mailbox(|m| {
        let mount = m.mount.as_ref().ok_or("Review a block adapter first")?;
        if !mount.permits(BLOCK_ADAPTER) {
            return Err("This block adapter has not been approved".into());
        }
        m.installs.push(claim);
        Ok(parsed.class)
    })
}

/// Requests the pane's adapters want the Worker to answer, as a JSON array of
/// `{class, id, request}`; taking them clears the queue.
#[wasm_bindgen]
pub fn block_adapter_take_requests() -> Result<String, JsValue> {
    with_mailbox(|m| {
        let taken = std::mem::take(&mut m.requests);
        Ok(Value::Array(
            taken
                .into_iter()
                .map(|(class, id, request)| json!({"class": class, "id": id, "request": request}))
                .collect(),
        )
        .to_string())
    })
}

/// The Worker's reply (`{"ok": ...}` or `{"err": ...}`) to request `id` of `class`.
#[wasm_bindgen]
pub fn block_adapter_reply(class: &str, id: f64, reply: &str) -> Result<(), JsValue> {
    if reply.len() > 4 * MAX_DISCLOSURE_BYTES {
        return Err(JsValue::from_str("Reply exceeds size limit"));
    }
    with_mailbox(|m| {
        m.replies.push((class.to_owned(), id as u64, reply.to_owned()));
        Ok(())
    })
}

/// The Worker stopped or the person revoked the adapter: its blocks return to plain text.
#[wasm_bindgen]
pub fn block_adapter_remove(class: &str) -> Result<(), JsValue> {
    with_mailbox(|m| {
        if let Some(mount) = m.mount.as_mut() {
            mount.revoke(BLOCK_ADAPTER);
        }
        m.removals.push(class.to_owned());
        Ok(())
    })
}

/// Open or close the block pane (the document editor) without the page's toolbar. The same pane
/// that "Editable document" opens; its own "Return to graph" closes it.
#[wasm_bindgen]
pub fn block_pane_open(open: bool) -> Result<(), JsValue> {
    with_mailbox(|m| {
        m.open_request = Some(open);
        Ok(())
    })
}

/// Whether the pane asked for a review since the last call; clears the request.
#[wasm_bindgen]
pub fn block_adapter_review_requested() -> Result<bool, JsValue> {
    with_mailbox(|m| Ok(std::mem::take(&mut m.review_requested)))
}

/// The classes installed and the last review receipt, as JSON, for the host's probes.
#[wasm_bindgen]
pub fn block_adapter_receipt() -> Result<String, JsValue> {
    with_mailbox(|m| {
        Ok(json!({
            "installed": m.installed,
            "receipt": m.receipt,
            "approved": m.mount.as_ref().is_some_and(|c| c.permits(BLOCK_ADAPTER)),
            "pending_requests": m.requests.len(),
        })
        .to_string())
    })
}

/// The page's frame hook: move claims, replies and removals from the mailbox into the pane, and
/// the pane's review request out to the host.
pub(super) fn sync(ctx: &mut AppCtx<'_, TreePage, Logic, Child>) {
    let mailbox = ctx.runner.state().shared.block_adapters.clone();
    let review = ctx.runner.state().blocks.review_requested();
    let (installs, removals, replies, open) = {
        let mut m = mailbox.borrow_mut();
        if review {
            m.review_requested = true;
        }
        (
            std::mem::take(&mut m.installs),
            std::mem::take(&mut m.removals),
            std::mem::take(&mut m.replies),
            m.open_request.take(),
        )
    };
    if let Some(open) = open {
        ctx.runner.update(|p| p.blocks.set_open(open));
    }
    if review {
        ctx.runner.update(|p| {
            p.blocks.take_review_request();
        });
    }
    if installs.is_empty() && removals.is_empty() && replies.is_empty() {
        return;
    }
    let wake = ctx.wake.clone();
    ctx.runner.update(|p| {
        for class in &removals {
            p.blocks.remove_adapter(class);
            mailbox.borrow_mut().installed.retain(|c| c != class);
        }
        for claim in &installs {
            let result = WorkerAdapter::claim_from_json(claim).and_then(|claim| {
                let class = claim.class.clone();
                let transport = MailboxTransport {
                    class: class.clone(),
                    mailbox: mailbox.clone(),
                };
                p.blocks
                    .add_worker(WorkerAdapter::new(claim, transport))
                    .map(|()| class)
            });
            match result {
                Ok(class) => mailbox.borrow_mut().installed.push(class),
                Err(error) => p.blocks.set_status(error),
            }
        }
        for (class, id, reply) in &replies {
            p.blocks.deliver(class, *id, reply);
        }
    });
    wake.wake();
}
