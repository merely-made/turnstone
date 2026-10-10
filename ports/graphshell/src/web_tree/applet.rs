// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Graphshell's retained capsule review/reader, over its portable action gate.
//! The JS adapter drives worker turns and fetches; it never renders the guest.

use super::*;
use cambium::{Keyed, button, lens, text_field_typed};
use graphshell::capsule_applet::readings::Readings;
use graphshell::capsule_applet::{
    CapsuleDisclosure, CapsuleMount, MAX_DISCLOSURE_BYTES, NAVIGATE, VIEW,
};
use muniment::{Backend, IndexedDbBackend, WriteOp};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Default)]
pub(super) struct Pane {
    pub mount: Option<CapsuleMount>,
    pub version: u64,
    generation: u32,
    running: bool,
    status: String,
    readings: Readings,
    commands: Vec<Value>,
    refusals: Vec<Value>,
    logs: Vec<String>,
    turns: u64,
}

impl Pane {
    pub(super) fn generation(&self) -> u32 {
        self.generation
    }
    fn command(&mut self, command: &str, data: Value) {
        self.commands.push(json!({
            "generation": self.generation, "command": command, "data": data,
        }));
        self.version += 1;
    }

    fn approve(&mut self, grants: Vec<String>) -> Result<(), String> {
        let mount = self.mount.as_mut().ok_or("No applet review")?;
        mount.approve(grants)?;
        let grants = mount.granted();
        self.generation += 1;
        self.running = true;
        self.status = "Starting the reviewed component".into();
        self.command("start", json!(grants));
        self.search("");
        Ok(())
    }

    fn search(&mut self, query: &str) {
        let Some(mount) = &self.mount else { return };
        let data = json!({"entries": mount.entries(), "query": query});
        self.command(
            "event",
            json!({"kind":"catalogue", "payload": data.to_string()}),
        );
    }

    fn revoke(&mut self, power: &str) {
        let Some(mount) = self.mount.as_mut() else {
            return;
        };
        mount.revoke(power);
        let grants = mount.granted();
        self.status = format!("Revoked {power}; a new grant requires a new review");
        // Already disclosed reading may remain visible. Further opens are gated.
        self.command("grants", json!(grants));
    }

    fn close(&mut self) {
        self.generation += 1;
        self.running = false;
        self.mount = None;
        self.readings.clear();
        self.commands.clear();
        self.command("stop", Value::Null);
    }
}

fn with_pane<R>(f: impl FnOnce(&mut Pane) -> Result<R, String>) -> Result<R, JsValue> {
    TREE.with(|tree| {
        let tree = tree.borrow();
        let tree = tree.as_ref().ok_or("Mount Graphshell's tree first")?;
        let result = f(&mut tree.shared.applet.borrow_mut());
        tree.window.request_redraw();
        result
    })
    .map_err(|e: String| JsValue::from_str(&e))
}

/// Caller is the supplying host. Preparation does not run or grant the applet.
#[wasm_bindgen]
pub fn review_applet(
    pack_bytes: &[u8],
    expected_pack_hash: &str,
    component: &[u8],
    disclosure: &str,
) -> Result<u32, JsValue> {
    if disclosure.len() > MAX_DISCLOSURE_BYTES {
        return Err(JsValue::from_str("Disclosure exceeds size limit"));
    }
    let disclosure: CapsuleDisclosure =
        serde_json::from_str(disclosure).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let mount = CapsuleMount::review(pack_bytes, expected_pack_hash, component, disclosure)
        .map_err(|e| JsValue::from_str(&e))?;
    with_pane(|pane| {
        // Only a completely verified replacement displaces the current mount.
        pane.generation += 1;
        pane.mount = Some(mount);
        pane.running = false;
        pane.readings.clear();
        pane.commands.clear();
        pane.refusals.clear();
        pane.logs.clear();
        pane.turns = 0;
        pane.status = "Verified signatures and content; waiting for your execution review".into();
        pane.command("stop", Value::Null);
        Ok(pane.generation)
    })
}

/// The host's local review command, distinct from the guest envelope.
#[wasm_bindgen]
pub fn applet_confirm(grants: &str) -> Result<(), JsValue> {
    let grants = serde_json::from_str(grants)
        .map_err(|e| JsValue::from_str(&format!("Invalid grants: {e}")))?;
    with_pane(|pane| pane.approve(grants))
}

#[wasm_bindgen]
pub fn applet_revoke(power: &str) -> Result<(), JsValue> {
    with_pane(|pane| {
        pane.revoke(power);
        Ok(())
    })
}

