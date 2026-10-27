// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Bind the local owner's detail operations to shared Cambium controls.
use super::*;
use cambium::{button, el, lens, select, text_field_typed, textarea_typed};
use graphshell::local_edit::{DetailAction, apply_detail_action, relation_target};
use graphshell::product::EditableRelation;
use mere::canvas::Face;

const RELATIONS: [(EditableRelation, &str); 5] = [
    (EditableRelation::UserGrouped, "Grouped with"),
    (EditableRelation::Cites, "Cites"),
    (EditableRelation::Hyperlink, "Links to"),
    (EditableRelation::CollectionMember, "Contains"),
    (EditableRelation::FrameMember, "Shares scene"),
];
const FACES: [(Face, &str); 4] = [
    (Face::Favicon, "Favicon"),
    (Face::Derived, "Derived"),
    (Face::Bare, "Bare"),
    (Face::Sprite, "Sprite"),
];
const HANDLERS: [(&str, &str); 2] = [
    ("graphshell.inspect", "Inspect in Graphshell"),
    ("system.default", "Open in another application"),
];
const DEFAULT_SPRITE: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

pub(super) struct DetailControls {
    pub(super) facet_id: TextInput,
    pub(super) facet_json: TextInput,
    pub(super) target: TextInput,
    relation: SelectState,
    pub(super) face: SelectState,
    handler: SelectState,
    actions: u64,
}

impl Default for DetailControls {
    fn default() -> Self {
        Self {
            facet_id: TextInput::new("notes.user/v1"),
            facet_json: TextInput::new("{}"),
            target: TextInput::new(""),
            relation: SelectState::new(0).with_label("Relation kind"),
            face: SelectState::new(0).with_label("Representation"),
            handler: SelectState::new(0).with_label("Open handler"),
            actions: 0,
        }
    }
}

pub(super) fn face_index(canvas: &Canvas, member: Option<Uuid>) -> usize {
    member
        .and_then(|id| canvas.graph().get_node_by_id(id))
        .and_then(|(key, _)| {
            FACES
                .iter()
                .position(|(face, _)| *face == canvas.node_face(key))
        })
        .unwrap_or(0)
}

impl SavedProduct {
    pub(super) fn selected_handler(&self) -> &'static str {
        HANDLERS[self.detail.handler.selected.min(HANDLERS.len() - 1)].0
    }
    pub(super) fn restore_handler(&mut self, id: &str) {
        self.detail.handler.selected = HANDLERS
            .iter()
            .position(|(handler, _)| *handler == id)
            .unwrap_or(0);
    }
    pub(in crate::web_tree) fn action_count(&self) -> u64 {
        self.detail.actions
    }
    pub(in crate::web_tree) fn detail_facet(&self) -> String {
        self.app
            .as_ref()
            .and_then(|app| {
                app.host
                    .facet_value(&self.address, self.detail.facet_id.text())
            })
            .map(|value| value.to_string())
            .unwrap_or_default()
    }

    fn save_detail(&mut self, relation: bool) {
        if self.selection_locked() {
            return;
        }
        let Some(member) = self.selected else {
            return;
        };
        let Some(app) = self.app.as_mut() else {
            return;
        };
        let target = if relation {
            match relation_target(app, member, self.detail.target.text()) {
                Ok(target) => Some(target),
                Err(error) => {
                    self.status = error;
                    return;
                },
            }
        } else {
            None
        };
        let action = if relation {
            DetailAction::Relation {
                target: self.detail.target.text().into(),
                relation: RELATIONS[self.detail.relation.selected.min(RELATIONS.len() - 1)].0,
            }
        } else {
            DetailAction::Facet {
                id: self.detail.facet_id.text().into(),
                json: self.detail.facet_json.text().into(),
            }
        };
        match apply_detail_action(app, member, action) {
            Ok(()) => {
                self.pending_detail = Some(CompletedEdit::Detail(target));
                self.retry_detail();
            },
            Err(error) => {
                self.status = error;
                self.save_state = "error";
            },
        }
    }

    pub(super) fn retry_detail(&mut self) {
        if self.saving {
            return;
        }
        let (Some(edit), Some(member)) = (self.pending_detail, self.selected) else {
            return;
        };
        let Some(mut app) = self.app.take() else {
            return;
        };
        self.saving = true;
        self.save_state = "saving";
        self.status = "Saving detail changes…".into();
        let completed = self.completed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = persist_intake(&mut app, member, now_secs()).await;
            *completed.borrow_mut() = Some((app, edit, result));
        });
    }

    fn select_pair(&mut self, canvas: &mut Canvas) {
        if self.selection_locked() {
            return;
        }
        let (Some(member), Some(app)) = (self.selected, self.app.as_ref()) else {
            return;
        };
        match relation_target(app, member, self.detail.target.text()) {
            Ok(target) => {
                canvas.set_selected_members(&[member, target]);
                self.status = "Relation pair selected".into();
            },
            Err(error) => self.status = error,
        }
    }

    fn use_face(&mut self, canvas: &mut Canvas) {
        if self.selection_locked() {
            return;
        }
        let Some(member) = self.selected else {
            return;
        };
        let face = FACES[self.detail.face.selected.min(FACES.len() - 1)].0;
        if face == Face::Sprite {
            canvas.set_node_sprite(member, DEFAULT_SPRITE.into());
        } else {
            canvas.set_node_face(member, face);
        }
        self.status = format!("Representation set to {}", face.as_code());
    }

    pub(super) fn open_selected(&mut self) {
        if self.selection_locked() {
            return;
        }
        let (Some(member), Some(app)) = (self.selected, self.app.as_mut()) else {
            return;
        };
        let Ok(data) = metadata(app, member) else {
            return;
        };
        let handler = HANDLERS[self.detail.handler.selected.min(HANDLERS.len() - 1)].0;
        self.detail.actions = self.detail.actions.saturating_add(1);
        self.status = match app.open_address(&data.address, handler) {
            Ok(chirograph::IntentResult::Accepted) => "Accepted".into(),
            Ok(result) => format!("{result:?}"),
            Err(error) => format!("Open failed: {error}"),
        };
        if self.status == "Accepted" && handler == "system.default" {
            self.status = match web_sys::window().and_then(|window| {
                window
                    .open_with_url_and_target(&data.address, "_blank")
                    .ok()
                    .flatten()
            }) {
                Some(_) => "Accepted · opened in host browser".into(),
                None => "Failed · host browser blocked the external open".into(),
            };
        }
    }
}

