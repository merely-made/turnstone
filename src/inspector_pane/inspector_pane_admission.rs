// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
            viewer_expanded: false,
            research_expanded: false,
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
        if matches!(event.key, cambium::Key::Named(cambium::NamedKey::Escape))
            && self.runner.state().binding.expanded
        {
            self.runner.update(|state| state.binding.expanded = false);
            return Vec::new();
        }
        if matches!(event.key, cambium::Key::Named(cambium::NamedKey::Escape))
            && self.runner.state().viewer_expanded
        {
            self.runner.update(|state| state.viewer_expanded = false);
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
        let viewer_options: Vec<_> = crate::inspector_controls::viewer_options(app, saved_viewer)
            .into_iter()
            .filter(|option| option.selectable || option.viewer.as_deref() == saved_viewer)
            .collect();
        let synced_viewer =
            crate::inspector_controls::index_for_viewer(&viewer_options, saved_viewer);
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
            if state.member != member {
                state.viewer_expanded = false;
                state.research_expanded = false;
            }
            state.member = member;
            state.radio.selected = synced_viewer;
            state.radio.focus_request = None;
            state.viewer_options = viewer_options;
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
