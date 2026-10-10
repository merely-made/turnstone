// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Workbench reading tiles and their field in the retained Graphshell page.
use super::*;
use cambium::{Slot, button, frisket_with};
use graphshell::forme_workspace::FormeWorkspace;
use muniment::IndexedDbBackend;
use uuid::Uuid;
use workbench::{ContentSource, SplitAxis, Tile, TileEvent};

pub(super) struct Pane {
    pub(super) model: FormeWorkspace,
    backend: IndexedDbBackend,
    completed: Rc<RefCell<Option<Result<(), String>>>>,
    pub(super) saving: bool,
    pub(super) status: String,
    pub(super) workbench: bool,
    moving: Option<Uuid>,
    candidate: Option<FormeWorkspace>,
}
impl Pane {
    pub(super) async fn open(product: &product::SavedProduct) -> Result<Self, String> {
        let (backend, session) = product.forme_source().ok_or("workspace store is busy")?;
        let model = FormeWorkspace::load(backend.clone(), session, &product.graph()).await?;
        Ok(Self {
            model,
            backend,
            completed: Rc::new(RefCell::new(None)),
            saving: false,
            status: String::new(),
            workbench: false,
            moving: None,
            candidate: None,
        })
    }
    pub(super) fn install(&self, canvas: &mut Canvas) -> Result<(), String> {
        canvas.set_forme_region((!self.model.members().is_empty()).then(|| self.model.region()))
    }
    fn save(&mut self) {
        if self.saving {
            return;
        }
        let now = js_sys::Date::now() as u64;
        if self.model.document.created_at_ms == 0 {
            self.model.document.created_at_ms = now;
        }
        self.model.document.updated_at_ms = now;
        self.saving = true;
        self.status = "Saving workbench…".into();
        let backend = self.backend.clone();
        let saved = self.model.clone();
        let completed = self.completed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            *completed.borrow_mut() = Some(saved.save(backend).await);
        });
    }
    pub(super) fn previewing(&self) -> bool {
        self.candidate.is_some()
    }
    pub(super) fn ready(&self) -> bool {
        self.completed.borrow().is_some()
    }
    pub(super) fn poll(&mut self) -> bool {
        let Some(result) = self.completed.borrow_mut().take() else {
            return false;
        };
        self.saving = false;
        self.status = match result {
            Ok(()) => "Workbench saved".into(),
            Err(e) => format!("Save failed · {e} · Retry save keeps this arrangement"),
        };
        true
    }
}
fn changed(page: &mut TreePage) {
    let Some(pane) = &mut page.forme else {
        return;
    };
    pane.candidate = None;
    pane.moving = None;
    let result = pane.install(&mut page.shared.canvas.borrow_mut());
    match result {
        Ok(()) => pane.save(),
        Err(e) => pane.status = e,
    }
    page.shared.dirty.set(true);
}
fn open_selected(page: &mut TreePage) {
    let Some(pane) = &mut page.forme else {
        return;
    };
    if pane.saving {
        return;
    }
    let canvas = page.shared.canvas.borrow();
    let selected = canvas.selected_members();
    if selected.is_empty() {
        pane.status = "Select a node to open it in the workbench".into();
        return;
    }
    let mut candidate = pane.model.clone();
    for member in selected {
        if let Err(e) = candidate.open(member, canvas.graph()) {
            pane.status = e;
            return;
        }
    }
    pane.model = candidate;
    pane.workbench = true;
    drop(canvas);
    changed(page);
}
fn arrange(page: &mut TreePage, axis: Option<SplitAxis>) {
    let Some(pane) = &mut page.forme else {
        return;
    };
    if pane.saving {
        return;
    }
    if pane.model.locked {
        pane.status = "Unlock the forme to change its arrangement".into();
        return;
    }
    let mut layout = pane.model.layout();
    // Explicit convenience commands; nested trees otherwise remain untouched.
    match axis {
        None => layout.stack_all(),
        Some(SplitAxis::Row) => layout.split_all(),
        Some(SplitAxis::Column) => {
            let members = layout.open_members();
            for pair in members.windows(2) {
                layout.split_beside_axis(pair[1], pair[0], SplitAxis::Column, true);
            }
        },
    }
    let mut candidate = pane.model.clone();
    candidate.keep_layout(&layout);
    if let Err(e) = candidate.validate(candidate.document.graph_id) {
        pane.status = e;
        return;
    }
    pane.model = candidate;
    changed(page);
}
fn tile_event(page: &mut TreePage, event: TileEvent) {
    let Some(pane) = &mut page.forme else {
        return;
    };
    if pane.saving {
        return;
    }
    let activated = if let TileEvent::Activated(tile) = &event {
        pane.model.member_for_tile(*tile)
    } else {
        None
    };
    let result = pane.model.event(event, page.shared.canvas.borrow().graph());
    if let Err(e) = result {
        pane.status = e;
        return;
    }
    if let Some(member) = activated {
        page.shared.canvas.borrow_mut().select_member(member);
        selection_changed(page);
    }
    changed(page);
}
fn translate(page: &mut TreePage, dx: f32, dy: f32, scale: f32) {
    let Some(pane) = &mut page.forme else {
        return;
    };
    if pane.saving || pane.model.locked {
        return;
    }
    let [x, y, r, b] = pane.model.bounds;
    let (cx, cy) = (x + (r - x) / 2. + dx, y + (b - y) / 2. + dy);
    let (w, h) = (((r - x) * scale).max(80.), ((b - y) * scale).max(80.));
    let mut candidate = pane.model.clone();
    candidate.bounds = [cx - w / 2., cy - h / 2., cx + w / 2., cy + h / 2.];
    if let Err(e) = candidate.validate(candidate.document.graph_id) {
        pane.status = e;
        return;
    }
    pane.model = candidate;
    changed(page);
}

