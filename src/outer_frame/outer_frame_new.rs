// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl OuterFrame {
    pub(crate) fn new(dom: cambium::DomHandle) -> Self {
        use layout_dom_api::LayoutDomMut;
        let scope = dom.borrow_mut().create_element(cambium::html_qual("div"));
        let root = dom.borrow_mut().create_element(cambium::html_qual("div"));
        dom.borrow_mut()
            .set_attribute(root, cambium::attr_qual("class"), "turnstone-window-root");
        dom.borrow_mut().set_attribute(
            root,
            cambium::attr_qual("style"),
            "position:relative;width:1px;height:1px;box-sizing:border-box;",
        );
        let document = dom.borrow().document();
        dom.borrow_mut().append_child(document, scope);
        dom.borrow_mut().append_child(scope, root);
        let workspace_root = dom.borrow_mut().create_element(cambium::html_qual("div"));
        dom.borrow_mut().append_child(root, workspace_root);
        let controls = FrameControls::new_at(dom, workspace_root);
        let workspace = Workspace::default();
        let appearance = crate::shell_services::AppearanceConfig::default();
        let sheet = crate::frame_style::stylesheet(
            None,
            &appearance.tabard_mode(),
            &appearance.theme_typography,
            "",
            appearance.zoom(),
        );
        let sheet = format!("{}\n{}", crate::ui::cambium_sheet(&appearance), sheet);
        Self {
            window_scope: scope,
            window_root: root,
            workspace_root,
            workspace_area: None,
            main_view_switch: true,
            scrolls: Vec::new(),
            body_focus: None,
            controls,
            workspace,
            current: None,
            size: (1, 1),
            scale: 1.0,
            laid_scale: 1.0,
            sheet,
            appearance,
            layout: std::rc::Rc::new(std::cell::RefCell::new(crate::ui::RetainedLayout::new_at(
                Some(scope),
            ))),
        }
    }
}
