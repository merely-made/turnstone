// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Native host for guest block adapters.
//!
//! A guest block adapter is a component pack reviewed once ([`CapsuleMount`], the
//! `power:block-adapter` grant) and run in process under wasmtime ([`block_host`]). This
//! crate is the native counterpart of the web Worker adapter: the same pack, the same review,
//! answering the pane's [`BlockAdapter`] calls inside the frame.

use block_host::{BlockGuestHost, Watchdog, guarded_engine, wasmtime, wit};
use graphshell::block_editing::Change;
use graphshell::block_editing::adapter::{
    Answer, AdapterError, BlockAdapter, BlockInput, Claim, Command, CommandKind, Example, Field,
    GuestView, Row, RunResult, ViewAsk, ViewMode,
};
use graphshell::capsule_applet::{BLOCK_ADAPTER, CapsuleMount};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;
use wasmtime::{Engine, StoreLimits};

/// Epoch ticks a turn may run (at 10 ms a tick: one second).
pub const TURN_TICKS: u64 = 100;

/// An engine with its watchdog; keep both alive as long as any adapter runs on the engine.
pub struct GuestEngine {
    pub engine: Engine,
    _watchdog: Watchdog,
}

impl GuestEngine {
    pub fn new() -> Result<Self, String> {
        let engine = guarded_engine().map_err(|e| e.to_string())?;
        let watchdog = Watchdog::start(engine.clone(), Duration::from_millis(10));
        Ok(Self {
            engine,
            _watchdog: watchdog,
        })
    }
}

/// A guest component answering the pane's calls in process.
pub struct GuestAdapter {
    claim: Claim,
    host: Mutex<BlockGuestHost>,
}

fn attrs_to_wit(attrs: &BTreeMap<String, String>) -> Vec<wit::Attr> {
    attrs
        .iter()
        .map(|(key, value)| wit::Attr {
            key: key.clone(),
            value: value.clone(),
        })
        .collect()
}

fn attrs_from_wit(attrs: Vec<wit::Attr>) -> BTreeMap<String, String> {
    attrs.into_iter().map(|a| (a.key, a.value)).collect()
}

fn block_to_wit(b: &BlockInput) -> wit::Block {
    wit::Block {
        class: b.class.clone(),
        body: b.body.clone(),
        attrs: attrs_to_wit(&b.attrs),
        revision: b.revision.clone(),
        selected: b.selected.clone(),
        pattern: b.pattern.clone(),
    }
}

fn field_from(f: wit::Field) -> Field {
    Field {
        label: f.label,
        default: f.default,
    }
}

fn claim_from(c: wit::Claim) -> Result<Claim, String> {
    if c.class.is_empty() || c.class.contains(char::is_whitespace) {
        return Err("The adapter must claim one class name".into());
    }
    Ok(Claim {
        class: c.class,
        source_key: Some(c.source_key).filter(|k| !k.is_empty()),
        source_versions: c.source_versions,
        pattern: c.pattern.map(field_from),
        commands: c
            .commands
            .into_iter()
            .map(|c| Command {
                id: c.id,
                label: c.label,
                kind: match c.kind {
                    wit::CommandKind::Choice => CommandKind::Choice,
                    wit::CommandKind::Write => CommandKind::Write,
                    wit::CommandKind::Text => CommandKind::Text,
                },
                arg: c.arg.map(field_from),
            })
            .collect(),
        views: c
            .views
            .into_iter()
            .map(|v| ViewMode {
                id: v.id,
                label: v.label,
            })
            .collect(),
        example: c.example.map(|e| Example {
            title: e.title,
            body: e.body,
            attrs: attrs_from_wit(e.attrs),
        }),
    })
}

impl GuestAdapter {
    /// Instantiate the component a reviewed mount approved. The mount must hold the
    /// `power:block-adapter` grant, `bytes` must be exactly the reviewed component, and the
    /// claim must name one class.
    pub fn attach(engine: &GuestEngine, mount: &CapsuleMount, bytes: &[u8]) -> Result<Self, String> {
        if !mount.is_block_adapter() || !mount.permits(BLOCK_ADAPTER) {
            return Err("This component has not been approved as a block adapter".into());
        }
        if !mount.verify_component(bytes) {
            return Err("The component is not the one that was reviewed".into());
        }
        let mut host = BlockGuestHost::attach_bytes_blocking(
            &engine.engine,
            bytes,
            mount.granted(),
            StoreLimits::default(),
            Some(TURN_TICKS),
        )
        .map_err(|e| e.to_string())?;
        let claim = claim_from(host.describe().map_err(|e| e.to_string())?)?;
        Ok(Self {
            claim,
            host: Mutex::new(host),
        })
    }

    fn with<T>(
        &self,
        call: impl FnOnce(&mut BlockGuestHost) -> wasmtime::Result<Result<T, String>>,
    ) -> Answer<T> {
        let mut host = self.host.lock().map_err(|_| "The adapter is unavailable")?;
        match call(&mut host) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(text)) => Err(AdapterError::Refused(text)),
            // A trap (deadline, memory) ends the turn; the adapter stays refused with its text.
            Err(trap) => Err(AdapterError::Refused(format!(
                "The block adapter stopped: {trap}"
            ))),
        }
    }
}

impl BlockAdapter for GuestAdapter {
    fn claim(&self) -> &Claim {
        &self.claim
    }

    fn rows(&self, block: &BlockInput) -> Answer<Vec<Row>> {
        let b = block_to_wit(block);
        self.with(|h| h.rows(&b)).map(|rows| {
            rows.into_iter()
                .map(|r| Row {
                    index: r.index,
                    label: r.label,
                })
                .collect()
        })
    }

    fn choices(&self, block: &BlockInput, command: &str) -> Answer<Vec<Change>> {
        let b = block_to_wit(block);
        self.with(|h| h.choices(&b, command)).map(|changes| {
            changes
                .into_iter()
                .map(|c| Change {
                    index: c.index as usize,
                    label: c.label,
                    before: c.before,
                    after: c.after,
                    conflict: c.conflict,
                    use_update: c.use_update,
                })
                .collect()
        })
    }

    fn apply(
        &self,
        block: &BlockInput,
        command: &str,
        changes: &[Change],
    ) -> Answer<BTreeMap<String, String>> {
        let b = block_to_wit(block);
        let changes: Vec<wit::Change> = changes
            .iter()
            .map(|c| wit::Change {
                index: c.index as u32,
                label: c.label.clone(),
                before: c.before.clone(),
                after: c.after.clone(),
                conflict: c.conflict,
                use_update: c.use_update,
            })
            .collect();
        self.with(|h| h.apply(&b, command, &changes))
            .map(attrs_from_wit)
    }

    fn run(&self, block: &BlockInput, command: &str, arg: &str) -> Answer<RunResult> {
        let b = block_to_wit(block);
        self.with(|h| h.run(&b, command, arg)).map(|r| match r {
            wit::RunResult::Attrs(a) => RunResult::Attrs(attrs_from_wit(a)),
            wit::RunResult::Text(t) => RunResult::Text(t),
        })
    }

    fn view(&self, block: &BlockInput, ask: &ViewAsk) -> Answer<Option<GuestView>> {
        let b = block_to_wit(block);
        let ask = wit::ViewAsk {
            mode: ask.mode.clone(),
            width: ask.width,
            selected: ask.selected,
        };
        let view = self.with(|h| h.view(&b, &ask))?;
        GuestView::decode(&view.scene, &view.sheet, view.width, view.height, view.description)
            .map(Some)
            .map_err(AdapterError::Refused)
    }
}