fn selection_changed(page: &mut TreePage) {
    let canvas = page.shared.canvas.borrow();
    page.picked = canvas.focused_url().map(str::to_owned);
    if let Some(product) = &mut page.product {
        product.select(canvas.selected_members().first().copied());
    }
}
fn begin_move(page: &mut TreePage, member: Uuid) {
    let Some(pane) = &mut page.forme else {
        return;
    };
    if pane.saving || pane.model.locked {
        return;
    }
    pane.candidate = None;
    pane.moving = Some(member);
    let _ = pane.install(&mut page.shared.canvas.borrow_mut());
    page.shared.dirty.set(true);
}
fn preview(page: &mut TreePage, target: Uuid, edge: workbench::Edge) {
    let Some(pane) = &mut page.forme else {
        return;
    };
    if pane.saving || pane.model.locked {
        return;
    }
    let Some(moving) = pane.moving else {
        return;
    };
    let mut candidate = pane.model.clone();
    let (Some(tile), Some(to)) = (
        candidate.tile_for_member(moving),
        candidate.tile_for_member(target),
    ) else {
        return;
    };
    let event = TileEvent::Dragged {
        tile,
        to: workbench::DropTarget::Edge { tile: to, edge },
    };
    if let Err(e) = candidate.event(event, page.shared.canvas.borrow().graph()) {
        pane.status = e;
        return;
    }
    match page
        .shared
        .canvas
        .borrow_mut()
        .set_forme_region(Some(candidate.region()))
    {
        Ok(()) => {
            pane.candidate = Some(candidate);
            pane.status = "Preview ready · Apply move or cancel".into();
        },
        Err(e) => pane.status = e,
    }
    page.shared.dirty.set(true);
}
fn finish_preview(page: &mut TreePage, apply: bool) {
    let Some(pane) = &mut page.forme else {
        return;
    };
    if pane.saving {
        return;
    }
    if let Some(candidate) = pane.candidate.take() {
        if apply {
            pane.model = candidate;
        }
    }
    pane.moving = None;
    if apply {
        changed(page);
    } else {
        pane.status = "Tile move cancelled".into();
        if let Some(f) = &page.forme {
            let _ = f.install(&mut page.shared.canvas.borrow_mut());
        }
        page.shared.dirty.set(true);
    }
}

pub(super) fn toolbar(page: &TreePage) -> Child {
    let Some(pane) = &page.forme else {
        return Box::new(el("span", ()));
    };
    Box::new(
        el(
            "div",
            (
                button("Open selected in workbench", |p: &mut TreePage, _| {
                    open_selected(p)
                }),
                button(
                    if pane.workbench {
                        "Return to Mere"
                    } else {
                        "Show workbench"
                    },
                    |p: &mut TreePage, _| {
                        if let Some(f) = &mut p.forme {
                            f.workbench = !f.workbench;
                        }
                        p.shared.dirty.set(true);
                    },
                ),
            ),
        )
        .attr("class", "tree-controls"),
    )
}