fn field(label: &'static str, field: TextField, multiline: bool) -> Child {
    Box::new(el(
        "label",
        (
            label,
            lens(
                move |input: &mut TextInput| {
                    if multiline {
                        textarea_typed(input).attr("aria-label", label)
                    } else {
                        text_field_typed(input).attr("aria-label", label)
                    }
                },
                move |page: &mut TreePage| {
                    field.get_mut(page.product.as_mut().expect("local detail"))
                },
            ),
        ),
    ))
}

pub(super) fn controls(_page: &TreePage) -> Child {
    let mut children: Vec<Child> = vec![
        field("Facet id", TextField::FacetId, false),
        field("Facet JSON", TextField::FacetJson, true),
        Box::new(button("Save facet", |page: &mut TreePage, _| {
            if let Some(product) = &mut page.product {
                product.save_detail(false);
            }
        })),
        field("Relate to address", TextField::RelationTarget, false),
    ];
    children.push(Box::new(lens(
        |state: &mut SelectState| select(state, &RELATIONS.map(|(_, label)| label)),
        |page: &mut TreePage| &mut page.product.as_mut().expect("local detail").detail.relation,
    )));
    children.push(Box::new(button(
        "Add relation",
        |page: &mut TreePage, _| {
            if let Some(product) = &mut page.product {
                product.save_detail(true);
            }
        },
    )));
    children.push(Box::new(button("Select pair", |page: &mut TreePage, _| {
        if let Some(product) = &mut page.product {
            product.select_pair(&mut page.shared.canvas.borrow_mut());
            page.shared.dirty.set(true);
        }
    })));
    children.push(Box::new(lens(
        |state: &mut SelectState| select(state, &FACES.map(|(_, label)| label)),
        |page: &mut TreePage| &mut page.product.as_mut().expect("local detail").detail.face,
    )));
    children.push(Box::new(button(
        "Use representation",
        |page: &mut TreePage, _| {
            if let Some(product) = &mut page.product {
                product.use_face(&mut page.shared.canvas.borrow_mut());
                page.shared.dirty.set(true);
            }
        },
    )));
    children.push(Box::new(lens(
        |state: &mut SelectState| select(state, &HANDLERS.map(|(_, label)| label)),
        |page: &mut TreePage| &mut page.product.as_mut().expect("local detail").detail.handler,
    )));
    children.push(Box::new(button(
        "Open selected",
        |page: &mut TreePage, _| {
            if let Some(product) = &mut page.product {
                product.open_selected();
            }
        },
    )));
    Box::new(el("section", children).attr("aria-label", "Detail actions"))
}
