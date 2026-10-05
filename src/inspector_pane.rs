// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The Inspector pane on cambium's `detail_panel` — the catalog entry the
//! surfaces-in-cambium mapping named for it (key/value sections plus declared
//! actions).
//!
//! `inspector_view` is the data half (app truth -> sections); this is the
//! view half: sections handed to the panel, composited at the pane's rect
//! like every other Cambium pane. Viewer controls write the followed graph
//! member through the product action spine; the Knot clip button uses its
//! configured endpoint handle.

use std::cell::RefCell;
use std::rc::Rc;

use cambium::{
    AnyView, DetailRow, DetailSection, DomHandle, GenetCtx, GenetElement, Key, NamedKey,
    PointerClick, RadioGroup, View, button, detail_panel, el, lens, map_message_result, on_click,
    on_key, request_focus,
};
use genet_scripted_dom::ScriptedDom;

use crate::app::App;
use crate::inspector_view::{InspectorSection, inspector_member, inspector_sections_for_pane};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InspectorIntent {
    ClipToKnot,
    SetViewer {
        member: uuid::Uuid,
        viewer: Option<String>,
    },
}

impl cambium::Action for InspectorIntent {}

struct InspectorState {
    sections: Vec<InspectorSection>,
    member: Option<uuid::Uuid>,
    radio: RadioGroup,
    viewer_options: Vec<crate::inspector_controls::ViewerOption>,
    capabilities: Vec<String>,
    clip_target: Option<String>,
    clip_source_available: bool,
    clip_status: String,
    viewport_w: f32,
    viewport_h: f32,
}

type InspectorView = Box<dyn AnyView<InspectorState, InspectorIntent, GenetCtx, GenetElement>>;
type InspectorRunner = cambium::GenetAppRunner<
    InspectorState,
    fn(&InspectorState) -> InspectorView,
    InspectorView,
    InspectorIntent,
>;

/// The viewer uses Cambium's radio keyboard pattern with host availability.
/// Disabled rows retain saved identity but cannot activate or receive focus.
fn viewer_radio_group(
    state: &RadioGroup,
    options: &[crate::inspector_controls::ViewerOption],
) -> impl View<RadioGroup, (), GenetCtx, Element = GenetElement> + use<> {
    let enabled: Rc<Vec<usize>> = Rc::new(
        options
            .iter()
            .enumerate()
            .filter_map(|(index, option)| option.selectable.then_some(index))
            .collect(),
    );
    let active = if enabled.contains(&state.selected) {
        state.selected
    } else {
        enabled.first().copied().unwrap_or(0)
    };
    let items: Vec<_> = options
        .iter()
        .enumerate()
        .map(|(index, option)| {
            let selected = index == state.selected;
            let selectable = option.selectable;
            let enabled = enabled.clone();
            let item = on_click(
                el::<_, RadioGroup, ()>(
                    "div",
                    format!("{}{}", if selected { "(o) " } else { "( ) " }, option.label,),
                )
                .attr("role", "radio")
                .attr("aria-checked", if selected { "true" } else { "false" })
                .attr("aria-disabled", if selectable { "false" } else { "true" })
                .attr(
                    "data-engine-id",
                    option.viewer.clone().unwrap_or_else(|| "auto".into()),
                )
                .attr(
                    "tabindex",
                    if index == active && selectable {
                        "0"
                    } else {
                        "-1"
                    },
                )
                .attr(
                    "class",
                    match (selected, selectable) {
                        (true, true) => "radio selected",
                        (false, true) => "radio",
                        (true, false) => "radio selected muted",
                        (false, false) => "radio muted",
                    },
                ),
                move |state: &mut RadioGroup, _| {
                    if selectable {
                        state.selected = index;
                        state.focus_request = None;
                    }
                },
            );
            let keyboard = on_key(item, move |state: &mut RadioGroup, event| {
                if !selectable || enabled.is_empty() {
                    return;
                }
                let position = enabled
                    .iter()
                    .position(|candidate| *candidate == index)
                    .unwrap();
                let next = match event.key {
                    Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowUp) => {
                        enabled[(position + enabled.len() - 1) % enabled.len()]
                    },
                    Key::Named(NamedKey::ArrowRight | NamedKey::ArrowDown) => {
                        enabled[(position + 1) % enabled.len()]
                    },
                    Key::Named(NamedKey::Home) => enabled[0],
                    Key::Named(NamedKey::End) => enabled[enabled.len() - 1],
                    Key::Named(NamedKey::Space) => index,
                    _ => return,
                };
                state.selected = next;
                state.focus_request = Some(next);
                event.prevent_default();
            })
            .focusable(index == active && selectable);
            request_focus(keyboard, state.focus_request == Some(index) && selectable)
        })
        .collect();
    el::<_, RadioGroup, ()>("div", items)
        .attr("role", "radiogroup")
        .attr("aria-label", state.label.clone())
}