#[wasm_bindgen]
pub fn applet_close() -> Result<(), JsValue> {
    with_pane(|pane| {
        pane.close();
        Ok(())
    })
}

#[wasm_bindgen]
pub fn applet_commands() -> Result<String, JsValue> {
    with_pane(|pane| {
        serde_json::to_string(&std::mem::take(&mut pane.commands)).map_err(|e| e.to_string())
    })
}

#[derive(Deserialize)]
struct WorkerAction {
    name: String,
    payload: Value,
}
#[derive(Deserialize)]
struct WorkerTurn {
    #[serde(default)]
    actions: Vec<WorkerAction>,
    #[serde(default)]
    logs: Vec<String>,
    #[serde(default)]
    refusals: Vec<Value>,
}

#[wasm_bindgen]
pub fn applet_apply_turn(generation: u32, result: &str) -> Result<String, JsValue> {
    if result.len() > MAX_DISCLOSURE_BYTES {
        return Err(JsValue::from_str("Turn exceeds size limit"));
    }
    let turn: WorkerTurn =
        serde_json::from_str(result).map_err(|e| JsValue::from_str(&e.to_string()))?;
    with_pane(|pane| {
        if generation != pane.generation || !pane.running {
            return Err("Stale or stopped applet turn".into());
        }
        let mount = pane.mount.as_mut().ok_or("No active applet")?;
        let mut opens = Vec::new();
        for action in turn.actions {
            let accepted = mount.accept(&action.name, &action.payload.to_string());
            match accepted.and_then(|proposal| mount.lower(proposal)) {
                Ok(Some(url)) => opens.push(url),
                Ok(None) => {},
                Err(refusal) => pane.refusals.push(json!(refusal)),
            }
        }
        pane.logs.extend(turn.logs);
        pane.refusals.extend(turn.refusals);
        if pane.logs.len() > 64 {
            pane.logs.drain(..pane.logs.len() - 64);
        }
        if pane.refusals.len() > 64 {
            pane.refusals.drain(..pane.refusals.len() - 64);
        }
        pane.turns += 1;
        pane.status = if mount.permits(VIEW) {
            format!(
                "{} shared addresses · reading keeps files only when you choose",
                mount.projection.len()
            )
        } else {
            "Applet running without catalogue permission".into()
        };
        pane.version += 1;
        serde_json::to_string(&opens).map_err(|e| e.to_string())
    })
}

#[wasm_bindgen]
pub fn applet_open_body(
    generation: u32,
    url: &str,
    body: &[u8],
    kept: bool,
) -> Result<(), JsValue> {
    with_pane(|pane| {
        if generation != pane.generation || !pane.running {
            return Err("Stale or stopped capsule response".into());
        }
        let opened = pane.mount.as_ref().ok_or("No applet")?.open(url, body)?;
        if let Err(error) = pane.readings.open(opened, kept) {
            pane.status = error.clone();
            pane.refusals.push(json!({"tag":"denied", "val":error}));
        }
        pane.version += 1;
        Ok(())
    })
}

#[wasm_bindgen]
pub fn applet_failed(generation: u32, error: &str) -> Result<(), JsValue> {
    with_pane(|pane| {
        if generation == pane.generation {
            pane.generation += 1;
            pane.running = false;
            if let Some(mount) = pane.mount.as_mut() {
                mount.end_execution();
            }
            pane.status = error.chars().take(300).collect();
            pane.version += 1;
        }
        Ok(())
    })
}

#[derive(Serialize, Deserialize)]
struct KeptCapsule {
    pack_bytes: Vec<u8>,
    body: Vec<u8>,
}

/// Only the explicit host retention command writes a capsule payload.
#[wasm_bindgen]
pub async fn applet_keep(generation: u32, reading: u32) -> Result<bool, JsValue> {
    match keep_reading(generation, reading).await {
        Ok(()) => Ok(true),
        Err(error) => with_pane(|pane| {
            if generation == pane.generation {
                let detail = error
                    .as_string()
                    .unwrap_or_else(|| "Storage or reading unavailable".into());
                pane.status = format!("Could not keep reading {reading}: {detail}")
                    .chars()
                    .take(300)
                    .collect();
                pane.refusals
                    .push(json!({"tag":"retention", "val":pane.status}));
                if pane.refusals.len() > 64 {
                    pane.refusals.remove(0);
                }
                pane.version += 1;
            }
            Ok(false)
        }),
    }
}