fn split_controls(tree: &workbench::TileTree, path: Vec<usize>, out: &mut Vec<Child>) {
    let workbench::TileTree::Split { children, .. } = tree else {
        return;
    };
    let name = if path.is_empty() {
        "root".into()
    } else {
        cambium::encode_pane_path(&path)
    };
    for (label, factor) in [("Enlarge", 1.2), ("Shrink", 1. / 1.2)] {
        let at = path.clone();
        out.push(Box::new(button(
            format!("{label} first region in split {name}"),
            move |p: &mut TreePage, _| {
                let Some(f) = &p.forme else {
                    return;
                };
                if f.saving || f.model.locked {
                    return;
                }
                if let Some(mut shares) = f.model.layout().split_fractions(&at) {
                    shares[0] *= factor;
                    tile_event(
                        p,
                        TileEvent::DividerMoved {
                            split: workbench::TilePath(at.clone()),
                            fractions: shares,
                        },
                    );
                }
            },
        )));
    }
    for (i, child) in children.iter().enumerate() {
        let mut at = path.clone();
        at.push(i);
        split_controls(&child.tree, at, out);
    }
}
pub(super) fn section(page: &TreePage) -> Child {
    let canvas = page.shared.canvas.borrow();
    let mut fields: Vec<_> = canvas.graph().fields().filter(|f| f.is_active()).collect();
    fields.sort_by_key(|f| f.id.as_uuid());
    let mut children: Vec<Child> = vec![Box::new(el("h2", "Fields"))];
    if let Some(error) = &page.forme_error {
        children.push(Box::new(el("p", error.clone()).attr("role", "alert")));
    }
    if let Some(pane) = &page.forme {
        children.push(Box::new(el(
            "p",
            format!(
                "Workbench · {} accesses · {} regions",
                pane.model.members().len(),
                pane.model.region().cells.len()
            ),
        )));
        let pins = pane
            .model
            .region()
            .cells
            .iter()
            .filter(|c| {
                canvas
                    .graph()
                    .get_node_key_by_id(c.member)
                    .is_some_and(|key| {
                        canvas.arrangement_role_of(key) == mere::canvas::Role::Pinned
                    })
            })
            .count();
        if pins > 0 {
            children.push(Box::new(el(
                "p",
                format!("{pins} position-pinned accesses keep their own positions."),
            )));
        }
        children.push(Box::new(button(
            if pane.model.locked {
                "Unlock forme"
            } else {
                "Lock forme"
            },
            |p: &mut TreePage, _| {
                if let Some(f) = &mut p.forme {
                    if f.saving {
                        return;
                    }
                    f.model.locked = !f.model.locked;
                }
                changed(p);
            },
        )));
        children.push(Box::new(button(
            if pane.model.visible {
                "Hide forme boundary"
            } else {
                "Show forme boundary"
            },
            |p: &mut TreePage, _| {
                if let Some(f) = &mut p.forme {
                    if f.saving {
                        return;
                    }
                    f.model.visible = !f.model.visible;
                }
                changed(p);
            },
        )));
        children.push(Box::new(button(
            "Select forme members",
            |p: &mut TreePage, _| {
                if let Some(f) = &p.forme {
                    let mut canvas = p.shared.canvas.borrow_mut();
                    canvas.clear_selection();
                    for m in f.model.members() {
                        canvas.toggle_select_member(m);
                    }
                }
                selection_changed(p);
                p.shared.dirty.set(true);
            },
        )));
        if !pane.model.locked {
            if let Some(tree) = pane.model.tile_tree(canvas.graph()) {
                split_controls(&tree, Vec::new(), &mut children);
            }
            for (label, axis) in [
                ("Arrange side by side", Some(SplitAxis::Row)),
                ("Arrange top to bottom", Some(SplitAxis::Column)),
                ("Group as tabs", None),
            ] {
                children.push(Box::new(button(label, move |p: &mut TreePage, _| {
                    arrange(p, axis)
                })));
            }
            for (label, dx, dy, scale) in [
                ("Move forme left", -40., 0., 1.),
                ("Move forme right", 40., 0., 1.),
                ("Move forme up", 0., -40., 1.),
                ("Move forme down", 0., 40., 1.),
                ("Enlarge forme", 0., 0., 1.2),
                ("Shrink forme", 0., 0., 1. / 1.2),
            ] {
                children.push(Box::new(button(label, move |p: &mut TreePage, _| {
                    translate(p, dx, dy, scale)
                })));
            }
        }
        if pane.moving.is_some() {
            children.push(Box::new(el(
                "p",
                "Choose a tile's placement preview, then apply or cancel.",
            )));
            children.push(Box::new(button(
                "Apply tile move",
                |p: &mut TreePage, _| finish_preview(p, true),
            )));
            children.push(Box::new(button(
                "Cancel tile move",
                |p: &mut TreePage, _| finish_preview(p, false),
            )));
        }
        children.push(Box::new(button(
            "Retry workbench save",
            |p: &mut TreePage, _| {
                if let Some(f) = &mut p.forme {
                    f.save();
                }
            },
        )));
        if !pane.status.is_empty() {
            children.push(Box::new(
                el("p", pane.status.clone()).attr("role", "status"),
            ));
        }
    }
    for field in fields {
        let id = field.id;
        children.push(Box::new(el("h3", field.name.clone())));
        children.push(Box::new(el("p", format!("Extent: {:?}", field.extent))));
        children.push(Box::new(button(
            "Locate field",
            move |p: &mut TreePage, _| {
                p.shared.canvas.borrow_mut().center_on_field(id);
                p.shared.dirty.set(true);
            },
        )));
        children.push(Box::new(button(
            if canvas.field_visible(id) {
                "Hide field"
            } else {
                "Show field"
            },
            move |p: &mut TreePage, _| {
                p.shared.canvas.borrow_mut().toggle_field_visible(id);
                p.shared.dirty.set(true);
            },
        )));
    }
    if children.len() == 1 {
        children.push(Box::new(el("p", "No fields in this view")));
    }
    Box::new(
        el("section", children)
            .attr("class", "tools-section")
            .attr("aria-label", "Fields"),
    )
}

