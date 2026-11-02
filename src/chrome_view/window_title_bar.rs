// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn window_title_bar(state: &ChromeState, win: &WindowChrome, slot: usize) -> ChromeView {
    let height = navigation_height(state.zoom).max(win.titlebar_insets.height);
    let merged = win.primary && state.shellbar_visible && !state.layout.toolbar_bottom;
    let title: cambium::TitleBarSlot<ChromeState> = Box::new(
        el(
            "div",
            (
                el("span", if merged { "" } else { "Turnstone" })
                    .attr("class", "turnstone-title-name"),
                el(
                    "span",
                    if merged {
                        String::new()
                    } else if win.primary {
                        "Workspace".into()
                    } else {
                        format!("Lens {slot}")
                    },
                )
                .attr("class", "turnstone-title-context"),
            ),
        )
        .attr("class", "turnstone-title-identity"),
    );
    let ornament: cambium::TitleBarSlot<ChromeState> = Box::new(
        el("span", el("span", ()).attr("class", "turnstone-stone-vein"))
            .attr(
                "class",
                if merged && win.w - win.titlebar_insets.left < 300.0 * state.zoom {
                    "turnstone-stone turnstone-stone-hidden"
                } else {
                    "turnstone-stone"
                },
            )
            .attr("aria-hidden", "true"),
    );
    let bar = cambium::title_bar(
        ornament,
        title,
        Box::new(el("span", ())),
        Box::new(el("span", ())),
    );
    // The identity slots are inert; window dragging is interpreted by the
    // native shell from Cambium's computed app-region after retained hit testing.
    let bar = cambium::map_message_result(bar, |_: &mut ChromeState, result| match result {
        cambium::MessageResult::Action(()) | cambium::MessageResult::Nop => {
            cambium::MessageResult::<ChromeIntent>::Nop
        },
        cambium::MessageResult::RequestRebuild => cambium::MessageResult::RequestRebuild,
        cambium::MessageResult::Stale => cambium::MessageResult::Stale,
    });
    Box::new(el::<_, ChromeState, ChromeIntent>("div", bar)
        .attr("class", "turnstone-title-frame")
        .attr("style", format!(
            "position:absolute; transform:translate(0px, 0px); width:{}px; height:{height}px; --titlebar-min-height:{height}px; --titlebar-gap:{}px; {}",
            win.w, 10.0 * state.zoom, win.titlebar_insets.declarations(win.w),
        )))
}
