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
    AnyView, DetailRow, DetailSection, DomHandle, GenetCtx, GenetElement, PointerClick, RadioGroup,
    RadioOption, button, detail_panel, el, lens, map_message_result, radio_options,
};
use genet_scripted_dom::ScriptedDom;
use layout_dom_api::LayoutDom;

use crate::app::App;
use crate::inspector_view::{InspectorSection, inspector_member, inspector_sections_for_pane};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InspectorIntent {
    Binding(crate::pane_binding::BindingIntent),
    ClipToKnot {
        offer: uuid::Uuid,
    },
    NewResearchNote,
    OpenResearchNote,
    ChooseResearchNote {
        choice: crate::app::research_note_arms::NoteChoice,
    },
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
    binding: crate::pane_binding::BindingViewState,
    sections: Vec<InspectorSection>,
    member: Option<uuid::Uuid>,
    radio: RadioGroup,
    viewer_options: Vec<crate::inspector_controls::ViewerOption>,
    capabilities: Vec<String>,
    clip_target: Option<String>,
    clip_offer: Option<uuid::Uuid>,
    clip_status: String,
    saved_passage: bool,
    note_choice: Option<crate::app::research_note_arms::NoteChoice>,
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

/// The viewer options as Cambium radio options: unselectable engines stay
/// listed (and keep a saved selection) but are disabled.
fn viewer_radio_options(
    options: &[crate::inspector_controls::ViewerOption],
) -> Vec<RadioOption> {
    options
        .iter()
        .map(|option| {
            RadioOption::new(option.label.clone())
                .disabled(!option.selectable)
                .with_attr(
                    "data-engine-id",
                    option.viewer.clone().unwrap_or_else(|| "auto".into()),
                )
        })
        .collect()
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
    let clip_label = match (&state.clip_target, state.clip_offer.is_some()) {
        (Some(target), true) => format!(
            "Clip {} to {target}",
            if state.saved_passage {
                "saved passage"
            } else {
                "document"
            }
        ),
        (None, _) => "Create or choose a research note first".into(),
        (Some(_), false) => "Clip unavailable; see status below".into(),
    };
    let clip_status = el::<_, InspectorState, InspectorIntent>(
        "div",
        format!("Passage: {}", state.clip_status),
    )
    .attr("class", "detail-value");
    let offered_clip = state.clip_offer;
    let clip_button = button(
        clip_label,
        move |state: &mut InspectorState, _click: PointerClick| {
            state
                .clip_target
                .as_ref()
                .and(offered_clip)
                .map(|offer| InspectorIntent::ClipToKnot { offer })
        },
    )
    .attr(
        "class",
        if state.clip_target.is_some() && state.clip_offer.is_some() {
            "list-row action"
        } else {
            "list-row muted"
        },
    )
    .attr(
        "aria-disabled",
        if state.clip_target.is_some() && state.clip_offer.is_some() {
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
            move |radio: &mut RadioGroup| radio_options(radio, &viewer_radio_options(&viewer_options)),
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
                crate::pane_binding::binding_view(&state.binding, |s: &mut InspectorState| &mut s.binding, InspectorIntent::Binding),
                el::<_, InspectorState, InspectorIntent>("div", "Document controls")
                    .attr("class", "list-section-title"),
                el::<_, InspectorState, InspectorIntent>(
                    "div",
                    format!(
                        "Research note: {}",
                        state
                            .clip_target
                            .as_deref()
                            .unwrap_or("choose or create a note")
                    ),
                )
                .attr("class", "detail-value")
                .attr("style", "overflow-wrap: anywhere;"),
                el(
                    "div",
                    (
                        button("New research note", |_: &mut InspectorState, _| {
                            InspectorIntent::NewResearchNote
                        }),
                        button("Open research note", |state: &mut InspectorState, _| {
                            state
                                .clip_target
                                .as_ref()
                                .map(|_| InspectorIntent::OpenResearchNote)
                        }),
                        {
                            let choice = state.note_choice.clone();
                            button(
                                choice
                                    .as_ref()
                                    .map(|choice| choice.use_label())
                                    .unwrap_or_else(|| "Use this document as research note".into()),
                                move |_: &mut InspectorState, _| {
                                    choice.clone().map(|choice| {
                                        InspectorIntent::ChooseResearchNote { choice }
                                    })
                                },
                            )
                            .attr(
                                "style",
                                if state.note_choice.is_some() {
                                    ""
                                } else {
                                    "display: none;"
                                },
                            )
                        },
                    ),
                )
                .attr("style", "display: flex; flex-wrap: wrap; gap: 4px;"),
                clip_status,
                clip_button,
                source_button,
                source_status,
                capabilities,
                el::<_, InspectorState, InspectorIntent>("div", "Viewer")
                    .attr("class", "list-section-title"),
                viewer,
                detail_panel(&sections),
            ),
        )
        .attr("class", "pane cambium-shell-panel")
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
    admission: uuid::Uuid,
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
    pub(crate) fn admission(&self) -> uuid::Uuid {
        self.admission
    }

    pub fn new() -> Self {
        Self::new_in(None)
    }

    pub(crate) fn new_in(mount: Option<cambium::RetainedMount>) -> Self {
        let dom: DomHandle = mount
            .as_ref()
            .map(cambium::RetainedMount::dom)
            .unwrap_or_else(|| Rc::new(RefCell::new(ScriptedDom::new())));
        let state = InspectorState {
            binding: Default::default(),
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
            clip_offer: None,
            note_choice: None,
            clip_status: "unconfigured".into(),
            saved_passage: false,
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
            admission: uuid::Uuid::new_v4(),
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
        if matches!(event.key, cambium::Key::Named(cambium::NamedKey::Escape)) && self.runner.state().binding.expanded {
            self.runner.update(|state| state.binding.expanded = false);
            return Vec::new();
        }
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
        clip_offer: Option<uuid::Uuid>,
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
        let note_choice = member
            .and_then(|member| app.note_choice(member))
            .filter(|choice| {
                app.content
                    .facts(choice.member)
                    .is_some_and(|f| f.engine == crate::knot_authoring::ENGINE_ID)
                    && app
                        .knot_documents()
                        .iter()
                        .any(|d| d.address == choice.address && d.status == "local")
            });
        let source_target = member.and_then(|member| app.source_capture_target(member).ok());
        let source_status = app.source_capture_status(member);
        let source_unavailable = member
            .and_then(|member| app.source_capture_target(member).err())
            .filter(|_| !source_status.starts_with("Unavailable:"))
            .map(|reason| format!("; Save source unavailable: {reason}"))
            .unwrap_or_default();
        self.runner.update(|state| {
            state.binding.sync(app, pane_id);
            state.member = member;
            state.radio.selected = synced_viewer;
            state.radio.focus_request = None;
            state.viewer_options = viewer_options;
            state.capabilities = capabilities;
            state.note_choice = note_choice;
            state.source_target = source_target;
            state.source_status = source_status;
            state.source_unavailable = source_unavailable;
            state.sections = sections;
            state.clip_target = clip_target.map(str::to_string);
            state.clip_offer = clip_offer;
            state.clip_status = clip_status.to_string();
            state.saved_passage = member
                .and_then(|m| app.content.facts(m))
                .is_some_and(|f| f.engine == crate::saved_sources::ENGINE_ID);
            state.viewport_w = pane_w;
            state.viewport_h = pane_h;
        });
    }

    /// The pane's scene at its size, under the host's cambium sheet.
    pub fn scene(&mut self, w: u32, h: u32, scale: f32) -> netrender::Scene {
        self.layout.set_scale(scale);
        self.layout.scene_scrolled(
            &mut self.dom.borrow_mut(),
            crate::ui::cambium_shell_sheet(),
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
            crate::ui::cambium_shell_sheet(),
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
