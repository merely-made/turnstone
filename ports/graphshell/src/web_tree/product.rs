// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The bounded local saved-graph workflow on the retained tree.
use super::*;
use cambium::{FileEvent, SelectState, TextInput};
use graphshell::local_edit::{
    NodeMetadata, metadata, save_metadata, sync_canvas_metadata_from_graph,
};
use graphshell::local_intake::{create_address, create_file, persist_intake, sync_canvas_intake};
use mere::canvas::Role;
use muniment::IndexedDbBackend;
use uuid::Uuid;

type App = GraphshellApp<IndexedDbBackend>;
#[derive(Clone, Copy)]
enum CompletedEdit {
    Metadata,
    Intake,
}
type Completion = (App, CompletedEdit, Result<NodeMetadata, String>);

pub(super) struct SavedProduct {
    app: Option<App>,
    completed: Rc<RefCell<Option<Completion>>>,
    pub(super) selected: Option<Uuid>,
    pub(super) title: TextInput,
    pub(super) tags: TextInput,
    pub(super) address: String,
    pub(super) detail_open: bool,
    /// The picked item's own role: index 0 is "as recipe" (no override),
    /// then each role (F48).
    pub(super) role: SelectState,
    pub(super) saving: bool,
    /// Save feedback for the detail editor; empty until a save.
    pub(super) status: String,
    /// The store's open and persistence state, shown in Graph tools.
    pub(super) storage: String,
    pub(super) save_state: &'static str,
    pub(super) session: String,
    pub(super) reopened: bool,
    intake_open: bool,
    intake_address: TextInput,
    intake_title: TextInput,
    file_requested: bool,
    /// A created but unacknowledged member. Retrying stores this same object.
    pending_intake: Option<Uuid>,
}

