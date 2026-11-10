// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Graphshell's block pane in a native Cambium window.
//!
//! `graphshell-blocks [--n2-woodshed] [--data DIR] [--adapter PACK.json COMPONENT.wasm]... [--moot-disclosure FILE]...`
//!
//! The pane is `graphshell::block_pane`, the same one the web page shows. Block adapters are
//! signed component packs reviewed once: each `--adapter` pair is read and verified at start
//! (`CapsuleMount::review`), and nothing runs until the person presses "Approve block adapter"
//! in the review panel, which grants the one `power:block-adapter` and loads the component
//! under wasmtime. "Review block adapters" in the pane opens that panel. The document is kept
//! in `--data DIR` (default `./graphshell-blocks-data`).
//!
//! This shell has no live Moot connection. `--adapter` is a development input, disclosed as
//! "local-files". `--moot-disclosure FILE` reads the `moot-disclosure.json` that
//! `ports/moot/examples/block_adapter_disclosure.rs` writes from a Moot collection (its packs
//! and components sit beside it): each entry is reviewed against the disclosed pack address, and
//! the panel names the Moot, collection and revision. The Moot's own checks (who contributed,
//! that the collection selected it) were made by that example, not here.

use cambium::{AnyView, GenetCtx, GenetElement, button, el, status_note};
use cambium_genet_winit_host::{AppCtx, HostOptions, Init, inert_hooks, run};
use mandrake::{LaneConfig, Product};
use std::cell::RefCell;
use taproot::ProbeSnapshot;
use graphshell::block_editing::sheet_leaf::{ColorF, SheetTheme};
use cambium::EditorFactory;
use graphshell::block_editing::DefaultEditors;
use graphshell::block_pane::{
    self, BackendStorage, Pane, PaneHost, PanePorts, Storage, SwatchStyle, Task, Child,
};
use graphshell::capsule_applet::{BLOCK_ADAPTER, CapsuleDisclosure, CapsuleMount, SignedPack};
use block_adapter::GuestEngine;
use graphshell_blocks::attach_reviewed;
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use mere::canvas::backdrop_surface::SurfaceTheme;
use muniment::DirectoryBackend;
use std::path::PathBuf;
use std::rc::Rc;

const CSS: &str = ":root{background:#16191d;color:#e8e6e1;font-family:sans-serif;font-size:14px;--tabard-color-text:#e8e6e1;--tabard-color-surface:#22262c;--tabard-color-border:#6d7783;--tabard-color-accent:#8cb4de;--tabard-color-primary:#8cb4de} main{display:flex;flex-direction:column;height:100%;overflow:auto} button{padding:4px;margin:2px;background:#22262c;color:#e8e6e1;border:1px solid #6d7783} .blocks-adapters{padding:8px 14px;border-top:1px solid #6d7783} .blocks-adapters p{margin:3px;overflow-wrap:anywhere}";

struct NativePorts {
    exports: PathBuf,
}
impl PanePorts for NativePorts {
    fn spawn(&self, task: Task) {
        // The directory store answers in one poll; run it to completion here.
        pollster::block_on(task);
    }
    fn export(&self, bytes: &[u8], _mime: &str, name: &str) -> Result<String, String> {
        std::fs::create_dir_all(&self.exports).map_err(|e| e.to_string())?;
        let safe: String = name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
            .collect();
        let path = self.exports.join(safe);
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        Ok(path.display().to_string())
    }
    fn release_export(&self, _handle: &str) {}
}

/// A pack read from disk and reviewed, waiting for approval.
struct Candidate {
    name: String,
    author: String,
    asks: Vec<String>,
    component_hash: String,
    size: usize,
    disclosed_by: String,
    component: Vec<u8>,
    mount: CapsuleMount,
    state: String,
    installed: bool,
}

struct App {
    pane: Pane,
    size: (u32, u32),
    engine: Rc<GuestEngine>,
    candidates: Vec<Candidate>,
    panel: bool,
}

impl PaneHost for App {
    fn pane(&self) -> &Pane {
        &self.pane
    }
    fn pane_mut(&mut self) -> &mut Pane {
        &mut self.pane
    }
    fn size(&self) -> (u32, u32) {
        self.size
    }
    fn sheet_theme(&self) -> SheetTheme {
        SheetTheme::from_surface(&SurfaceTheme::default())
    }
    fn sheet_ground(&self) -> Option<ColorF> {
        Some(SheetTheme::ground_of(&SurfaceTheme::default()))
    }
    fn swatch_style(&self) -> Rc<dyn Fn() -> SwatchStyle> {
        Rc::new(|| SwatchStyle { revision: 1, font_roles: String::new(), theme: SurfaceTheme::default() })
    }
}