fn inspector_pane_view(state: &InspectorState) -> InspectorView {
    let sections: Vec<DetailSection> = state
        .sections
        .iter()
        .map(|s| {
            DetailSection::new(
                s.title.clone(),
                s.rows
                    .iter()
                    .map(|(k, v)| DetailRow::new(k.clone(), v.clone()))
                    .collect(),
            )
        })
        .collect();
    let clip_label = match (&state.clip_target, state.clip_source_available) {
        (Some(target), true) => format!("Clip document to {target}"),
        (None, _) => "Knot clipping is not configured".into(),
        (Some(_), false) => "This document cannot supply a clip".into(),
    };
    let clip_status = el::<_, InspectorState, InspectorIntent>(
        "div",
        format!("Knot clip: {}", state.clip_status),
    )
    .attr("class", "detail-value");
    let clip_button = button(
        clip_label,
        |state: &mut InspectorState, _click: PointerClick| {
            (state.clip_target.is_some() && state.clip_source_available)
                .then_some(InspectorIntent::ClipToKnot)
        },
    )
    .attr(
        "class",
        if state.clip_target.is_some() && state.clip_source_available {
            "list-row action"
        } else {
            "list-row muted"
        },
    )
    .attr(
        "aria-disabled",
        if state.clip_target.is_some() && state.clip_source_available {
            "false"
        } else {
            "true"
        },
    )
    .attr("style", "white-space: normal; overflow-wrap: anywhere;");
    let viewer_options = state.viewer_options.clone();
    let viewer = map_message_result(
        lens(
            move |radio: &mut RadioGroup| viewer_radio_group(radio, &viewer_options),
            |state: &mut InspectorState| &mut state.radio,
        ),
        |_state, message| match message {
            cambium::MessageResult::Action(()) | cambium::MessageResult::Nop => {
                cambium::MessageResult::<InspectorIntent>::Nop
            },
            cambium::MessageResult::RequestRebuild => cambium::MessageResult::RequestRebuild,
            cambium::MessageResult::Stale => cambium::MessageResult::Stale,
        },
    );
    let capabilities = state
        .capabilities
        .iter()
        .cloned()
        .map(|line| el::<_, InspectorState, InspectorIntent>("div", line).attr("class", "list-row"))
        .collect::<Vec<_>>();
    Box::new(
        el::<_, InspectorState, InspectorIntent>(
            "div",
            (
                el::<_, InspectorState, InspectorIntent>("div", "Viewer")
                    .attr("class", "list-section-title"),
                viewer,
                el::<_, InspectorState, InspectorIntent>("div", "Document controls")
                    .attr("class", "list-section-title"),
                capabilities,
                clip_status,
                clip_button,
                detail_panel(&sections),
            ),
        )
        .attr("class", "pane")
        .attr(
            "style",
            format!(
                "width: {}px; height: {}px;",
                state.viewport_w, state.viewport_h
            ),
        ),
    )
}

/// The Inspector pane: a retained cambium runner over the detail sections.
/// Held by the shell like the other panes.
pub struct InspectorPane {
    dom: DomHandle,
    runner: InspectorRunner,
    scroll: crate::ui::PaneScroll,
    /// Kept across frames. Rebuilding a layout per paint re-cascaded and
    /// re-shaped the whole pane to draw an unchanged screen; see
    /// [`crate::ui::RetainedLayout`] for the measurement.
    layout: crate::ui::RetainedLayout,
}

