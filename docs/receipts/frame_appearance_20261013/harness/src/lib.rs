// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

// Actual consumer stylesheet code; actual shared Workspace DOM and Livery layout.
#[path = "../../../../../src/frame_style.rs"]
mod frame_style;
#[path = "../../../../../src/panes/workspace.rs"]
mod workspace;
#[path = "../../../../../src/panes/workspace_store.rs"]
mod workspace_store;

#[cfg(test)]
mod tests {
    use super::frame_style;
    use cambium::{
        AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, Slot, WorkspaceModel,
    };
    use genet_livery::{Device, InteractionStates, LiveryLayout, StylePlane, StyleSet, TextSystem};
    use genet_scripted_dom::{NodeId, ScriptedDom};
    use layout_dom_api::{LayoutDom, LocalName, Namespace};
    use std::{cell::RefCell, rc::Rc};
    use tabard::theme::{
        choice::ThemeChoice,
        registry::{Mode, ThemeRegistry},
    };
    use tabard::{FontRole, ThemeFontStyle, ThemePresentation, Typography};
    use workbench::{ContentSource, Tile, TileId, TileTree, Workspace};

    #[test]
    fn native_scenarios_parse_through_the_published_driver() {
        for source in [
            include_str!("../../../../../scenarios/frame_fonts_workshop.scn"),
            include_str!("../../../../../scenarios/frame_appearance.scn"),
            include_str!("../../../../../scenarios/frame_appearance_reopen.scn"),
            include_str!("../../../../../scenarios/frame_fonts_missing_workshop.scn"),
            include_str!("../../../../../scenarios/frame_appearance_missing.scn"),
            include_str!("../../../../../scenarios/frame_appearance_missing_reopen.scn"),
        ] {
            taproot::Scenario::parse(source).unwrap();
        }
    }
    #[test]
    fn application_family_overrides_generated_theme_defaults_on_authored_mode() {
        let (dom, _) = fixture();
        let dom = dom.borrow();
        let typography = fonts(42.0);
        let presentation = ThemePresentation::AuthoredStylesheet(vec![
            typography.css_custom_properties(),
            ":root{--tabard-color-bg:rgb(1,2,3)}".into(),
        ]);
        let requests = serde_json::from_str(r#"{"interface":["monospace"]}"#).unwrap();
        let properties = frame_style::interface_font_properties(&requests, &[]);
        let (styles, _) = measure(
            &dom,
            &frame_style::stylesheet(Some(&presentation), &typography, &properties, 1.0),
            800.0,
        );
        let label = node(&dom, "frisket-label");
        assert_eq!(
            styles.computed_style(label, "font-family").as_deref(),
            Some("monospace")
        );
        assert_eq!(
            styles.computed_style(label, "font-size").as_deref(),
            Some("42px")
        );
    }

    type View = Box<dyn AnyView<Workspace, (), GenetCtx, GenetElement>>;
    fn view(w: &Workspace) -> View {
        Box::new(cambium::workspace_view(
            &WorkspaceModel {
                workspace: w,
                current: Some(TileId(1)),
                float_layer_visible: true,
            },
            |_: &mut Workspace, _| {},
            |_| Slot::Surface,
        ))
    }
    fn fixture() -> (DomHandle, Workspace) {
        let dom = Rc::new(RefCell::new(ScriptedDom::new()));
        let tile = Tile {
            id: TileId(1),
            content: ContentSource::Open {
                kind: "test".into(),
                id: "1".into(),
            },
            title: "A wide title WWW and narrow iii".into(),
            accent: None,
        };
        let workspace = Workspace::new(TileTree::single(tile));
        let _runner = GenetAppRunner::<_, _, View, ()>::new(
            dom.clone(),
            view as fn(&Workspace) -> View,
            workspace.clone(),
        );
        (dom, workspace)
    }
    fn node(dom: &ScriptedDom, class: &str) -> NodeId {
        fn visit(d: &ScriptedDom, n: NodeId, class: &str) -> Option<NodeId> {
            if d.attribute(n, &Namespace::default(), &LocalName::from("class"))
                .is_some_and(|c| c.split_whitespace().any(|c| c == class))
            {
                return Some(n);
            }
            d.dom_children(n)
                .into_iter()
                .find_map(|c| visit(d, c, class))
        }
        visit(dom, dom.document(), class).expect(class)
    }
    fn measure(
        dom: &ScriptedDom,
        sheet: &str,
        width: f32,
    ) -> (StylePlane<NodeId>, LiveryLayout<NodeId>) {
        let device = Device::screen(width, 600.0);
        let styles = genet_livery::resolve_styles(
            dom,
            &StyleSet::cambium(&[sheet]),
            &device,
            &InteractionStates::default(),
        );
        genet_livery::layout_with_text_system(
            dom,
            &styles,
            width,
            600.0,
            device.viewport_sizes,
            &mut TextSystem::new(),
            &Default::default(),
        )
        .unwrap()
    }
    fn fonts(size: f32) -> Typography {
        let mut fonts = Typography::default();
        fonts.0.insert(
            FontRole::Interface,
            ThemeFontStyle {
                family: Some("serif".into()),
                size_px: Some(size),
                line_height: Some(1.5),
            },
        );
        fonts
    }
    fn sheet(fonts: &Typography) -> String {
        frame_style::stylesheet(None, fonts, "", 1.0)
    }

    #[test]
    fn authored_fonts_move_body_aperture_without_changing_workspace_or_dom_identity() {
        let (dom, workspace) = fixture();
        let dom = dom.borrow();
        let saved = serde_json::to_string(&workspace).unwrap();
        let label = node(&dom, "frisket-label");
        let body = node(&dom, "frisket-content");
        let header = node(&dom, "frisket-tabbar");
        let (_, small) = measure(&dom, &sheet(&Typography::default()), 800.0);
        let (style, big) = measure(&dom, &sheet(&fonts(42.0)), 800.0);
        assert_eq!(
            style.computed_style(label, "font-size").as_deref(),
            Some("42px")
        );
        assert!(
            style
                .computed_style(label, "font-family")
                .unwrap()
                .contains("serif")
        );
        assert!(big.get(header).unwrap().height > small.get(header).unwrap().height);
        assert!(big.get(body).unwrap().y > small.get(body).unwrap().y);
        assert!(big.get(body).unwrap().height < small.get(body).unwrap().height);
        assert_eq!(label, node(&dom, "frisket-label"));
        assert_eq!(saved, serde_json::to_string(&workspace).unwrap());
    }
    #[test]
    fn removing_font_override_restores_geometry_and_zoom_changes_host_defaults() {
        let (dom, _) = fixture();
        let dom = dom.borrow();
        let header = node(&dom, "frisket-tabbar");
        let (_, initial) = measure(&dom, &sheet(&Typography::default()), 800.0);
        let (_, big) = measure(&dom, &sheet(&fonts(48.0)), 800.0);
        let (_, reset) = measure(&dom, &sheet(&Typography::default()), 800.0);
        assert_eq!(
            initial.get(header).unwrap().height,
            reset.get(header).unwrap().height
        );
        assert!(big.get(header).unwrap().height > reset.get(header).unwrap().height);
        let (_, zoomed) = measure(
            &dom,
            &frame_style::stylesheet(None, &Typography::default(), "", 2.0),
            800.0,
        );
        assert!(zoomed.get(header).unwrap().height > initial.get(header).unwrap().height);
    }
    #[test]
    fn narrow_large_type_retains_body_and_close_apertures() {
        let (dom, _) = fixture();
        let dom = dom.borrow();
        let (_, layout) = measure(&dom, &sheet(&fonts(48.0)), 240.0);
        let close = layout.get(node(&dom, "frisket-close")).unwrap();
        let body = layout.get(node(&dom, "frisket-content")).unwrap();
        assert!(close.width > 0.0 && close.height > 0.0 && close.x + close.width <= 240.1);
        assert!(body.width > 0.0 && body.height > 0.0 && body.y + body.height <= 600.1);
    }
    #[test]
    fn exact_authored_css_wins_over_roles_and_application_family() {
        let (dom, _) = fixture();
        let dom = dom.borrow();
        let label = node(&dom, "frisket-label");
        let css = ThemePresentation::AuthoredStylesheet(vec![
            ".frisket-label{font-family:monospace;font-size:31px;color:rgb(1,2,3)}".into(),
        ]);
        let application = serde_json::from_str(r#"{"interface":["sans-serif"]}"#).unwrap();
        let props = frame_style::interface_font_properties(&application, &[]);
        let (styles, _) = measure(
            &dom,
            &frame_style::stylesheet(Some(&css), &fonts(42.0), &props, 1.0),
            800.0,
        );
        assert_eq!(
            styles.computed_style(label, "font-size").as_deref(),
            Some("31px")
        );
        assert_eq!(
            styles.computed_style(label, "font-family").as_deref(),
            Some("monospace")
        );
        assert_eq!(
            styles.computed_style(label, "color").as_deref(),
            Some("rgb(1, 2, 3)")
        );
    }
    #[test]
    fn selected_palette_reaches_shared_furniture() {
        let (dom, _) = fixture();
        let dom = dom.borrow();
        let label = node(&dom, "frisket-label");
        let color = |mode| {
            let theme = tabard::resolve_theme_choice(
                &ThemeRegistry::default(),
                &ThemeChoice::new("theme:default", Some(mode)),
            )
            .unwrap();
            measure(
                &dom,
                &frame_style::stylesheet(
                    Some(&theme.presentation),
                    &Typography::default(),
                    "",
                    1.0,
                ),
                800.0,
            )
            .0
            .computed_style(label, "color")
            .unwrap()
        };
        assert_ne!(color(Mode::Light), color(Mode::Dark));
    }
    #[test]
    fn unrelated_roles_do_not_restyle_frame_and_omissions_keep_defaults() {
        let (dom, _) = fixture();
        let dom = dom.borrow();
        let label = node(&dom, "frisket-label");
        let mut typography = Typography::default();
        typography.0.insert(
            FontRole::Reading,
            ThemeFontStyle {
                family: Some("monospace".into()),
                size_px: Some(80.0),
                line_height: Some(3.0),
            },
        );
        let (styles, _) = measure(&dom, &sheet(&typography), 800.0);
        assert_eq!(
            styles.computed_style(label, "font-size").as_deref(),
            Some("15px")
        );
        typography.0.insert(
            FontRole::Interface,
            ThemeFontStyle {
                family: Some("serif".into()),
                ..Default::default()
            },
        );
        let (styles, _) = measure(&dom, &sheet(&typography), 800.0);
        assert_eq!(
            styles.computed_style(label, "font-size").as_deref(),
            Some("15px")
        );
    }
    #[test]
    fn missing_font_requests_survive_resolution_and_persistent_reopening() {
        use tabard::theme::choice::{FileThemeChoiceStore, ThemeChoiceStore};
        let path =
            std::env::temp_dir().join(format!("turnstone-frame-fonts-{}", std::process::id()));
        let mut store = FileThemeChoiceStore::load_strict(&path).unwrap();
        let mut choice = ThemeChoice::default();
        choice.fonts =
            serde_json::from_str(r#"{"interface":["Absent on this host"],"reading":["serif"]}"#)
                .unwrap();
        let props = frame_style::interface_font_properties(&choice.fonts, &[]);
        assert!(props.contains("--tabard-font-interface: sans-serif"));
        assert!(!props.contains("--tabard-font-reading"));
        store.set_choice(choice.clone()).unwrap();
        assert_eq!(
            FileThemeChoiceStore::load_strict(&path).unwrap().choice(),
            &choice
        );
        let present =
            frame_style::interface_font_properties(&choice.fonts, &["Absent on this host".into()]);
        assert!(present.contains("\"Absent on this host\", sans-serif"));
        std::fs::remove_file(path).unwrap();
    }
}