/// The existing fixture and generated comparison routes remain unchanged.
/// `?app=local` is the opt-in migration route onto the existing browser store.
pub(super) async fn open() -> Result<Option<SavedProduct>, String> {
    let search = super::super::window()?
        .location()
        .search()
        .map_err(|_| "cannot read page options")?;
    let params =
        web_sys::UrlSearchParams::new_with_str(&search).map_err(|_| "invalid page options")?;
    if params.get("app").as_deref() != Some("local") {
        return Ok(None);
    }
    if web_graphs::requested().is_some() {
        return Err("app=local cannot also select a generated graph".into());
    }
    let backend = IndexedDbBackend::open("graphshell-reference-host-h5", "muniment")
        .await
        .map_err(|error| error.to_string())?;
    let mut app = GraphshellApp::open_or_fixture(
        backend,
        SelectedPersonaRef {
            persona: FIXTURE_PERSONA_ADDRESS.into(),
            profile: "profile:graphshell-h3".into(),
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    let reopened = app.host.was_reopened();
    app.host
        .persist(now_secs())
        .await
        .map_err(|error| error.to_string())?;
    app.mount_local().map_err(|error| error.to_string())?;
    let persistence = super::super::resolve_storage_persistence().await;
    let storage = graphshell::browser_storage::status_line(
        if reopened {
            "IndexedDB reopened"
        } else {
            "IndexedDB seeded"
        },
        &persistence,
    );
    let session = app.host.graph_session().id().0.to_string();
    Ok(Some(SavedProduct {
        app: Some(app),
        completed: Rc::new(RefCell::new(None)),
        selected: None,
        title: TextInput::new(""),
        tags: TextInput::new(""),
        address: String::new(),
        detail_open: false,
        role: SelectState::new(0).with_label("Item role"),
        saving: false,
        status: String::new(),
        storage,
        save_state: "idle",
        session,
        reopened,
        intake_open: false,
        intake_address: TextInput::new(""),
        intake_title: TextInput::new(""),
        file_requested: false,
        pending_intake: None,
    }))
}

fn now_secs() -> u64 {
    (js_sys::Date::now() / 1_000.0) as u64
}

#[derive(Clone, Copy)]
enum TextField {
    Title,
    Tags,
    IntakeAddress,
    IntakeTitle,
}

impl TextField {
    fn get(self, product: &SavedProduct) -> &TextInput {
        match self {
            Self::Title => &product.title,
            Self::Tags => &product.tags,
            Self::IntakeAddress => &product.intake_address,
            Self::IntakeTitle => &product.intake_title,
        }
    }
    fn get_mut(self, product: &mut SavedProduct) -> &mut TextInput {
        match self {
            Self::Title => &mut product.title,
            Self::Tags => &mut product.tags,
            Self::IntakeAddress => &mut product.intake_address,
            Self::IntakeTitle => &mut product.intake_title,
        }
    }
}

/// Map the retained fields to the host's caret, IME and selection path.
pub(super) fn focused_text(
    runner: &cambium_rootstock::Runner<TreePage, Logic, Child>,
) -> Option<cambium_rootstock::FocusedTextSlot<TreePage>> {
    let product = runner.state().product.as_ref()?;
    if product.saving || product.pending_intake.is_some() {
        return None;
    }
    let node = runner.focus()?;
    let dom = runner.dom();
    let dom = dom.borrow();
    let field = [
        ("Title", TextField::Title, product.detail_open),
        ("Tags", TextField::Tags, product.detail_open),
        ("Address", TextField::IntakeAddress, product.intake_open),
        (
            "New object title",
            TextField::IntakeTitle,
            product.intake_open,
        ),
    ]
    .into_iter()
    .find_map(|(label, field, visible)| {
        (visible
            && taproot::matching(&dom, &Selector::role("textbox").containing(label))
                .contains(&node))
        .then_some(field)
    })?;
    Some(cambium_rootstock::FocusedTextSlot {
        node,
        get: Box::new(move |page: &TreePage| {
            let product = page.product.as_ref().expect("focused local detail");
            field.get(product)
        }),
        get_mut: Box::new(move |page: &mut TreePage| {
            let product = page.product.as_mut().expect("focused local detail");
            field.get_mut(product)
        }),
    })
}

impl SavedProduct {
    pub(super) fn forme_source(&self) -> Option<(IndexedDbBackend, Uuid)> {
        let app = self.app.as_ref()?;
        Some((app.host.store(), app.host.graph_session().id().0))
    }
    pub(super) fn ready(&self) -> bool {
        self.completed.borrow().is_some()
    }
    pub(super) fn selection_locked(&self) -> bool {
        self.saving || self.file_requested || self.pending_intake.is_some()
    }
    pub(super) fn graph(&self) -> Graph {
        self.app
            .as_ref()
            .expect("app present before a save")
            .host
            .graph()
            .clone()
    }

    /// Inspect the current source owner, never a cached Canvas or form draft.
    pub(super) fn inspect_access(
        &self,
        member: Uuid,
    ) -> Result<graphshell::access_inspection::AccessInspection, String> {
        let app = self
            .app
            .as_ref()
            .ok_or("Source inspection will resume when the current save finishes")?;
        graphshell::access_inspection::inspect_access(app.host.graph(), member)
    }

    /// Selected file facts from the acknowledged local owner, for receipts.
    pub(super) fn saved_file(&self) -> Option<serde_json::Value> {
        if self.saving || self.pending_intake.is_some() {
            return None;
        }
        let app = self.app.as_ref()?;
        let (_, node) = app.host.graph().get_node_by_id(self.selected?)?;
        let content = app
            .host
            .facet_value(node.url(), graphshell::product::CONTENT_FACET)?;
        let local = app
            .host
            .facet_value(node.url(), graphshell::product::LOCAL_FILE_FACET)?;
        Some(serde_json::json!({ "content": content, "local": local }))
    }

    pub(super) fn select(&mut self, selected: Option<Uuid>) {
        if self.selection_locked() || self.selected == selected {
            return;
        }
        self.selected = selected;
        self.detail_open = false;
        self.save_state = "idle";
        let data = selected
            .and_then(|member| self.app.as_ref().and_then(|app| metadata(app, member).ok()));
        if let Some(data) = data {
            self.title = TextInput::new(data.title);
            self.tags = TextInput::new(data.tags.join(", "));
            self.address = data.address;
        } else {
            self.selected = None;
            self.title = TextInput::new("");
            self.tags = TextInput::new("");
            self.address.clear();
        }
    }

    /// Open the detail editor, its item role read from the canvas.
    pub(super) fn open_detail(&mut self, canvas: &Canvas) {
        // Keep the open_file view's registered target alive until the host
        // finishes reading the chosen bytes and dispatches its answer.
        if self.selection_locked() {
            return;
        }
        self.intake_open = false;
        self.detail_open = true;
        self.role.selected =
            item_role_index(self.selected.and_then(|member| canvas.member_role(member)));
    }

    pub(super) fn save(&mut self) {
        if self.saving || self.file_requested {
            return;
        }
        if self.pending_intake.is_some() {
            self.status = "Retry intake before saving other changes".into();
            return;
        }
        let Some(member) = self.selected else {
            self.status = "Select an object before saving".into();
            return;
        };
        let Some(mut app) = self.app.take() else {
            return;
        };
        let title = self.title.text().to_string();
        let tags = self.tags.text().to_string();
        self.saving = true;
        self.save_state = "saving";
        self.status = "Saving changes…".into();
        let completed = self.completed.clone();
        // The task owns the app. No RefCell borrow crosses the IndexedDB await.
        wasm_bindgen_futures::spawn_local(async move {
            let result = save_metadata(&mut app, member, &title, &tags, now_secs()).await;
            *completed.borrow_mut() = Some((app, CompletedEdit::Metadata, result));
        });
    }

    fn add_address(&mut self) {
        if self.saving || self.pending_intake.is_some() || self.file_requested {
            return;
        }
        let Some(app) = self.app.as_mut() else {
            return;
        };
        match create_address(app, self.intake_address.text(), self.intake_title.text()) {
            Ok(member) => self.store_intake(member),
            Err(error) => {
                self.status = error;
                self.save_state = "error";
            },
        }
    }

    fn receive_file(&mut self, event: FileEvent) {
        self.file_requested = false;
        if self.saving || self.pending_intake.is_some() {
            return;
        }
        let Some(file) = event.files.into_iter().next() else {
            self.status = "File choice canceled".into();
            return;
        };
        let Some(app) = self.app.as_mut() else {
            return;
        };
        match create_file(
            app,
            &file.name,
            file.media_type.as_deref(),
            file.last_modified_ms,
            &file.bytes,
        ) {
            Ok(member) => self.store_intake(member),
            Err(error) => {
                self.status = format!("File intake refused · {error}");
                self.save_state = "error";
            },
        }
    }

    fn store_intake(&mut self, member: Uuid) {
        if self.saving {
            return;
        }
        let Some(mut app) = self.app.take() else {
            return;
        };
        self.pending_intake = Some(member);
        self.saving = true;
        self.save_state = "saving";
        self.status = "Saving new object…".into();
        let completed = self.completed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = persist_intake(&mut app, member, now_secs()).await;
            *completed.borrow_mut() = Some((app, CompletedEdit::Intake, result));
        });
    }

    pub(super) fn poll(&mut self, canvas: &mut Canvas) -> bool {
        let Some((app, edit, result)) = self.completed.borrow_mut().take() else {
            return false;
        };
        self.app = Some(app);
        self.saving = false;
        match result {
            Ok(saved) => {
                let refresh = match edit {
                    CompletedEdit::Metadata => sync_canvas_metadata_from_graph(
                        canvas,
                        self.app.as_ref().expect("completed app").host.graph(),
                        &saved,
                    ),
                    CompletedEdit::Intake => {
                        let refresh = sync_canvas_intake(
                            canvas,
                            self.app.as_ref().expect("completed app").host.graph(),
                            saved.member,
                        );
                        self.pending_intake = None;
                        if refresh.is_ok() {
                            self.select(Some(saved.member));
                            self.open_detail(canvas);
                            self.intake_address = TextInput::new("");
                            self.intake_title = TextInput::new("");
                        }
                        refresh
                    },
                };
                if matches!(edit, CompletedEdit::Metadata) || refresh.is_ok() {
                    self.title = TextInput::new(saved.title);
                    self.tags = TextInput::new(saved.tags.join(", "));
                }
                self.status = match refresh {
                    Ok(()) => match edit {
                        CompletedEdit::Metadata => "Changes saved".into(),
                        CompletedEdit::Intake => "Object added and saved".into(),
                    },
                    Err(error) => match edit {
                        CompletedEdit::Metadata => {
                            format!("Changes saved · canvas refresh failed: {error}")
                        },
                        CompletedEdit::Intake => format!(
                            "Object saved · canvas refresh failed: {error} · reload to restore the local view"
                        ),
                    },
                };
                self.save_state = "saved";
            },
            Err(error) => {
                self.status = match edit {
                    CompletedEdit::Metadata => {
                        format!("Save failed · {error} · changes remain in memory")
                    },
                    CompletedEdit::Intake => format!(
                        "Intake save failed · {error} · new object remains in memory; retry intake to save it"
                    ),
                };
                self.save_state = "error";
            },
        }
        true
    }
}