async fn keep_reading(generation: u32, reading: u32) -> Result<(), JsValue> {
    let (moot, revision, saved) = with_pane(|pane| {
        if generation != pane.generation {
            return Err("Stale keep command".into());
        }
        let opened = pane
            .readings
            .view(reading)
            .ok_or("Reading is closed or unknown")?
            .opened;
        let mount = pane.mount.as_ref().ok_or("No applet")?;
        let capsule = mount
            .disclosure
            .capsules
            .iter()
            .find(|c| c.entry.revision == opened.entry.revision)
            .ok_or("Not disclosed")?;
        Ok((
            mount.disclosure.moot.clone(),
            opened.entry.revision.clone(),
            KeptCapsule {
                pack_bytes: capsule.pack_bytes.clone(),
                body: opened.body.as_bytes().to_vec(),
            },
        ))
    })?;
    let backend = IndexedDbBackend::open("graphshell-capsule-applets-v1", "muniment")
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let bytes = serde_json::to_vec(&saved).map_err(|e| JsValue::from_str(&e.to_string()))?;
    backend
        .apply(&[WriteOp::Put {
            key: format!("capsule/{moot}/{revision}"),
            value: bytes,
        }])
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    with_pane(|pane| {
        if generation == pane.generation && pane.readings.kept(reading, &revision) {
            let title = pane
                .readings
                .view(reading)
                .expect("confirmed reading")
                .opened
                .entry
                .title
                .clone();
            pane.status = format!("Kept {title} on this device (reading {reading})");
            pane.version += 1;
        }
        Ok(())
    })
}

#[wasm_bindgen]
pub async fn applet_kept_body(generation: u32, url: &str) -> Result<Option<Vec<u8>>, JsValue> {
    let (moot, entry) = with_pane(|pane| {
        if generation != pane.generation {
            return Err("Stale read command".into());
        }
        let mount = pane.mount.as_ref().ok_or("No applet")?;
        let entry = mount
            .entries()
            .into_iter()
            .find(|e| e.url == url)
            .ok_or("Not disclosed")?;
        Ok((mount.disclosure.moot.clone(), entry))
    })?;
    let backend = IndexedDbBackend::open("graphshell-capsule-applets-v1", "muniment")
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let bytes = backend
        .get(&format!("capsule/{moot}/{}", entry.revision))
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    let saved: KeptCapsule =
        serde_json::from_slice(&bytes).map_err(|e| JsValue::from_str(&e.to_string()))?;
    with_pane(|pane| {
        if generation != pane.generation {
            return Err("Stale kept response".into());
        }
        let mount = pane.mount.as_ref().ok_or("No applet")?;
        let capsule = mount
            .disclosure
            .capsules
            .iter()
            .find(|c| c.entry == entry)
            .ok_or("Not disclosed")?;
        if saved.pack_bytes != capsule.pack_bytes {
            return Err("Kept pack differs from disclosed revision".into());
        }
        mount.open(url, &saved.body)?;
        Ok(Some(saved.body))
    })
}

#[wasm_bindgen]
pub fn applet_receipt() -> Result<String, JsValue> {
    with_pane(|pane| {
        let mount = pane.mount.as_ref();
        let selected = pane.readings.selected();
        Ok(json!({
            "generation": pane.generation, "running": pane.running, "turns": pane.turns,
            "component_hash": mount.map(|m| &m.component_hash),
            "pack_hash": mount.map(|m| &m.pack_hash),
            "moot": mount.map(|m| &m.disclosure.moot),
            "collection": mount.map(|m| &m.disclosure.collection),
            "disclosure_revision": mount.map(|m| &m.disclosure.revision),
            "grants": mount.map(|m| m.granted()).unwrap_or_default(),
            "projection": mount.map(|m| &m.projection),
            "readings": pane.readings.views(),
            "selected_reading": selected.as_ref().map(|reading| reading.id),
            "opened": selected.as_ref().map(|reading| reading.opened),
            "kept": selected.as_ref().is_some_and(|reading| reading.kept),
            "status": pane.status, "refusals": pane.refusals, "logs": pane.logs,
        })
        .to_string())
    })
}

/// Local view curation changes neither publication nor retention.
#[wasm_bindgen]
pub fn applet_select_reading(reading: Option<u32>) -> Result<(), JsValue> {
    with_pane(|pane| {
        match reading {
            Some(id) => pane.readings.select(id)?,
            None => pane.readings.deselect(),
        }
        pane.version += 1;
        Ok(())
    })
}

#[wasm_bindgen]
pub fn applet_close_reading(reading: u32) -> Result<(), JsValue> {
    with_pane(|pane| {
        pane.readings.close(reading)?;
        pane.version += 1;
        Ok(())
    })
}

