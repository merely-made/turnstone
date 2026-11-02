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

struct InspectorState {
    binding: crate::pane_binding::BindingViewState,
    sections: Vec<InspectorSection>,
    member: Option<uuid::Uuid>,
    radio: RadioGroup,
    viewer_options: Vec<crate::inspector_controls::ViewerOption>,
    viewer_expanded: bool,
    research_expanded: bool,
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

#[cfg(test)]
mod tests;

#[path = "inspector_pane/impl_cambium_action_for_inspector_intent.rs"]
mod impl_cambium_action_for_inspector_intent;
#[path = "inspector_pane/viewer_radio_options.rs"]
mod viewer_radio_options;
use viewer_radio_options::viewer_radio_options;
#[path = "inspector_pane/inspector_pane_view.rs"]
mod inspector_pane_view;
use inspector_pane_view::inspector_pane_view;
#[path = "inspector_pane/inspector_pane_admission.rs"]
mod inspector_pane_admission;