/// The item role select's index for an override (0 is "as recipe").
fn item_role_index(role: Option<Role>) -> usize {
    role.and_then(|role| Role::ALL.iter().position(|r| *r == role))
        .map_or(0, |index| index + 1)
}

/// Set or clear the picked item's own role (F48). It is view state, saved
/// with the scene, not graph truth.
fn apply_item_role(page: &mut TreePage) {
    let Some(product) = &mut page.product else {
        return;
    };
    let Some(member) = product.selected else {
        product.status = "Select an object first".into();
        return;
    };
    let role = product
        .role
        .selected
        .checked_sub(1)
        .and_then(|i| Role::ALL.get(i).copied());
    let set = page
        .shared
        .canvas
        .borrow_mut()
        .set_member_role(member, role);
    product.status = match role {
        _ if !set => "Item role refused: the item does not permit it".into(),
        Some(role) => format!("Item role set to {}", role.id()),
        None => "Item role follows the recipe".into(),
    };
    page.shared.dirty.set(true);
}

fn intake_controls(page: &TreePage) -> Child {
    use cambium::{FileFilter, button, el, lens, open_file, text_field_typed};
    let product = page.product.as_ref().expect("local intake");
    let mut children: Vec<Child> = vec![Box::new(
        button("Add an object", |page: &mut TreePage, _| {
            if let Some(product) = &mut page.product
                && !product.saving
                && !product.file_requested
            {
                product.intake_open = !product.intake_open;
                if product.intake_open {
                    product.detail_open = false;
                }
            }
        })
        .attr(
            "aria-expanded",
            if product.intake_open { "true" } else { "false" },
        ),
    )];
    if product.intake_open {
        let mut fields: Vec<Child> = Vec::new();
        if product.saving {
            fields.push(Box::new(el("p", "Saving new object…")));
        } else if let Some(member) = product.pending_intake {
            fields.push(Box::new(button(
                "Retry intake",
                move |page: &mut TreePage, _| {
                    if let Some(product) = &mut page.product {
                        product.store_intake(member);
                    }
                },
            )));
        } else {
            fields.push(Box::new(el(
                "label",
                (
                    "Address",
                    lens(
                        |input: &mut TextInput| {
                            text_field_typed(input).attr("aria-label", "Address")
                        },
                        |page: &mut TreePage| {
                            &mut page.product.as_mut().expect("local intake").intake_address
                        },
                    ),
                ),
            )));
            fields.push(Box::new(el(
                "label",
                (
                    "New object title",
                    lens(
                        |input: &mut TextInput| {
                            text_field_typed(input).attr("aria-label", "New object title")
                        },
                        |page: &mut TreePage| {
                            &mut page.product.as_mut().expect("local intake").intake_title
                        },
                    ),
                ),
            )));
            fields.push(Box::new(button(
                cambium::catalogue::label(cambium::catalogue::ids::NODE_NEW)
                    .expect("shared New node label"),
                |page: &mut TreePage, _| {
                    if let Some(product) = &mut page.product {
                        product.add_address();
                    }
                },
            )));
            fields.push(Box::new(open_file(
                button("Choose a file", |page: &mut TreePage, _| {
                    if let Some(product) = &mut page.product
                        && !product.saving
                        && product.pending_intake.is_none()
                    {
                        product.file_requested = true;
                    }
                }),
                product.file_requested,
                FileFilter::default(),
                |page: &mut TreePage, event: FileEvent| {
                    if let Some(product) = &mut page.product {
                        product.receive_file(event);
                    }
                },
            )));
        }
        children.push(Box::new(
            el("section", fields)
                .attr("class", "tree-detail tree-intake")
                .attr("aria-label", "Add an object"),
        ));
    }
    Box::new(el("div", children))
}

