// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The bounded local saved-graph workflow on the retained tree.
use super::*;
use cambium::{SelectState, TextInput};
use graphshell::local_edit::{NodeMetadata, metadata, save_metadata, sync_canvas_metadata};
use mere::canvas::Role;
use muniment::IndexedDbBackend;
use uuid::Uuid;

type App = GraphshellApp<IndexedDbBackend>;
type Completion = (App, Result<NodeMetadata, String>);

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
    }))
}

fn now_secs() -> u64 {
    (js_sys::Date::now() / 1_000.0) as u64
}

/// Map the retained detail field to the host's caret, IME and selection path.
pub(super) fn focused_text(
    runner: &cambium_rootstock::Runner<TreePage, Logic, Child>,
) -> Option<cambium_rootstock::FocusedTextSlot<TreePage>> {
    let product = runner.state().product.as_ref()?;
    if !product.detail_open || product.saving {
        return None;
    }
    let node = runner.focus()?;
    let dom = runner.dom();
    let dom = dom.borrow();
    let title =
        taproot::matching(&dom, &Selector::role("textbox").containing("Title")).contains(&node);
    if !title
        && !taproot::matching(&dom, &Selector::role("textbox").containing("Tags")).contains(&node)
    {
        return None;
    }
    Some(cambium_rootstock::FocusedTextSlot {
        node,
        get: Box::new(move |page: &TreePage| {
            let product = page.product.as_ref().expect("focused local detail");
            if title { &product.title } else { &product.tags }
        }),
        get_mut: Box::new(move |page: &mut TreePage| {
            let product = page.product.as_mut().expect("focused local detail");
            if title {
                &mut product.title
            } else {
                &mut product.tags
            }
        }),
    })
}

impl SavedProduct {
    pub(super) fn ready(&self) -> bool {
        self.completed.borrow().is_some()
    }
    pub(super) fn graph(&self) -> Graph {
        self.app
            .as_ref()
            .expect("app present before a save")
            .host
            .graph()
            .clone()
    }

    pub(super) fn select(&mut self, selected: Option<Uuid>) {
        if self.saving || self.selected == selected {
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
        self.detail_open = true;
        self.role.selected =
            item_role_index(self.selected.and_then(|member| canvas.member_role(member)));
    }

    pub(super) fn save(&mut self) {
        if self.saving {
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
            *completed.borrow_mut() = Some((app, result));
        });
    }

    pub(super) fn poll(&mut self, canvas: &mut Canvas) -> bool {
        let Some((app, result)) = self.completed.borrow_mut().take() else {
            return false;
        };
        self.app = Some(app);
        self.saving = false;
        match result {
            Ok(saved) => {
                let refresh = sync_canvas_metadata(canvas, &saved);
                self.title = TextInput::new(saved.title);
                self.tags = TextInput::new(saved.tags.join(", "));
                self.status = match refresh {
                    Ok(()) => "Changes saved".into(),
                    Err(error) => format!("Changes saved · canvas refresh failed: {error}"),
                };
                self.save_state = "saved";
            },
            Err(error) => {
                self.status = format!("Save failed · {error} · changes remain in memory");
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
    page.shared
        .canvas
        .borrow_mut()
        .set_member_role(member, role);
    product.status = match role {
        Some(role) => format!("Item role set to {}", role.id()),
        None => "Item role follows the recipe".into(),
    };
    page.shared.dirty.set(true);
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
