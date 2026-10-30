// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn build_livery_snapshot<D>(
    dom: &D,
    sheet: &str,
    w: u32,
    h: u32,
    mut text: TextSystem,
) -> LiverySnapshot<D::NodeId>
where
    D: LayoutDom,
    D::NodeId: Copy + Eq + Hash,
{
    let device = Device::screen(w as f32, h as f32);
    let styles = resolve_styles(
        dom,
        &StyleSet::cambium(&[sheet]),
        &device,
        &InteractionStates::default(),
    );
    crate::reactive_render::completed(
        cambium::reactive_diagnostics::DiagnosticKind::RendererStyleCompleted,
    );
    let images = HashMap::new();
    let result = layout_with_text_system(
        dom,
        &styles,
        w as f32,
        h as f32,
        device.viewport_sizes,
        &mut text,
        &images,
    );
    if result.is_err() {
        crate::reactive_render::completed(
            cambium::reactive_diagnostics::DiagnosticKind::RendererWorkFailed,
        );
    }
    let (styles, fragments) = result.expect("Turnstone Livery/Buckram layout");
    crate::reactive_render::completed(
        cambium::reactive_diagnostics::DiagnosticKind::RendererLayoutCompleted,
    );
    let content_extent = content_extent(dom, &fragments);
    LiverySnapshot {
        styles,
        fragments,
        text,
        content_extent,
    }
}
