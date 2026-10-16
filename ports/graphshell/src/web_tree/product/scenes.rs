// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Saved scenes and transfers use the same MereHost facets and codicil operations.
use super::*;
use cambium::{button, checkbox, el, lens, select, textarea_typed};
use graphshell::local_scene::{capture_scene, restore_scene};
use graphshell::product::{ExportRequest, SavedSceneV3, TransferScope};
const ADDRESS: &str = "mere://scene/graphshell-h3";
const SCOPES: [(TransferScope, &str); 4] = [
    (TransferScope::SelectedSubgraph, "Selected subgraph"),
    (TransferScope::ObjectOnly, "Object only"),
    (TransferScope::DirectRelations, "Direct relations"),
    (TransferScope::SavedScene, "Saved scene"),
];

pub(super) struct SceneControls {
    pub(super) open: bool,
    pub(super) codicil: TextInput,
    scope: SelectState,
    include_local_file: bool,
    saved: Option<SavedSceneV3>,
    export_bytes: usize,
    imported_nodes: usize,
}
impl Default for SceneControls {
    fn default() -> Self {
        Self {
            open: false,
            codicil: TextInput::new(""),
            scope: SelectState::new(0).with_label("Transfer scope"),
            include_local_file: false,
            saved: None,
            export_bytes: 0,
            imported_nodes: 0,
        }
    }
}
impl SavedProduct {
    pub(in crate::web_tree) fn saved_scene_facet(&self) -> &'static str {
        use graphshell::product::{SAVED_SCENE_FACET, SAVED_SCENE_FACET_V2, SAVED_SCENE_FACET_V3};
        [
            (SAVED_SCENE_FACET, "v4"),
            (SAVED_SCENE_FACET_V3, "v3"),
            (SAVED_SCENE_FACET_V2, "v2"),
        ]
        .into_iter()
        .find(|(facet, _)| {
            self.app
                .as_ref()
                .is_some_and(|app| app.host.facet_value(ADDRESS, facet).is_some())
        })
        .map_or("none", |(_, version)| version)
    }
    pub(in crate::web_tree) fn export_bytes(&self) -> usize {
        self.scenes.export_bytes
    }
    pub(in crate::web_tree) fn imported_nodes(&self) -> usize {
        self.scenes.imported_nodes
    }

    /// Preserve the ordinary command path without inventing UI observation context.
    pub(super) fn scene_action(
        &mut self,
        canvas: &mut Canvas,
        layout: &str,
        viewport: (u32, u32),
        action: &str,
    ) -> Result<(), String> {
        self.scene_action_observed(canvas, layout, viewport, action, None)
    }

    pub(super) fn scene_action_observed(
        &mut self,
        canvas: &mut Canvas,
        layout: &str,
        viewport: (u32, u32),
        action: &str,
        waypoint: Option<super::super::owner_waypoint::OpenContext>,
    ) -> Result<(), String> {
        if self.selection_locked() {
            return Err("A save is still pending".into());
        }
        let handler = self.selected_handler();
        let app = self.app.as_mut().ok_or("Local graph is busy")?;
        match action {
            "save" => {
                let scene = capture_scene(canvas, layout, handler)?;
                app.host
                    .save_product_scene(ADDRESS, &scene)
                    .map_err(|e| e.to_string())?;
                self.forme_reconcile_session = Some(app.host.graph_session().id().0);
                restore_scene(canvas, app.host.graph(), &scene, viewport)?;
                self.scenes.saved = Some(scene);
                self.operation_success = Some("Scene saved".into());
                self.persist_operation();
            },
            "reopen" => {
                let scene = app.host.product_scene(ADDRESS).map_err(|e| e.to_string())?;
                self.forme_reconcile_session = Some(app.host.graph_session().id().0);
                restore_scene(canvas, app.host.graph(), &scene, viewport)?;
                self.scenes.saved = Some(scene);
                let handler = self
                    .scenes
                    .saved
                    .as_ref()
                    .map(|scene| scene.default_handler.clone())
                    .unwrap_or_default();
                self.restore_handler(&handler);
                self.select(canvas.selected_members().first().copied());
                self.status = "Saved scene reopened".into();
            },
            "export" => {
                let focused = self
                    .selected
                    .or_else(|| canvas.selected_members().first().copied())
                    .ok_or("Select an object first")?;
                let scope = SCOPES[self.scenes.scope.selected.min(SCOPES.len() - 1)].0;
                let scene = self
                    .scenes
                    .saved
                    .clone()
                    .or_else(|| app.host.product_scene(ADDRESS).ok());
                if scope == TransferScope::SavedScene && scene.is_none() {
                    return Err("Save a scene before exporting scene scope".into());
                }
                let bytes = app
                    .host
                    .export_product_codicil(ExportRequest {
                        focused,
                        selected: canvas.selected_members(),
                        scope,
                        exported_at_ms: js_sys::Date::now() as u64,
                        include_local_file_locations: self.scenes.include_local_file,
                        scene,
                    })
                    .map_err(|e| e.to_string())?;
                self.scenes.export_bytes = bytes.len();
                self.scenes.codicil =
                    TextInput::new(String::from_utf8(bytes).map_err(|e| e.to_string())?);
                self.status = format!("Exported {} bytes", self.scenes.export_bytes);
            },
            "open" => {
                let previous_session = app.host.graph_session().id().0;
                super::super::owner_waypoint::pre_open(waypoint.as_ref(), previous_session);
                let accepted = graphshell::local_scene::open_codicil(
                    &mut app.host,
                    canvas,
                    self.scenes.codicil.text().as_bytes(),
                    viewport,
                )?;
                // Acceptance may precede a view error; count actual owner truth.
                self.observed_session_changes += u64::from(previous_session != accepted.owner);
                self.session = accepted.owner.to_string();
                self.forme_new_session = Some(accepted.owner);
                self.scenes.imported_nodes = accepted.receipt.nodes;
                if let Some(error) = accepted.view_error {
                    return Err(error);
                }
                if let Some(scene) = accepted.scene {
                    self.scenes.saved = Some(scene);
                }
                self.detail_open = false;
                self.operation_success = Some("Opened codicil".into());
                self.persist_operation();
            },
            _ => return Err("Unknown scene operation".into()),
        }
        Ok(())
    }
}