pub(super) fn workbench(page: &TreePage) -> Option<Child> {
    let pane = page.forme.as_ref().filter(|p| p.workbench)?;
    let graph = page.shared.canvas.borrow().graph().clone();
    let tree = pane
        .candidate
        .as_ref()
        .unwrap_or(&pane.model)
        .tile_tree(&graph);
    Some(match tree {
        None => Box::new(el(
            "p",
            "Select nodes in the Mere and open them in the workbench",
        )),
        Some(tree) => {
            let moving = pane.moving;
            let locked = pane.model.locked;
            let reading = move |tile: &Tile| -> Slot<TreePage, ()> {
                let ContentSource::Open { id, .. } = &tile.content else {
                    return Slot::Hole;
                };
                let node = id
                    .parse::<Uuid>()
                    .ok()
                    .and_then(|id| graph.get_node_by_id(id).map(|(_, n)| n));
                let Some(node) = node else {
                    return Slot::View(Box::new(el("p", "This access is no longer present")));
                };
                let member = node.id;
                let title = node.title.clone();
                let address = node.url().to_string();
                let mut controls: Vec<Child> = vec![
                    Box::new(el("h2", title.clone())),
                    Box::new(el("p", address).attr("class", "reading-address")),
                    Box::new(button("Show in Mere", move |p: &mut TreePage, _| {
                        p.shared.canvas.borrow_mut().select_member(member);
                        if let Some(f) = &mut p.forme {
                            f.workbench = false;
                        }
                        selection_changed(p);
                        p.shared.dirty.set(true);
                    })),
                ];
                let mut tags: Vec<_> = node.tags.iter().cloned().collect();
                tags.sort();
                if !tags.is_empty() {
                    controls.push(Box::new(el("p", tags.join(" · "))));
                }
                if let Some(body) = &node.body {
                    controls.push(Box::new(
                        el("pre", body.chars().take(16_384).collect::<String>())
                            .attr("class", "reading-body"),
                    ));
                }
                if !locked {
                    controls.push(Box::new(
                        button("Move tile", move |p: &mut TreePage, _| {
                            begin_move(p, member);
                        })
                        .attr("data-forme-move", "true"),
                    ));
                    if moving.is_some_and(|m| m != member) {
                        for (label, edge) in [
                            ("Preview left", workbench::Edge::Left),
                            ("Preview right", workbench::Edge::Right),
                            ("Preview above", workbench::Edge::Top),
                            ("Preview below", workbench::Edge::Bottom),
                        ] {
                            controls.push(Box::new(
                                button(label, move |p: &mut TreePage, _| preview(p, member, edge))
                                    .attr("data-forme-preview", format!("{edge:?}").to_lowercase()),
                            ));
                        }
                    }
                }
                Slot::View(Box::new(
                    el("article", controls)
                        .attr("class", "reading-pane")
                        .attr("aria-label", format!("Reading access {title}"))
                        .attr("data-forme-access", member.to_string())
                        .attr("data-forme-address", node.url().to_string()),
                ))
            };
            Box::new(
                el("div", frisket_with(&tree, tile_event, reading))
                    .attr("class", "forme-workbench"),
            )
        },
    })
}
pub(super) const SHEET: &str = " .forme-workbench { width:100%;height:100%;min-height:0; } \
    .forme-workbench .frisket-content { background:inherit; } .reading-pane { padding:16px;color:inherit;height:100%;overflow:auto; } \
    .reading-address { overflow-wrap:anywhere; } .reading-body { white-space:pre-wrap;overflow-wrap:anywhere; } ";
