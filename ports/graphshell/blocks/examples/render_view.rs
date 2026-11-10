// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Headless receipt: a reviewed guest's view, painted by the host's own sheet painter and
//! rasterised through Netrender and Vello CPU, one PNG per reading.
//!
//! `render_view COMPONENT.wasm OUT_DIR [CLASS-EXAMPLE-ARG]` signs the component with a
//! throwaway test key, reviews and approves it, loads the guest, inserts its example block,
//! runs its `write-notation` command when it has one, and writes `<mode>.png` for every view
//! mode the guest claims. Not a window: no display is needed.

use eidetic::pack::{PackManifest, PackPart, PackPartRole, sign_pack};
use eidetic::{ManifestId, TrustEnvelope};
use block_adapter::{BlockAdapter, BlockInput, CommandKind, RunResult, ViewAsk};
use graphshell::block_editing::sheet_leaf::{ColorF, SheetLeaf, SheetTheme};
use graphshell::capsule_applet::{BLOCK_ADAPTER, CapsuleDisclosure, CapsuleMount, SignedPack};
use block_adapter::GuestEngine;
use graphshell_blocks::attach_reviewed;
use netrender::vello_backends::{CpuSession, SparseResourceLimits};
use paint_list_api::DeviceIntSize;
use paint_list_render::{CompositeLayer, composite_paint_layers};
use personae::{IdentityProvider, InMemoryProvider};
use sprigging::{Leaf, PaintCx, Size};

const LIGHT_PAGE: ColorF = ColorF { r: 0.97, g: 0.97, b: 0.95, a: 1.0 };
const DARK_PAGE: ColorF = ColorF { r: 0.06, g: 0.08, b: 0.10, a: 1.0 };

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&args[1]).expect("component");
    let out = std::path::PathBuf::from(&args[2]);
    std::fs::create_dir_all(&out).unwrap();
    let author = InMemoryProvider::from_seed([9; 32]);
    let manifest = PackManifest {
        name: "render-view".into(),
        version: "0".into(),
        author: author.master_public_key().to_bytes().iter().map(|b| format!("{b:02x}")).collect(),
        requested_scopes: vec![BLOCK_ADAPTER.into()],
        parts: vec![PackPart {
            name: "guest.wasm".into(),
            role: PackPartRole::WasmComponent,
            blob: ManifestId::of_blob(&bytes),
            bytes: bytes.len() as u64,
        }],
    };
    let mut trust = TrustEnvelope::self_asserted();
    trust.signatures.push(sign_pack(&manifest, author.master_keypair()));
    let pack = serde_json::to_vec(&SignedPack { manifest, trust }).unwrap();
    let mut mount = CapsuleMount::review(
        &pack,
        &ManifestId::of_blob(&pack).0.to_hex(),
        &bytes,
        CapsuleDisclosure { moot: "m".into(), collection: "c".into(), revision: "r".into(), capsules: vec![] },
    )
    .unwrap();
    mount.approve(vec![BLOCK_ADAPTER.into()]).unwrap();
    let engine = GuestEngine::new().unwrap();
    let adapter = attach_reviewed(&engine, &mount, &bytes).unwrap();
    let claim = adapter.claim().clone();
    let example = claim.example.clone().expect("example");
    let mut attrs = example.attrs.clone();
    attrs.insert("id".into(), "render".into());
    let mut block = BlockInput {
        class: claim.class.clone(),
        body: example.body.clone(),
        attrs,
        revision: "r".into(),
        selected: vec![],
        pattern: claim.pattern.as_ref().map(|p| p.default.clone()).unwrap_or_default(),
    };
    for command in claim.commands.iter().filter(|c| c.kind == CommandKind::Write) {
        let arg = command.arg.as_ref().map(|a| a.default.clone()).unwrap_or_default();
        if let RunResult::Attrs(next) = adapter.run(&block, &command.id, &arg).expect("write") {
            block.attrs = next;
        }
    }
    // `RENDER_THEME=dark` paints the dark pane the web host shows by default.
    let dark = std::env::var("RENDER_THEME").is_ok_and(|t| t == "dark");
    let (page, ink, accent) = if dark {
        (DARK_PAGE, ColorF { r: 0.91, g: 0.90, b: 0.88, a: 1.0 }, ColorF { r: 0.85, g: 0.72, b: 0.45, a: 1.0 })
    } else {
        (LIGHT_PAGE, ColorF { r: 0.08, g: 0.08, b: 0.10, a: 1.0 }, ColorF { r: 0.10, g: 0.40, b: 0.85, a: 1.0 })
    };
    let theme = SheetTheme { ink, accent, muted: ColorF { r: 0.5, g: 0.5, b: 0.5, a: 1.0 } };
    for mode in &claim.views {
        let view = adapter
            .view(&block, &ViewAsk { mode: mode.id.clone(), width: 640.0, selected: Some((0, 1)) })
            .expect("view")
            .expect("a view");
        let mut leaf = SheetLeaf::new(view.sheet.clone(), theme).expect("paints").on_ground(page);
        let (w, h) = (view.width.ceil() as u32, view.height.ceil() as u32);
        let mut cmds = Vec::new();
        {
            let mut cx = PaintCx::new(&mut cmds, Size { width: w as f32, height: h as f32 });
            cx.fill_rect(0.0, 0.0, w as f32, h as f32, page);
        }
        leaf.paint(&mut PaintCx::new(&mut cmds, Size { width: w as f32, height: h as f32 }));
        let scene = composite_paint_layers(
            DeviceIntSize::new(w as i32, h as i32),
            &[CompositeLayer::commands_only(&cmds)],
        )
        .unwrap()
        .scene;
        let mut cpu = CpuSession::new(SparseResourceLimits::default());
        let pixels = cpu.render(&scene).unwrap();
        let file = std::fs::File::create(out.join(format!("{}_{}.png", claim.class, mode.id))).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(pixels.data_as_u8_slice()).unwrap();
        println!(
            "{}: {}x{}, {} regions, {} text runs, {}",
            mode.id, w, h, view.regions().len(), leaf.texts().len(), view.description
        );
        // This receipt paints paths only; a host draws the text runs over it in its own fonts.
        // Print what each run would be drawn in, and how it reads against the page.
        for run in leaf.texts() {
            println!(
                "  text {:?} at ({:.0},{:.0}) {} contrast {:.2}",
                run.text, run.x, run.y, run.css_colour(),
                graphshell::block_editing::sheet_leaf::contrast_ratio(run.colour, page)
            );
        }
    }
}