pub(in crate::web_tree) fn section(page: &TreePage) -> Child {
    let Some(product) = &page.product else {
        return Box::new(el("div", ()));
    };
    let mut children: Vec<Child> = vec![Box::new(
        button("Scenes and transfers", |page: &mut TreePage, _| {
            if let Some(product) = &mut page.product {
                product.scenes.open = !product.scenes.open;
            }
        })
        .attr("aria-expanded", product.scenes.open.to_string()),
    )];
    if product.scenes.open {
        for (label, action) in [("Save scene", "save"), ("Reopen scene", "reopen")] {
            children.push(action_button(label, action));
        }
        children.push(Box::new(lens(
            |state: &mut SelectState| select(state, &SCOPES.map(|(_, label)| label)),
            |page: &mut TreePage| &mut page.product.as_mut().expect("scene").scenes.scope,
        )));
        children.push(Box::new(lens(
            |checked: &mut bool| {
                checkbox(*checked).attr("aria-label", "Include local file locations")
            },
            |page: &mut TreePage| {
                &mut page
                    .product
                    .as_mut()
                    .expect("scene")
                    .scenes
                    .include_local_file
            },
        )));
        children.push(action_button("Export codicil", "export"));
        children.push(Box::new(lens(
            |input: &mut TextInput| textarea_typed(input).attr("aria-label", "Codicil"),
            |page: &mut TreePage| &mut page.product.as_mut().expect("scene").scenes.codicil,
        )));
        children.push(action_button("Open codicil", "open"));
    }
    Box::new(el("section", children).attr("aria-label", "Scenes and transfers"))
}
fn action_button(label: &'static str, action: &'static str) -> Child {
    Box::new(button(label, move |page: &mut TreePage, _| {
        let waypoint = if action == "open" {
            super::super::owner_waypoint::context(page)
        } else {
            None
        };
        if let Some(product) = &mut page.product {
            if let Err(error) = product.scene_action_observed(
                &mut page.shared.canvas.borrow_mut(),
                &page.physics.layout_id,
                page.size,
                action,
                waypoint,
            ) {
                product.status = error;
            }
        }
        let canvas = page.shared.canvas.borrow();
        page.physics.sync(&canvas);
        if let Some(product) = &page.product {
            if let Some(scene) = &product.scenes.saved {
                if let Ok(layout) = scene.arrangement() {
                    page.physics.arrangement.selected =
                        mere::canvas::physics_controls::arrangement_choices()
                            .position(|(id, _)| id == layout)
                            .unwrap_or(0);
                    page.physics.layout_id = layout;
                }
            }
        }
        page.shared.dirty.set(true);
    }))
}