impl InspectorPane {
    pub fn new() -> Self {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = InspectorState {
            sections: Vec::new(),
            member: None,
            radio: RadioGroup::new(0).with_label("Viewer"),
            viewer_options: vec![crate::inspector_controls::ViewerOption {
                viewer: None,
                label: "Auto".into(),
                selectable: true,
            }],
            capabilities: Vec::new(),
            clip_target: None,
            clip_source_available: false,
            clip_status: "unconfigured".into(),
            viewport_w: 0.0,
            viewport_h: 0.0,
        };
        let runner = InspectorRunner::new(
            dom.clone(),
            inspector_pane_view as fn(&InspectorState) -> InspectorView,
            state,
        );
        Self {
            dom,
            runner,
            scroll: crate::ui::PaneScroll::new(),
            layout: crate::ui::RetainedLayout::new(),
        }
    }

    /// Refresh from app truth at the pane's size.
    pub fn sync(
        &mut self,
        app: &App,
        pane_id: crate::panes::PaneId,
        pane_w: f32,
        pane_h: f32,
        clip_target: Option<&str>,
        clip_source_available: bool,
        clip_status: &str,
    ) {
        let sections = inspector_sections_for_pane(app, pane_id);
        let member = inspector_member(app, pane_id);
        let saved_viewer = member
            .and_then(|member| app.browser.get(member))
            .and_then(|browser| browser.viewer_override.as_deref());
        let viewer_options = crate::inspector_controls::viewer_options(app, saved_viewer);
        let synced_viewer =
            crate::inspector_controls::index_for_viewer(&viewer_options, saved_viewer);
        let capabilities = crate::inspector_controls::capabilities(app, member);
        self.runner.update(|state| {
            state.member = member;
            state.radio.selected = synced_viewer;
            state.radio.focus_request = None;
            state.viewer_options = viewer_options;
            state.capabilities = capabilities;
            state.sections = sections;
            state.clip_target = clip_target.map(str::to_string);
            state.clip_source_available = clip_source_available;
            state.clip_status = clip_status.to_string();
            state.viewport_w = pane_w;
            state.viewport_h = pane_h;
        });
    }

    /// The pane's scene at its size, under the host's cambium sheet.
    pub fn scene(&mut self, w: u32, h: u32) -> netrender::Scene {
        self.layout.scene_scrolled(
            &mut self.dom.borrow_mut(),
            crate::ui::CAMBIUM_SHEET,
            w,
            h,
            &mut self.scroll,
        )
    }

    /// Wheel delta from the shell.
    pub fn scroll_by(&mut self, dx: f32, dy: f32) {
        self.scroll.nudge(dx, dy);
    }

    /// Whether the overlay bars still need repainting as they fade.
    pub fn bars_visible(&mut self) -> bool {
        self.scroll.bars_visible()
    }

    /// Borrow the retained DOM for Genet Probe target resolution.
    pub fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.dom.borrow()
    }

    /// Resolve a scenario against the shaped fragments actually painted by
    /// this pane. The generic probe's estimated text layout can put its row
    /// centre on a different option, especially in a narrow Inspector.
    pub(crate) fn selector_point(
        &self,
        selector: &taproot::Selector,
    ) -> Result<Option<(f32, f32)>, &'static str> {
        if !selector.matches_surface("inspector") {
            return Ok(None);
        }
        let dom = self.dom.borrow();
        let mut point = None;
        for node in taproot::matching(&dom, selector) {
            let Some((x, y, width, height)) = self.layout.visible_rect(&dom, node) else {
                continue;
            };
            if width <= 0.0 || height <= 0.0 {
                continue;
            }
            if point.is_some() {
                return Err("multiple visible Inspector targets match the selector");
            }
            point = Some((x + width / 2.0, y + height / 2.0));
        }
        Ok(point)
    }

    pub fn click(&mut self, x: f32, y: f32, w: u32, h: u32) -> Vec<InspectorIntent> {
        let previous_viewer = self.runner.state().radio.selected;
        let hit = self.layout.hit_test_scrolled(
            &mut self.dom.borrow_mut(),
            crate::ui::CAMBIUM_SHEET,
            w,
            h,
            x,
            y,
            &self.scroll,
        );
        let mut intents = hit
            .map(|node| self.runner.dispatch_click(node, PointerClick::at((x, y))))
            .unwrap_or_default();
        let state = self.runner.state();
        if state.radio.selected != previous_viewer
            && let Some(member) = state.member
            && let Some(viewer) = crate::inspector_controls::viewer_for_index(
                &state.viewer_options,
                state.radio.selected,
            )
        {
            intents.push(InspectorIntent::SetViewer { member, viewer });
        }
        intents
    }
}

#[cfg(test)]
mod tests;
