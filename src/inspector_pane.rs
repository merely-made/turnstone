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
use layout_dom_api::LayoutDom;

use crate::app::App;
use crate::inspector_view::{InspectorSection, inspector_member, inspector_sections_for_pane};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InspectorIntent {
    ClipToKnot,
    CaptureSourceDocument {
        target: crate::source_capture::SourceCaptureTarget,
    },
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
    source_target: Option<crate::source_capture::SourceCaptureTarget>,
    source_status: String,
    source_unavailable: String,
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
    // Bind the offered source, rather than consulting the possibly retargeted
    // pane when a queued activation is finally delivered.
    let source_target = state.source_target.clone();
    let source_button = button(
        "Save source and keep",
        move |_state: &mut InspectorState, _click: PointerClick| {
            source_target
                .clone()
                .map(|target| InspectorIntent::CaptureSourceDocument { target })
        },
    )
    .attr(
        "class",
        if state.source_target.is_some() {
            "list-row action"
        } else {
            "list-row muted"
        },
    )
    .attr(
        "aria-disabled",
        if state.source_target.is_some() {
            "false"
        } else {
            "true"
        },
    );
    let source_status = el::<_, InspectorState, InspectorIntent>(
        "div",
        format!(
            "Source save: {}{}",
            state.source_status, state.source_unavailable
        ),
    )
    .attr("class", "detail-value")
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
                source_button,
                source_status,
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
    // Drop the runner and its widget resources before final mount custody.
    mount: Option<cambium::RetainedMount>,
}

impl InspectorPane {
    pub fn new() -> Self {
        Self::new_in(None)
    }

    pub(crate) fn new_in(mount: Option<cambium::RetainedMount>) -> Self {
        let dom: DomHandle = mount
            .as_ref()
            .map(cambium::RetainedMount::dom)
            .unwrap_or_else(|| Rc::new(RefCell::new(ScriptedDom::new())));
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
            source_target: None,
            source_status: "Unavailable: No source member".into(),
            source_unavailable: String::new(),
            viewport_w: 0.0,
            viewport_h: 0.0,
        };
        let runner = InspectorRunner::new_at(
            dom.clone(),
            mount
                .as_ref()
                .map_or_else(|| dom.borrow().document(), cambium::RetainedMount::node),
            inspector_pane_view as fn(&InspectorState) -> InspectorView,
            state,
        );
        Self {
            mount: mount.clone(),
            dom,
            runner,
            scroll: crate::ui::PaneScroll::new(),
            layout: crate::ui::RetainedLayout::new_at(
                mount.as_ref().map(cambium::RetainedMount::node),
            ),
        }
    }

    pub(crate) fn inline_focus(&self) -> Option<genet_scripted_dom::NodeId> {
        self.runner.focus()
    }
    pub(crate) fn focus_inline(&mut self, node: genet_scripted_dom::NodeId) -> bool {
        let Some(mount) = &self.mount else {
            return false;
        };
        if !cambium_rootstock::WindowDom::new(&self.dom.borrow(), mount.node()).contains(node) {
            return false;
        }
        self.runner.set_focus(Some(node));
        true
    }
    pub(crate) fn inline_selector(
        &self,
        selector: &taproot::Selector,
    ) -> Result<Option<(f32, f32)>, &'static str> {
        self.layout.selector_point(&self.dom.borrow(), selector)
    }
    pub(crate) fn key_inline(&mut self, event: cambium::KeyEvent) -> Vec<InspectorIntent> {
        let previous = self.runner.state().radio.selected;
        let mut intents = self.runner.dispatch_key(event);
        let state = self.runner.state();
        if state.radio.selected != previous
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
    pub(crate) fn blur_inline(&mut self) {
        self.runner.set_focus(None);
    }
    pub(crate) fn inline_accepts_text(&self) -> bool {
        self.layout
            .editable_focus(&self.dom.borrow(), self.runner.focus())
    }
    pub(crate) fn inline_cursor_area(&self) -> Option<(f32, f32, f32, f32)> {
        self.layout
            .cursor_area(&self.dom.borrow(), self.runner.focus())
    }
    pub(crate) fn inline_mount(&self) -> Option<cambium::RetainedMount> {
        self.mount.clone()
    }
    pub(crate) fn unbind_inline(&mut self) {
        self.layout.unbind_inline();
    }
    pub(crate) fn bind_inline(&mut self, layout: &crate::ui::InlineLayoutHandle) {
        if let Some(mount) = &self.mount {
            self.layout.bind_inline(layout, mount);
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
        let source_target = member.and_then(|member| app.source_capture_target(member).ok());
        let source_status = app.source_capture_status(member);
        let source_unavailable = member
            .and_then(|member| app.source_capture_target(member).err())
            .filter(|_| !source_status.starts_with("Unavailable:"))
            .map(|reason| format!("; Save source unavailable: {reason}"))
            .unwrap_or_default();
        self.runner.update(|state| {
            state.member = member;
            state.radio.selected = synced_viewer;
            state.radio.focus_request = None;
            state.viewer_options = viewer_options;
            state.capabilities = capabilities;
            state.source_target = source_target;
            state.source_status = source_status;
            state.source_unavailable = source_unavailable;
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
        if !self.layout.has_current_snapshot(&dom) {
            return Err("retained Inspector snapshot is unavailable or stale");
        }
        let mut point = None;
        for node in self.layout.matching_nodes(&dom, selector) {
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