pub(super) fn controls(page: &TreePage) -> Child {
    use cambium::{DetailRow, DetailSection, button, detail_panel, el, lens, text_field_typed};
    let Some(product) = &page.product else {
        return Box::new(el("div", ()));
    };
    let mut children: Vec<Child> = Vec::new();
    if !product.status.is_empty() {
        children.push(Box::new(
            el("p", product.status.clone()).attr("role", "status"),
        ));
    }
    children.push(intake_controls(page));
    children.push(super::inspection::open_button(page));
    if product.selected.is_some() {
        children.push(Box::new(button(
            if product.detail_open {
                "Close details"
            } else {
                "Open details"
            },
            |page: &mut TreePage, _| {
                let canvas = page.shared.canvas.borrow();
                if let Some(product) = &mut page.product {
                    if product.detail_open {
                        product.detail_open = false;
                    } else {
                        product.open_detail(&canvas);
                    }
                }
            },
        )));
    }
    if product.detail_open {
        let mut fields: Vec<Child> = vec![Box::new(detail_panel(&[DetailSection::new(
            "Object details",
            vec![DetailRow::new("Address", &product.address)],
        )]))];
        if product.saving {
            fields.push(Box::new(el("p", "Saving changes…")));
        } else {
            fields.push(Box::new(el(
                "label",
                (
                    "Title",
                    lens(
                        |input: &mut TextInput| text_field_typed(input).attr("aria-label", "Title"),
                        |page: &mut TreePage| {
                            &mut page.product.as_mut().expect("local detail").title
                        },
                    ),
                ),
            )));
            fields.push(Box::new(el(
                "label",
                (
                    "Tags",
                    lens(
                        |input: &mut TextInput| text_field_typed(input).attr("aria-label", "Tags"),
                        |page: &mut TreePage| {
                            &mut page.product.as_mut().expect("local detail").tags
                        },
                    ),
                ),
            )));
            fields.push(Box::new(button(
                "Save changes",
                |page: &mut TreePage, _| {
                    if let Some(product) = &mut page.product {
                        product.save();
                    }
                },
            )));
            let options: Vec<&'static str> = std::iter::once("As recipe")
                .chain(Role::ALL.iter().map(|role| role.label()))
                .collect();
            fields.push(Box::new(el(
                "label",
                (
                    "Item role",
                    lens(
                        move |state: &mut SelectState| cambium::select(state, &options),
                        |page: &mut TreePage| {
                            &mut page.product.as_mut().expect("local detail").role
                        },
                    ),
                ),
            )));
            fields.push(Box::new(button(
                "Apply item role",
                |page: &mut TreePage, _| apply_item_role(page),
            )));
        }
        children.push(Box::new(
            el("section", fields)
                .attr("class", "tree-detail")
                .attr("aria-label", "Object details"),
        ));
    }
    Box::new(
        el("aside", children)
            .attr("class", "tree-product")
            .attr("aria-label", "Saved graph"),
    )
}