type Logic = fn(&App) -> Child<App>;

impl App {
    fn approve(&mut self, index: usize) {
        let engine = self.engine.clone();
        let Some(c) = self.candidates.get_mut(index) else { return };
        if c.installed {
            return;
        }
        let result = c
            .mount
            .approve(vec![BLOCK_ADAPTER.into()])
            .and_then(|()| attach_reviewed(&engine, &c.mount, &c.component))
            .and_then(|adapter| self.pane.add_adapter(Box::new(adapter)));
        match result {
            Ok(()) => {
                c.installed = true;
                c.state = "Approved · loaded".into();
                self.pane.set_status(format!("Block adapter {} installed", c.name));
            },
            Err(error) => {
                c.state = format!("Refused: {error}");
            },
        }
    }
}

fn hash(bytes: &[u8]) -> String {
    eidetic::ManifestId::of_blob(bytes).0.to_hex()
}

fn read_candidate(pack_path: &str, component_path: &str) -> Result<Candidate, String> {
    read_candidate_as(pack_path, component_path, None, |pack| CapsuleDisclosure {
        moot: "local-files".into(),
        collection: "block adapters on this computer".into(),
        revision: hash(pack),
        capsules: vec![],
    })
}

/// The entries a Moot disclosure file lists, each reviewed against its disclosed pack address.
fn read_disclosed(file: &str) -> Result<Vec<Candidate>, String> {
    let text = std::fs::read(file).map_err(|e| format!("{file}: {e}"))?;
    let disclosed: serde_json::Value = serde_json::from_slice(&text).map_err(|e| format!("{file}: {e}"))?;
    let field = |v: &serde_json::Value, name: &str| -> Result<String, String> {
        v.get(name).and_then(|f| f.as_str()).map(str::to_owned).ok_or_else(|| format!("{file}: no {name}"))
    };
    let (moot, collection, revision) =
        (field(&disclosed, "moot")?, field(&disclosed, "collection")?, field(&disclosed, "revision")?);
    let entries = disclosed.get("entries").and_then(|e| e.as_array()).ok_or_else(|| format!("{file}: no entries"))?;
    let dir = std::path::Path::new(file).parent().unwrap_or(std::path::Path::new("."));
    entries
        .iter()
        .map(|entry| {
            read_candidate_as(
                &dir.join(field(entry, "pack_file")?).display().to_string(),
                &dir.join(field(entry, "component_file")?).display().to_string(),
                Some(&field(entry, "pack_hash")?),
                |_| CapsuleDisclosure {
                    moot: moot.clone(),
                    collection: collection.clone(),
                    revision: revision.clone(),
                    capsules: vec![],
                },
            )
        })
        .collect()
}

fn read_candidate_as(
    pack_path: &str,
    component_path: &str,
    disclosed_hash: Option<&str>,
    disclosure: impl FnOnce(&[u8]) -> CapsuleDisclosure,
) -> Result<Candidate, String> {
    let pack = std::fs::read(pack_path).map_err(|e| format!("{pack_path}: {e}"))?;
    let component = std::fs::read(component_path).map_err(|e| format!("{component_path}: {e}"))?;
    let signed: SignedPack = serde_json::from_slice(&pack).map_err(|e| format!("{pack_path}: {e}"))?;
    let disclosure = disclosure(&pack);
    let by = format!("{} · {} · {}", disclosure.moot, disclosure.collection, disclosure.revision);
    let address = disclosed_hash.map_or_else(|| hash(&pack), str::to_owned);
    let mount = CapsuleMount::review(&pack, &address, &component, disclosure)?;
    Ok(Candidate {
        name: signed.manifest.name.clone(),
        author: signed.manifest.author.clone(),
        asks: signed.manifest.requested_scopes.clone(),
        component_hash: mount.component_hash.clone(),
        size: component.len(),
        disclosed_by: by,
        component,
        mount,
        state: "Reviewed: signature, address and size verified · not yet approved".into(),
        installed: false,
    })
}

fn root(app: &App) -> Child<App> {
    let mut children: Vec<Child<App>> = Vec::new();
    if let Some(proof) = block_pane::woodshed_passage(app) {
        children.push(proof);
    } else {
        children.extend(block_pane::toolbar(app));
        children.extend(block_pane::workspace(app));
    }
    if app.panel {
        children.push(adapters(app));
    }
    Box::new(el("main", children))
}