pub(super) fn view(page: &TreePage) -> Child {
    let pane = page.shared.applet.borrow();
    let mount = pane.mount.as_ref().expect("review or active applet");
    let mut children: Vec<Child> = vec![
        Box::new(el("h1", "Graphshell · Moot capsule library")),
        Box::new(el(
            "p",
            format!(
                "{} · shared collection · {} disclosed capsules",
                mount.pack.manifest.name,
                mount.disclosure.capsules.len()
            ),
        )),
        Box::new(
            el("p", pane.status.clone())
                .attr("role", "status")
                .attr("id", "gs-applet-status"),
        ),
    ];
    if !pane.running {
        children.push(Box::new(
            el(
                "section",
                (
                    el("h2", "Review this applet"),
                    el("p", format!("Signed by {}", mount.pack.manifest.author)),
                    el("p", "Catalogue permission lets the applet browse and search this disclosed collection."),
                    el("p", "Reader permission lets it propose opening these capsule addresses."),
                    button("Run catalogue and reader", |page: &mut TreePage, _| {
                        let mut pane = page.shared.applet.borrow_mut();
                        if let Err(error) = pane.approve(vec![VIEW.into(), NAVIGATE.into()]) {
                            pane.status = error;
                        }
                    })
                    .attr("id", "gs-applet-run"),
                    button("Run catalogue only", |page: &mut TreePage, _| {
                        let mut pane = page.shared.applet.borrow_mut();
                        if let Err(error) = pane.approve(vec![VIEW.into()]) {
                            pane.status = error;
                        }
                    })
                    .attr("id", "gs-applet-run-view"),
                ),
            )
            .attr("class", "applet-review"),
        ));
    } else {
        children.push(Box::new(
            el(
                "nav",
                (
                    button("Close applet", |page: &mut TreePage, _| {
                        page.shared.applet.borrow_mut().close()
                    })
                    .attr("id", "gs-applet-close"),
                    button("Revoke navigation", |page: &mut TreePage, _| {
                        page.shared.applet.borrow_mut().revoke(NAVIGATE)
                    })
                    .attr("id", "gs-applet-revoke"),
                ),
            )
            .attr("aria-label", "Applet controls"),
        ));
        if mount.permits(VIEW) {
            children.push(Box::new(el(
                "div",
                (
                    el("label", "Search capsules"),
                    lens(
                        |input: &mut cambium::TextInput| {
                            text_field_typed(input)
                                .attr("aria-label", "Search capsules")
                                .attr("class", "applet-search-field")
                        },
                        |page: &mut TreePage| &mut page.applet_query,
                    ),
                    button("Search", |page: &mut TreePage, _| {
                        page.shared
                            .applet
                            .borrow_mut()
                            .search(page.applet_query.text())
                    })
                    .attr("id", "gs-applet-search"),
                ),
            )));
        }
    }
    let rows: Vec<Child> = mount
        .projection
        .iter()
        .map(|entry| {
            let url = entry.url.clone();
            let can_open = pane.running && mount.permits(NAVIGATE);
            let open = button("Open capsule", move |page: &mut TreePage, _| {
                page.shared.applet.borrow_mut().command(
                    "event",
                    json!({"kind":"open", "payload":json!({"url":url}).to_string()}),
                );
            })
            .attr("aria-label", format!("Open {}", entry.title));
            let open = if can_open {
                open
            } else {
                open.attr("disabled", "")
            };
            Box::new(el(
                "li",
                (
                    el("h2", entry.title.clone()),
                    el(
                        "p",
                        format!("Author {} · {} bytes", &entry.author[..12], entry.bytes),
                    ),
                    open,
                ),
            )) as Child
        })
        .collect();
    children.push(Box::new(
        el("ul", rows)
            .attr("id", "gs-applet-entries")
            .attr("role", "list"),
    ));
    let clear_selection = button("Clear reading selection", |page: &mut TreePage, _| {
        let mut pane = page.shared.applet.borrow_mut();
        pane.readings.deselect();
        pane.version += 1;
    });
    let clear_selection = if pane.readings.selected().is_some() {
        clear_selection
    } else {
        clear_selection.attr("disabled", "")
    };
    children.push(Box::new(clear_selection));
    let readings: Keyed<u32, Child> = pane
        .readings
        .views()
        .into_iter()
        .map(|reading| {
            let id = reading.id;
            let opened = reading.opened;
            let keep = button("Keep this revision", move |page: &mut TreePage, _| {
                page.shared.applet.borrow_mut().command("keep", json!(id))
            })
            .attr("aria-label", format!("Keep revision in reading {id}"));
            let keep = if reading.kept {
                keep.attr("disabled", "")
            } else {
                keep
            };
            let select = button("Select reading", move |page: &mut TreePage, _| {
                let mut pane = page.shared.applet.borrow_mut();
                if let Err(error) = pane.readings.select(id) {
                    pane.status = error;
                }
                pane.version += 1;
            })
            .attr("aria-label", format!("Select reading {id}"))
            .attr(
                "aria-pressed",
                if reading.selected { "true" } else { "false" },
            );
            let url = opened.entry.url.clone();
            let another = button("Open another reading", move |page: &mut TreePage, _| {
                page.shared.applet.borrow_mut().command(
                    "event",
                    json!({"kind":"open", "payload":json!({"url":url}).to_string()}),
                );
            })
            .attr(
                "aria-label",
                format!("Open another reading of {}", opened.entry.title),
            );
            let another = if pane.running && mount.permits(NAVIGATE) {
                another
            } else {
                another.attr("disabled", "")
            };
            let close = button("Close reading", move |page: &mut TreePage, _| {
                let mut pane = page.shared.applet.borrow_mut();
                if let Err(error) = pane.readings.close(id) {
                    pane.status = error;
                }
                pane.version += 1;
            })
            .attr("aria-label", format!("Close reading {id}"));
            let card: Child = Box::new(
                el(
                    "section",
                    (
                        el(
                            "p",
                            format!(
                                "Reading {id}{}",
                                if reading.selected { " · selected" } else { "" }
                            ),
                        ),
                        el("h2", opened.entry.title.clone()),
                        el("pre", opened.body.clone()).attr("id", format!("gs-applet-body-{id}")),
                        el(
                            "p",
                            if reading.kept {
                                "This revision is kept on this device"
                            } else {
                                "Reading has not saved this capsule"
                            },
                        ),
                        el("div", (keep, select, another, close))
                            .attr("class", "applet-reading-controls"),
                    ),
                )
                .attr("id", format!("gs-applet-reader-{id}"))
                .attr("class", "applet-reader")
                .attr(
                    "data-selected",
                    if reading.selected { "true" } else { "false" },
                )
                .attr(
                    "aria-label",
                    format!("Reading {id}: {}", opened.entry.title),
                ),
            );
            (id, card)
        })
        .collect();
    children.push(Box::new(
        el("div", readings).attr("class", "applet-readings"),
    ));
    let viewport = el("main", children).attr("class", "applet-library").attr(
        "style",
        format!(
            "width:{}px;height:{}px;overflow-y:auto;padding:20px;",
            page.size.0, page.size.1
        ),
    );
    Box::new(viewport)
}

