// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Graphshell's block pane in a native Cambium window.
//!
//! `graphshell-blocks [--data DIR] [--adapter PACK.json COMPONENT.wasm]...`
//!
//! The pane is `graphshell::block_pane`, the same one the web page shows. Block adapters are
//! signed component packs reviewed once: each `--adapter` pair is read and verified at start
//! (`CapsuleMount::review`), and nothing runs until the person presses "Approve block adapter"
//! in the review panel, which grants the one `power:block-adapter` and loads the component
//! under wasmtime. "Review block adapters" in the pane opens that panel. The document is kept
//! in `--data DIR` (default `./graphshell-blocks-data`).

use cambium::{AnyView, GenetCtx, GenetElement, button, el, status_note};
use cambium_genet_winit_host::{AppCtx, HostOptions, Init, inert_hooks, run};
use mandrake::{LaneConfig, Product};
use std::cell::RefCell;
use taproot::ProbeSnapshot;
use graphshell::block_editing::sheet_leaf::{ColorF, SheetTheme};
use graphshell::block_pane::{
    self, BackendStorage, Pane, PaneHost, PanePorts, Storage, SwatchStyle, Task, Child,
};
use graphshell::capsule_applet::{BLOCK_ADAPTER, CapsuleDisclosure, CapsuleMount, SignedPack};
use graphshell_blocks::{GuestAdapter, GuestEngine};
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
        let t = SurfaceTheme::default();
        let c = |a: [f32; 4]| ColorF { r: a[0], g: a[1], b: a[2], a: a[3] };
        SheetTheme { ink: c(t.text), accent: c(t.root), muted: c(t.guide) }
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
            .and_then(|()| GuestAdapter::attach(&engine, &c.mount, &c.component))
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
    let pack = std::fs::read(pack_path).map_err(|e| format!("{pack_path}: {e}"))?;
    let component = std::fs::read(component_path).map_err(|e| format!("{component_path}: {e}"))?;
    let signed: SignedPack = serde_json::from_slice(&pack).map_err(|e| format!("{pack_path}: {e}"))?;
    let disclosure = CapsuleDisclosure {
        moot: "local-files".into(),
        collection: "block adapters on this computer".into(),
        revision: hash(&pack),
        capsules: vec![],
    };
    let mount = CapsuleMount::review(&pack, &hash(&pack), &component, disclosure)?;
    Ok(Candidate {
        name: signed.manifest.name.clone(),
        author: signed.manifest.author.clone(),
        asks: signed.manifest.requested_scopes.clone(),
        component_hash: mount.component_hash.clone(),
        size: component.len(),
        component,
        mount,
        state: "Reviewed: signature, address and size verified · not yet approved".into(),
        installed: false,
    })
}

fn root(app: &App) -> Child<App> {
    let mut children: Vec<Child<App>> = Vec::new();
    if let Some(toolbar) = block_pane::toolbar(app) {
        children.push(toolbar);
    }
    if let Some(workspace) = block_pane::workspace(app) {
        children.push(workspace);
    }
    if app.panel {
        children.push(adapters(app));
    }
    Box::new(el("main", children))
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
        children.push(Box::new(status_note("blocks-adapter-state", c.state.clone())));
        children.push(Box::new(
            button("Approve block adapter", move |app: &mut App, _| app.approve(index))
                .attr("aria-disabled", c.installed.to_string())
                .attr("class", "blocks-approve-adapter"),
        ));
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
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--data" => {
                data = PathBuf::from(args.get(i + 1).expect("--data DIR"));
                i += 2;
            },
            "--adapter" => {
                packs.push((
                    args.get(i + 1).expect("--adapter PACK COMPONENT").clone(),
                    args.get(i + 2).expect("--adapter PACK COMPONENT").clone(),
                ));
                i += 3;
            },
            other => panic!("unknown argument {other}; usage: graphshell-blocks [--data DIR] [--adapter PACK COMPONENT]..."),
        }
    }
    std::fs::create_dir_all(&data).expect("data directory");
    let backend = DirectoryBackend::open(data.join("documents")).expect("open the document store");
    let storage: Rc<dyn Storage> = Rc::new(BackendStorage(backend));
    let ports = Rc::new(NativePorts { exports: data.join("exports") });
    let engine = Rc::new(GuestEngine::new().expect("guest engine"));
    let candidates: Vec<Candidate> = packs
        .iter()
        .map(|(p, c)| read_candidate(p, c).unwrap_or_else(|e| panic!("{e}")))
        .collect();
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
            let mut pane = pollster::block_on(Pane::load(Some((storage, "blocks".into())), ports));
            pane.set_open(true);
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
        let mut pane = pollster::block_on(Pane::load(Some((storage, "blocks".into())), ports));
        pane.set_open(true);
        App { pane, size: (1100, 800), engine: Rc::new(GuestEngine::new().unwrap()), candidates, panel: true }
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