fn approve_offered(candidate: &Candidate) -> bool {
    !candidate.installed
}

fn adapters(app: &App) -> Child<App> {
    let mut children: Vec<Child<App>> = vec![Box::new(el("h2", "Block adapters"))];
    if app.candidates.is_empty() {
        children.push(Box::new(el(
            "p",
            "No block adapter packs were given. Start with --adapter PACK.json COMPONENT.wasm.",
        )));
    }
    for (index, c) in app.candidates.iter().enumerate() {
        children.push(Box::new(el("p", format!("{} · author {}…", c.name, &c.author[..c.author.len().min(16)]))));
        children.push(Box::new(el("p", format!("asks: {} · component {} bytes · {}…", c.asks.join(", "), c.size, &c.component_hash[..c.component_hash.len().min(16)]))));
        children.push(Box::new(el("p", format!("Disclosed by {}", c.disclosed_by))));
        children.push(Box::new(status_note("blocks-adapter-state", c.state.clone())));
        // Approval is done once the adapter is loaded; there is nothing left to approve.
        if approve_offered(c) {
            children.push(Box::new(
                button("Approve block adapter", move |app: &mut App, _| app.approve(index))
                    .attr("class", "blocks-approve-adapter"),
            ));
        }
    }
    children.push(Box::new(
        button("Close review", |app: &mut App, _| app.panel = false).attr("class", "blocks-close-review"),
    ));
    Box::new(el("section", children).attr("class", "blocks-adapters").attr("aria-label", "Block adapters"))
}

/// The scenario lane, as in `graphshell_mere`: `GRAPHSHELL_BLOCKS_SCENARIO` names a scenario, and
/// `_CAPTURE_DIR` and `_RECEIPT` say where the window's own frame captures and receipt go.
struct LaneProduct {
    sheet: String,
}
impl Product for LaneProduct {
    type State = App;
    type Logic = Logic;
    type View = Child<App>;
    const KIND: &'static str = "graphshell-blocks";
    const SURFACE: &'static str = "app";
    const LOG_PREFIX: &'static str = "graphshell-blocks";
    fn sheet(&self) -> &str {
        &self.sheet
    }
    fn busy(&self, ctx: &AppCtx<'_, App, Logic, Child<App>>, capture_pending: bool) -> Option<bool> {
        let app = ctx.runner.state();
        app.pane
            .woodshed_passage_proof()
            .then(|| capture_pending || block_pane::busy(app))
    }
    fn snapshot(&self, ctx: &AppCtx<'_, App, Logic, Child<App>>, _: usize, _dt: f32) -> ProbeSnapshot {
        let app = ctx.runner.state();
        app.pane.probe_fields().into_iter().fold(
            ProbeSnapshot::default()
                .with_field("panel-open", app.panel.to_string())
                .with_field(
                    "adapter-state",
                    app.candidates.first().map(|c| c.state.clone()).unwrap_or_default(),
                )
                .with_field(
                    "adapter-approve-offered",
                    app.candidates.iter().any(approve_offered).to_string(),
                )
                .with_field(
                    "adapter-installed",
                    app.candidates.iter().any(|c| c.installed).to_string(),
                ),
            |snapshot, (name, value)| snapshot.with_field(name, value),
        )
    }
}

fn has_label(dom: &genet_scripted_dom::ScriptedDom, node: genet_scripted_dom::NodeId, label: &str) -> bool {
    dom.attribute(node, &Namespace::default(), &LocalName::from("aria-label")) == Some(label)
        && dom
            .attribute(node, &Namespace::default(), &LocalName::from("data-cambium-text-value"))
            .is_some()
}