pub(super) const SHEET: &str = "\
    .applet-library { display:flex;flex-direction:column;gap:10px;color:#e6f0ee;background:#17232b;box-sizing:border-box; } \
    .applet-library h1 { font-size:23px;margin:0 0 8px; } \
    .applet-library h2 { font-size:18px;margin:8px 0; } \
    .applet-library p { margin:4px 0;overflow-wrap:anywhere; } \
    .applet-library button { color:#edf4f1;background:#294943;border:1px solid #77968f;padding:8px 12px;margin:6px 8px 6px 0; } \
    .applet-library button:disabled { color:#a1b3ae;background:#263837; } \
    .applet-search-field { color:#edf4f1;background:#142723;border:1px solid #77968f;padding:8px;width:280px;min-height:36px; } \
    .applet-library ul { margin:0;padding:0;list-style:none; } \
    .applet-library li { padding:10px 14px;margin-bottom:8px;background:#1f3533;border:1px solid #506c67; } \
    .applet-library pre { color:#e6f0ee;background:#102723;padding:12px;white-space:pre-wrap;font-family:Roboto; } \
    .applet-readings { display:flex;flex-direction:row;flex-wrap:wrap;gap:12px; } \
    .applet-reader { flex:1 1 360px;min-width:260px;padding:12px;border:1px solid #506c67;box-sizing:border-box; } \
    .applet-reader[data-selected=true] { border-color:#b6d3c9; } \
    .applet-reader pre { max-height:180px;overflow:auto; } \
    .applet-reading-controls { display:flex;flex-wrap:wrap;gap:4px; } \
    .applet-reading-controls button { padding:6px 8px;margin:0; } \
    .applet-review { background:#203b36;padding:12px;border:1px solid #739389; }";
