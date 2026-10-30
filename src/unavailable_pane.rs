// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use genet_scripted_dom::{NodeId, ScriptedDom};
use std::{cell::Ref, marker::PhantomData, sync::Arc};
pub(crate) struct UnavailablePane<S> {
    pane: crate::inline_pane::InlinePane,
    marker: PhantomData<S>,
}
impl<S> UnavailablePane<S> {
    pub fn new(service: Option<Arc<S>>) -> Self {
        Self::new_in(None, service)
    }
    pub fn new_in(mount: Option<cambium::RetainedMount>, _: Option<Arc<S>>) -> Self {
        let mount = mount.unwrap_or_else(|| {
            cambium::RetainedMount::new(Rc::new(RefCell::new(ScriptedDom::new())))
        });
        Self {
            pane: crate::inline_pane::InlinePane::new_in(mount),
            marker: PhantomData,
        }
    }
    pub fn sync(&mut self, w: f32, h: f32) {
        self.pane.sync(
            "Private sharing requires a build with the peers feature.".into(),
            w as u32,
            h as u32,
        );
    }
    pub fn scene(&mut self, w: u32, h: u32, scale: f32) -> netrender::Scene {
        self.pane.scene(w, h, scale)
    }
    pub fn inline_focus(&self) -> Option<NodeId> {
        self.pane.inline_focus()
    }
    pub fn focus_inline(&mut self, n: NodeId) -> bool {
        self.pane.focus_inline(n)
    }
    pub fn inline_selector(
        &self,
        s: &taproot::Selector,
    ) -> Result<Option<(f32, f32)>, &'static str> {
        self.pane.inline_selector(s)
    }
    pub fn key_inline(&mut self, e: cambium::KeyEvent) {
        self.pane.key_inline(e);
    }
    pub fn blur_inline(&mut self) {
        self.pane.blur_inline()
    }
    pub fn inline_accepts_text(&self) -> bool {
        false
    }
    pub fn inline_cursor_area(&self) -> Option<(f32, f32, f32, f32)> {
        None
    }
    pub fn inline_mount(&self) -> Option<cambium::RetainedMount> {
        self.pane.inline_mount()
    }
    pub fn unbind_inline(&mut self) {
        self.pane.unbind_inline()
    }
    pub fn bind_inline(&mut self, l: &crate::ui::InlineLayoutHandle) {
        self.pane.bind_inline(l)
    }
    pub fn scroll_by(&mut self, _: f32, _: f32) {}
    pub fn bars_visible(&mut self) -> bool {
        false
    }
    pub fn click(&mut self, _: f32, _: f32, _: u32, _: u32) {}
    pub fn dom_ref(&self) -> Ref<'_, ScriptedDom> {
        self.pane.dom_ref()
    }
}
use std::{cell::RefCell, rc::Rc};

#[cfg(all(test, not(feature = "peers")))]
mod tests {
    use super::*;
    #[test]
    fn unavailable_sharing_keeps_the_saved_panes_mount_and_refuses_text_focus() {
        let dom = Rc::new(RefCell::new(ScriptedDom::new()));
        let mount = cambium::RetainedMount::new(dom);
        let node = mount.node();
        let mut pane = crate::publish_pane::PublishPane::new_in(Some(mount.clone()), None);
        pane.sync(320.0, 200.0);
        let _ = pane.scene(320, 200, 1.0);
        assert_eq!(pane.inline_mount().unwrap().node(), node);
        assert!(!pane.inline_accepts_text());
        assert!(!pane.focus_inline(node));
        assert!(pane.inline_cursor_area().is_none());
    }
}

/// Saved contributed-pane identities remain readable in a reduced build.
pub(crate) fn contributed_label(kind: &crate::panes::PaneKindId) -> Option<&'static str> {
    match kind.as_str() {
        "turnstone.answers" if !cfg!(feature = "inference") => {
            Some("Answers is not included in this build.")
        },
        "turnstone.distillery-installed" if !cfg!(feature = "inference") => {
            Some("Distillery is not included in this build.")
        },
        "turnstone.sky" if !cfg!(feature = "sky") => Some("Sky is not included in this build."),
        _ => None,
    }
}