fn main() {
    let mut data = PathBuf::from("graphshell-blocks-data");
    let mut packs: Vec<(String, String)> = Vec::new();
    let mut moots: Vec<String> = Vec::new();
    let mut woodshed_passage = false;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--n2-woodshed" => {
                woodshed_passage = true;
                i += 1;
            },
            "--data" => {
                data = PathBuf::from(args.get(i + 1).expect("--data DIR"));
                i += 2;
            },
            "--moot-disclosure" => {
                moots.push(args.get(i + 1).expect("--moot-disclosure FILE").clone());
                i += 2;
            },
            "--adapter" => {
                packs.push((
                    args.get(i + 1).expect("--adapter PACK COMPONENT").clone(),
                    args.get(i + 2).expect("--adapter PACK COMPONENT").clone(),
                ));
                i += 3;
            },
            other => panic!("unknown argument {other}; usage: graphshell-blocks [--n2-woodshed] [--data DIR] [--adapter PACK COMPONENT]... [--moot-disclosure FILE]..."),
        }
    }
    std::fs::create_dir_all(&data).expect("data directory");
    let backend = DirectoryBackend::open(data.join("documents")).expect("open the document store");
    let storage: Rc<dyn Storage> = Rc::new(BackendStorage(backend));
    let ports = Rc::new(NativePorts { exports: data.join("exports") });
    let engine = Rc::new(GuestEngine::new().expect("guest engine"));
    let mut candidates: Vec<Candidate> = packs
        .iter()
        .map(|(p, c)| read_candidate(p, c).unwrap_or_else(|e| panic!("{e}")))
        .collect();
    for file in &moots {
        candidates.extend(read_disclosed(file).unwrap_or_else(|e| panic!("{e}")));
    }
    let lane = LaneConfig::from_env("GRAPHSHELL_BLOCKS").map(|config| {
        let product = LaneProduct {
            sheet: format!("{CSS}\n{}", block_pane::SHEET),
        };
        let lane = if std::env::var_os("GRAPHSHELL_BLOCKS_JSON_RECEIPT").is_some() {
            let scenario = taproot::Scenario::parse(
                &std::fs::read_to_string(&config.scenario).expect("scenario file"),
            )
            .expect("scenario");
            mandrake::Lane::new(
                product,
                Some(scenario),
                config.receipt_path(),
                None,
                Rc::new(std::cell::Cell::new(0)),
            )
        } else {
            mandrake::Lane::from_config(config, product, cambium_genet_winit_host::read_file)
                .expect("scenario")
        };
        lane.with_presentation_timeout(std::time::Duration::from_secs(
            std::env::var("GRAPHSHELL_BLOCKS_PRESENTATION_TIMEOUT_SECS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(10)
                .clamp(10, 60),
        ))
    });
    let lane = Rc::new(RefCell::new(lane));
    let mut hooks = inert_hooks::<App, Logic, Child<App>>();
    hooks.after_frame = Box::new(move |ctx| {
        if let Some(lane) = lane.borrow_mut().as_mut() {
            lane.after_frame(ctx);
        }
    });
    hooks.frame = Box::new(|ctx| {
        let size = (ctx.logical_size.0.round() as u32, ctx.logical_size.1.round() as u32);
        if size != ctx.runner.state().size {
            ctx.runner.update(|app| app.size = size);
        }
        if ctx.runner.state().pane.review_requested() {
            ctx.runner.update(|app| {
                app.pane.take_review_request();
                app.panel = true;
            });
        }
        block_pane::register(ctx);
        false
    });
    hooks.focused_text = Box::new(|runner| block_pane::focused_text(runner, has_label));
    run(
        HostOptions {
            title: "Graphshell · block pane".into(),
            initial_logical_size: (1100., 800.),
            size_env: Some(("GRAPHSHELL_BLOCKS_WIDTH".into(), "GRAPHSHELL_BLOCKS_HEIGHT".into())),
            ..Default::default()
        },
        move |_, _, _wake| {
            let storage = (!woodshed_passage).then(|| (storage, "blocks".into()));
            let mut pane = pollster::block_on(Pane::load_with(storage, ports, editors()));
            if woodshed_passage {
                pane.enable_woodshed_passage_proof();
            } else {
                pane.set_open(true);
            }
            let panel = !candidates.is_empty();
            Init {
                state: App { pane, size: (1100, 800), engine, candidates, panel },
                logic: root as Logic,
                sheet: format!("{CSS}\n{}", block_pane::SHEET),
                fonts: Vec::new(),
                images: Vec::new(),
            }
        },
        hooks,
    )
    .expect("event loop");
}

#[cfg(test)]
mod tests {
    use super::*;
    use eidetic::pack::{PackManifest, PackPart, PackPartRole, sign_pack};
    use eidetic::{ManifestId, TrustEnvelope};
    use personae::{IdentityProvider, InMemoryProvider};

