// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! The web page's binding of the block pane (`graphshell::block_pane`): IndexedDB storage, object
//! URLs for exports, `spawn_local` for futures, the page's appearance for colours.
use super::*;
use graphshell::block_editing::sheet_leaf::{ColorF, SheetTheme};
use graphshell::block_pane::{
    self, BackendStorage, PaneHost, PanePorts, Storage, SwatchStyle, Task,
};

pub(super) use block_pane::SHEET;
pub(super) type Pane = block_pane::Pane;

struct WebPorts;
impl PanePorts for WebPorts {
    fn spawn(&self, task: Task) {
        wasm_bindgen_futures::spawn_local(task);
    }
    fn export(&self, bytes: &[u8], mime: &str, name: &str) -> Result<String, String> {
        backdrop::export_material(bytes, mime, name)
    }
    fn release_export(&self, handle: &str) {
        let _ = web_sys::Url::revoke_object_url(handle);
    }
}

pub(super) async fn load(product: Option<&product::SavedProduct>) -> Pane {
    let storage = product
        .and_then(|p| p.forme_source())
        .map(|(backend, id)| (Rc::new(BackendStorage(backend)) as Rc<dyn Storage>, id.to_string()));
    Pane::load(storage, Rc::new(WebPorts)).await
}

impl PaneHost for TreePage {
    fn pane(&self) -> &Pane {
        &self.blocks
    }
    fn pane_mut(&mut self) -> &mut Pane {
        &mut self.blocks
    }
    fn size(&self) -> (u32, u32) {
        self.size
    }
    fn sheet_theme(&self) -> SheetTheme {
        let theme = self.shared.appearance.borrow().backdrop_theme();
        let color = |a: [f32; 4]| ColorF {
            r: a[0],
            g: a[1],
            b: a[2],
            a: a[3],
        };
        SheetTheme {
            ink: color(theme.text),
            accent: color(theme.root),
            muted: color(theme.guide),
        }
    }
    fn swatch_style(&self) -> Rc<dyn Fn() -> SwatchStyle> {
        let appearance = self.shared.appearance.clone();
        Rc::new(move || {
            let appearance = appearance.borrow();
            SwatchStyle {
                revision: appearance.revision,
                font_roles: appearance.font_roles(),
                theme: appearance.backdrop_theme(),
            }
        })
    }
    fn available(&self) -> bool {
        self.product.is_some()
    }
}

pub(super) fn control(page: &TreePage) -> Child {
    block_pane::control(page)
}
pub(super) fn active(page: &TreePage) -> bool {
    block_pane::active(page)
}
pub(super) fn toolbar(page: &TreePage) -> Option<Child> {
    block_pane::toolbar(page)
}
pub(super) fn busy(page: &TreePage) -> bool {
    block_pane::busy(page)
}
pub(super) fn workspace(page: &TreePage) -> Option<Child> {
    block_pane::workspace(page)
}
pub(super) fn focused_text(
    runner: &cambium_rootstock::Runner<TreePage, Logic, Child>,
) -> Option<cambium_rootstock::FocusedTextSlot<TreePage>> {
    block_pane::focused_text(runner, |dom: &genet_scripted_dom::ScriptedDom, node, label| {
        focused_field_has_label(dom, node, label)
    })
}
pub(super) fn register(ctx: &mut AppCtx<'_, TreePage, Logic, Child>) {
    block_pane::register(ctx);
    #[cfg(feature = "applets")]
    super::block_adapters::sync(ctx);
}
pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    page.blocks
        .probe_fields()
        .into_iter()
        .fold(snapshot, |snapshot, (name, value)| snapshot.with_field(name, value))
}