    fn fixture_files(dir: &std::path::Path) -> (String, String) {
        let component = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../crates/script/block-host/guest/target/wasm32-wasip2/release/block_fixture_guest.wasm"),
        )
        .expect("build the fixture guest with `cargo build -p block-host`");
        let author = InMemoryProvider::from_seed([5; 32]);
        let manifest = PackManifest {
            name: "fixture-adapter".into(),
            version: "1".into(),
            author: author.master_public_key().to_bytes().iter().map(|b| format!("{b:02x}")).collect(),
            requested_scopes: vec![BLOCK_ADAPTER.into()],
            parts: vec![PackPart {
                name: "g".into(),
                role: PackPartRole::WasmComponent,
                blob: ManifestId::of_blob(&component),
                bytes: component.len() as u64,
            }],
        };
        let mut trust = TrustEnvelope::self_asserted();
        trust.signatures.push(sign_pack(&manifest, author.master_keypair()));
        let pack = serde_json::to_vec(&SignedPack { manifest, trust }).unwrap();
        let (p, c) = (dir.join("pack.json"), dir.join("guest.wasm"));
        std::fs::write(&p, pack).unwrap();
        std::fs::write(&c, component).unwrap();
        (p.display().to_string(), c.display().to_string())
    }

    fn app(candidates: Vec<Candidate>) -> App {
        let dir = std::env::temp_dir().join(format!("gsb-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let backend = DirectoryBackend::open(dir.join(format!("d{}", candidates.len()))).unwrap();
        let storage: Rc<dyn Storage> = Rc::new(BackendStorage(backend));
        let ports = Rc::new(NativePorts { exports: dir.join("exports") });
        let mut pane = pollster::block_on(Pane::load_with(Some((storage, "blocks".into())), ports, editors()));
        pane.set_open(true);
        App { pane, size: (1100, 800), engine: Rc::new(GuestEngine::new().unwrap()), candidates, panel: true }
    }

    #[test]
    fn a_moot_disclosure_is_reviewed_at_its_disclosed_address_and_names_its_moot() {
        let dir = std::env::temp_dir().join(format!("gsb-moot-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (pack, component) = fixture_files(&dir);
        let address = hash(&std::fs::read(&pack).unwrap());
        let write = |address: &str| {
            let file = dir.join("moot-disclosure.json");
            std::fs::write(
                &file,
                serde_json::json!({
                    "moot": "81", "collection": "82", "revision": "rev-1",
                    "entries": [{ "pack_hash": address, "pack_file": "pack.json", "component_file": "guest.wasm" }],
                })
                .to_string(),
            )
            .unwrap();
            file.display().to_string()
        };
        let candidates = read_disclosed(&write(&address)).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].disclosed_by, "81 · 82 · rev-1");
        assert_eq!(candidates[0].mount.disclosure.revision, "rev-1");
        assert!(!candidates[0].installed, "disclosure loads nothing");
        let _ = component;
        // A pack served under another address than the one disclosed is not reviewed.
        assert!(read_disclosed(&write(&"0".repeat(address.len()))).is_err());
    }

    #[test]
    fn a_pack_is_reviewed_at_start_and_nothing_loads_until_approval() {
        let dir = std::env::temp_dir().join(format!("gsb-pack-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (pack, component) = fixture_files(&dir);
        let candidate = read_candidate(&pack, &component).unwrap();
        assert_eq!(candidate.asks, [BLOCK_ADAPTER]);
        assert!(!candidate.installed);
        let mut app = app(vec![candidate]);
        assert_eq!(app.pane.classes(), ["metre"], "review loads nothing");
        app.approve(0);
        assert_eq!(app.pane.classes(), ["metre", "fixture"]);
        assert!(app.candidates[0].installed);
        assert!(app.candidates[0].state.contains("loaded"));
        assert!(!approve_offered(&app.candidates[0]), "a loaded pack offers no Approve");
        app.approve(0);
        assert_eq!(app.pane.classes().len(), 2, "approving twice installs nothing more");
        // A tampered component is not even reviewed.
        let mut bad = std::fs::read(&component).unwrap();
        bad.push(0);
        let tampered = dir.join("tampered.wasm");
        std::fs::write(&tampered, bad).unwrap();
        assert!(read_candidate(&pack, &tampered.display().to_string()).is_err());
    }

    #[test]
    fn the_window_view_builds_with_the_review_panel() {
        let app = app(Vec::new());
        let _ = root(&app);
        assert!(app.pane.open());
    }
}

/// The editors this binary's pane edits through: Graphshell's own buffer. A
/// product that composes the pane with another editor (Knot's, through its
/// `KnotBlockEditor::factory`) passes that factory here instead.
fn editors() -> Rc<dyn EditorFactory> {
    Rc::new(DefaultEditors)
}
