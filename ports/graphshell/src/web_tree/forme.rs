// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Workbench reading tiles and their scoped Forme draft in the retained page.
use super::*;
use cambium::{Slot, button, frisket_with};
use graphshell::{
    forme_session::{FormeSession, PreparedSave, SaveKind},
    forme_workspace::FormeWorkspace,
};
use muniment::IndexedDbBackend;
use uuid::Uuid;
use workbench::{ContentSource, SplitAxis, Tile, TileEvent};

pub(super) struct Pane {
    pub(super) session: FormeSession,
    backend: IndexedDbBackend,
    completed: Rc<RefCell<Option<Result<(), String>>>>,
    pending: Option<PreparedSave>,
    retry: Option<SaveKind>,
    pub(super) saving: bool,
    pub(super) status: String,
    pub(super) workbench: bool,
    moving: Option<Uuid>,
    candidate: Option<FormeWorkspace>,
    drag: Option<(Uuid, (f32, f32), bool)>,
    cancelled_drag: bool,
}
impl Pane {
    pub(super) async fn open(product: &product::SavedProduct) -> Result<Self, String> {
        let (backend, session) = product.forme_source().ok_or("workspace store is busy")?;
        let session = FormeSession::load(backend.clone(), session, &product.graph()).await?;
        Ok(Self {
            session,
            backend,
            completed: Rc::new(RefCell::new(None)),
            pending: None,
            retry: None,
            saving: false,
            status: String::new(),
            workbench: false,
            moving: None,
            candidate: None,
            drag: None,
            cancelled_drag: false,
        })
    }
    pub(super) fn model(&self) -> &FormeWorkspace {
        self.session.view()
    }
    pub(super) fn current(&self) -> &FormeWorkspace {
        self.candidate.as_ref().unwrap_or_else(|| self.model())
    }
    pub(super) fn install(&self, canvas: &mut Canvas) -> Result<(), String> {
        canvas.set_forme_region(
            (!self.current().members().is_empty()).then(|| self.current().region()),
        )
    }
    fn save(&mut self, kind: SaveKind) {
        if self.saving {
            return;
        }
        let proposal = match self.session.prepare_save(kind, now()) {
            Ok(proposal) => proposal,
            Err(error) => {
                self.status = error;
                return;
            },
        };
        self.saving = true;
        self.status = "Saving workbench…".into();
        self.retry = Some(kind);
        self.pending = Some(proposal.clone());
        let backend = self.backend.clone();
        let completed = self.completed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            *completed.borrow_mut() = Some(proposal.save(backend).await);
        });
    }
    pub(super) fn previewing(&self) -> bool {
        self.candidate.is_some()
    }
    pub(super) fn ready(&self) -> bool {
        self.completed.borrow().is_some()
    }
    fn poll(&mut self) {
        let Some(result) = self.completed.borrow_mut().take() else {
            return;
        };
        self.saving = false;
        if let Some(proposal) = self.pending.take() {
            self.status = match self.session.finish_save(proposal, result) {
                Ok(()) => {
                    self.retry = None;
                    "Workbench saved".into()
                },
                Err(error) => format!("Save failed · {error} · Draft retained; retry the save"),
            };
        }
    }
}
fn now() -> u64 {
    js_sys::Date::now() as u64
}
pub(super) fn poll(page: &mut TreePage) {
    if let Some(pane) = &mut page.forme {
        pane.poll();
    }
    changed(page);
}
fn changed(page: &mut TreePage) {
    if let Some(pane) = &mut page.forme {
        pane.candidate = None;
        pane.moving = None;
        pane.drag = None;
        if let Err(error) = pane.install(&mut page.shared.canvas.borrow_mut()) {
            pane.status = error;
        }
    }
    page.shared.dirty.set(true);
}
fn edit(
    page: &mut TreePage,
    key: Option<String>,
    auto_open: bool,
    change: impl FnOnce(&mut FormeWorkspace, &mere::kernel::graph::Graph) -> Result<(), String>,
) {
    let Some(pane) = &mut page.forme else {
        return;
    };
    if pane.saving {
        return;
    }
    let immediate = !pane.session.editing() && auto_open;
    if immediate {
        let _ = pane.session.unlock();
    }
    let result = pane.session.edit(key, now(), |workspace| {
        change(workspace, page.shared.canvas.borrow().graph())
    });
    match result {
        Ok(()) => {
            pane.status = if pane.session.dirty() {
                "Pending Forme changes"
            } else {
                "Forme draft matches saved arrangement"
            }
            .into();
            if immediate {
                pane.save(SaveKind::Apply);
            }
        },
        Err(error) => {
            if immediate {
                let _ = pane.session.discard();
            }
            pane.status = error;
        },
    }
    changed(page);
}
fn open_selected(page: &mut TreePage) {
    let selected = page.shared.canvas.borrow().selected_members();
    if selected.is_empty() {
        if let Some(pane) = &mut page.forme {
            pane.status = "Select a node to open it in the workbench".into();
        }
        return;
    }
    edit(page, None, true, |workspace, graph| {
        for member in selected {
            workspace.open(member, graph)?;
        }
        Ok(())
    });
    if let Some(pane) = &mut page.forme {
        pane.workbench = true;
    }
}
fn arrange(page: &mut TreePage, axis: Option<SplitAxis>) {
    edit(page, None, false, |workspace, _| {
        let mut layout = workspace.layout();
        match axis {
            None => layout.stack_all(),
            Some(SplitAxis::Row) => layout.split_all(),
            Some(SplitAxis::Column) => {
                for pair in layout.open_members().windows(2) {
                    layout.split_beside_axis(pair[1], pair[0], SplitAxis::Column, true);
                }
            },
        }
        workspace.keep_layout(&layout);
        Ok(())
    });
}
fn tile_event(page: &mut TreePage, event: TileEvent) {
    let activated = if let TileEvent::Activated(tile) = &event {
        page.forme
            .as_ref()
            .and_then(|p| p.model().member_for_tile(*tile))
    } else {
        None
    };
    let key = match &event {
        TileEvent::DividerMoved { split, .. } => Some(format!("divider:{:?}", split.0)),
        _ => None,
    };
    edit(
        page,
        key,
        matches!(event, TileEvent::Activated(_) | TileEvent::Closed(_)),
        |workspace, graph| workspace.event(event, graph),
    );
    if let Some(member) = activated {
        page.shared.canvas.borrow_mut().select_member(member);
        selection_changed(page);
    }
}
fn translate(page: &mut TreePage, dx: f32, dy: f32, scale: f32) {
    edit(page, None, false, |workspace, _| {
        let [x, y, r, b] = workspace.bounds;
        let (cx, cy) = (x + (r - x) / 2. + dx, y + (b - y) / 2. + dy);
        let (w, h) = (((r - x) * scale).max(80.), ((b - y) * scale).max(80.));
        workspace.bounds = [cx - w / 2., cy - h / 2., cx + w / 2., cy + h / 2.];
        Ok(())
    });
}
fn selection_changed(page: &mut TreePage) {
    let canvas = page.shared.canvas.borrow();
    page.picked = canvas.focused_url().map(str::to_owned);
    if let Some(product) = &mut page.product {
        product.select(canvas.selected_members().first().copied());
    }
}
fn begin_move(page: &mut TreePage, member: Uuid) {
    if let Some(pane) = &mut page.forme {
        if pane.saving || !pane.session.editing() {
            return;
        }
        pane.candidate = None;
        pane.moving = Some(member);
        pane.session.break_gesture();
        let _ = pane.install(&mut page.shared.canvas.borrow_mut());
    }
    page.shared.dirty.set(true);
}
fn preview(page: &mut TreePage, target: Uuid, edge: workbench::Edge) {
    let Some(pane) = &mut page.forme else {
        return;
    };
    if pane.saving || !pane.session.editing() {
        return;
    }
    let Some(moving) = pane.moving else {
        return;
    };
    let mut candidate = pane.model().clone();
    let (Some(tile), Some(to)) = (
        candidate.tile_for_member(moving),
        candidate.tile_for_member(target),
    ) else {
        return;
    };
    let result = candidate.event(
        TileEvent::Dragged {
            tile,
            to: workbench::DropTarget::Edge { tile: to, edge },
        },
        page.shared.canvas.borrow().graph(),
    );
    match result.and_then(|()| {
        page.shared
            .canvas
            .borrow_mut()
            .set_forme_region(Some(candidate.region()))
    }) {
        Ok(()) => {
            pane.candidate = Some(candidate);
            pane.status = "Preview ready · Apply move or cancel".into();
        },
        Err(error) => pane.status = error,
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
    let candidate = pane.candidate.take();
    pane.moving = None;
    pane.cancelled_drag |= !apply && pane.drag.is_some();
    pane.drag = None;
    if apply && let Some(candidate) = candidate {
        edit(page, None, false, |workspace, _| {
            *workspace = candidate;
            Ok(())
        });
    } else {
        pane.status = "Tile move cancelled".into();
        let _ = pane.install(&mut page.shared.canvas.borrow_mut());
        page.shared.dirty.set(true);
    }
}
fn unlock_or_apply(page: &mut TreePage) {
    if let Some(pane) = &mut page.forme {
        if pane.saving {
            return;
        }
        if pane.session.editing() {
            if pane.previewing() || pane.moving.is_some() || pane.drag.is_some() {
                pane.status = "Apply or cancel the tile move before locking the forme".into();
                return;
            }
            pane.save(SaveKind::Apply);
        } else {
            match pane.session.unlock() {
                Ok(()) => pane.status = "Forme draft open · drag nodes as tile handles".into(),
                Err(e) => pane.status = e,
            }
        }
    }
    changed(page);
}
fn discard(page: &mut TreePage) {
    if let Some(pane) = &mut page.forme {
        match pane.session.discard() {
            Ok(()) => pane.status = "Forme changes discarded".into(),
            Err(e) => pane.status = e,
        }
    }
    changed(page);
}
fn undo(page: &mut TreePage, redo: bool) {
    if page
        .forme
        .as_ref()
        .is_some_and(|pane| pane.previewing() || pane.drag.is_some() || pane.moving.is_some())
    {
        finish_preview(page, false);
        return;
    }
    if let Some(pane) = &mut page.forme {
        if pane.session.editing() {
            match pane.session.draft_undo(redo) {
                Ok(true) => pane.status = "Forme draft history restored".into(),
                Ok(false) => return,
                Err(e) => pane.status = e,
            }
        } else {
            pane.save(if redo { SaveKind::Redo } else { SaveKind::Undo });
        }
    }
    changed(page);
}
pub(super) fn shortcut(page: &mut TreePage, event: &cambium::KeyEvent) -> bool {
    if page.forme.is_none() || event.prop.default_prevented() {
        return false;
    }
    if matches!(event.key, Key::Named(NamedKey::Escape))
        && page
            .forme
            .as_ref()
            .is_some_and(|p| p.previewing() || p.drag.is_some())
    {
        finish_preview(page, false);
        return true;
    }
    if (event.mods.ctrl || event.mods.meta)
        && !event.mods.alt
        && let Key::Character(key) = &event.key
    {
        if key.eq_ignore_ascii_case("z") {
            undo(page, event.mods.shift);
            return true;
        }
        if key.eq_ignore_ascii_case("y") {
            undo(page, true);
            return true;
        }
    }
    false
}
/// An unlocked Forme explicitly turns graph nodes into tile handles. A
/// click remains selection; only motion past the threshold previews a drop.
pub(super) fn pointer(page: &mut TreePage, event: &cambium::PointerEvent) -> bool {
    let Some(pane) = &mut page.forme else {
        return false;
    };
    if pane.cancelled_drag && event.phase != cambium::PointerPhase::Down {
        if event.phase == cambium::PointerPhase::Up {
            pane.cancelled_drag = false;
        }
        return true;
    }
    if !pane.session.editing() || pane.saving {
        return false;
    }
    match event.phase {
        cambium::PointerPhase::Down => {
            pane.cancelled_drag = false;
            let Some(member) = page
                .shared
                .canvas
                .borrow()
                .node_at_screen(event.local.0, event.local.1)
            else {
                return false;
            };
            pane.candidate = None;
            pane.moving = None;
            pane.session.break_gesture();
            pane.drag = Some((member, event.local, false));
        },
        cambium::PointerPhase::Move => {
            let Some((member, start, active)) = &mut pane.drag else {
                return false;
            };
            *active |= (event.local.0 - start.0).hypot(event.local.1 - start.1) >= 4.;
            if !*active {
                return true;
            }
            let canvas = page.shared.canvas.borrow();
            let point = canvas.world_point_at(event.local);
            let preview = pane.session.drop_preview(*member, point, canvas.graph());
            drop(canvas);
            match preview {
                Ok(candidate) => {
                    pane.candidate = candidate;
                    pane.status = if pane.candidate.is_some() {
                        "Drop to edit the Forme draft · Escape cancels"
                    } else {
                        "Outside a drop region · release to cancel"
                    }
                    .into();
                },
                Err(error) => {
                    pane.candidate = None;
                    pane.status = error;
                },
            }
            let _ = pane.install(&mut page.shared.canvas.borrow_mut());
            event.defer_rebuild();
        },
        cambium::PointerPhase::Up => {
            let Some((member, _start, active)) = pane.drag.take() else {
                return false;
            };
            if active {
                // Resolve the release point as well; a final release outside
                // the field must not accept the last in-bounds preview.
                let canvas = page.shared.canvas.borrow();
                let preview = pane.session.drop_preview(
                    member,
                    canvas.world_point_at(event.local),
                    canvas.graph(),
                );
                drop(canvas);
                pane.candidate = preview.ok().flatten();
                finish_preview(page, true);
            } else {
                let mut canvas = page.shared.canvas.borrow_mut();
                canvas.pointer_down(
                    mere::canvas::PointerButton::Left,
                    event.local.0,
                    event.local.1,
                );
                canvas.pointer_up(
                    mere::canvas::PointerButton::Left,
                    event.local.0,
                    event.local.1,
                );
                drop(canvas);
                selection_changed(page);
            }
        },
    }
    page.shared.dirty.set(true);
    true
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
                let Some(f) = &mut p.forme else {
                    return;
                };
                if f.saving || f.model().locked {
                    return;
                }
                if let Some(mut shares) = f.model().layout().split_fractions(&at) {
                    f.session.break_gesture();
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
        let forme_start = children.len();
        children.push(Box::new(el(
            "p",
            format!(
                "Workbench · {} accesses · {} regions",
                pane.model().members().len(),
                pane.model().region().cells.len()
            ),
        )));
        let pins = pane
            .current()
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
            if pane.session.editing() {
                "Lock and apply"
            } else {
                "Unlock forme"
            },
            |p: &mut TreePage, _| unlock_or_apply(p),
        )));
        if pane.session.editing() {
            children.push(Box::new(
                el(
                    "p",
                    if pane.session.dirty() {
                        "Pending Forme changes"
                    } else {
                        "Forme draft matches saved arrangement"
                    },
                )
                .attr("role", "status"),
            ));
            children.push(Box::new(button(
                "Discard changes",
                |p: &mut TreePage, _| discard(p),
            )));
        }
        for (label, redo, enabled) in [
            (
                if pane.session.editing() {
                    "Undo forme draft"
                } else {
                    "Undo saved forme change"
                },
                false,
                pane.session.can_undo(),
            ),
            (
                if pane.session.editing() {
                    "Redo forme draft"
                } else {
                    "Redo saved forme change"
                },
                true,
                pane.session.can_redo(),
            ),
        ] {
            children.push(Box::new(
                cambium::focusable_if(
                    cambium::on_click(el("button", label), move |p: &mut TreePage, _| {
                        if enabled {
                            undo(p, redo);
                        }
                    }),
                    enabled,
                )
                .attr("aria-disabled", (!enabled).to_string()),
            ));
        }
        children.push(Box::new(button(
            if pane.model().visible {
                "Hide forme boundary"
            } else {
                "Show forme boundary"
            },
            |p: &mut TreePage, _| {
                if let Some(f) = &mut p.forme {
                    if f.saving {
                        return;
                    }
                    f.save(SaveKind::Visibility);
                }
                p.shared.dirty.set(true);
            },
        )));
        children.push(Box::new(button(
            "Select forme members",
            |p: &mut TreePage, _| {
                if let Some(f) = &p.forme {
                    let mut canvas = p.shared.canvas.borrow_mut();
                    canvas.clear_selection();
                    for m in f.model().members() {
                        canvas.toggle_select_member(m);
                    }
                }
                selection_changed(p);
                p.shared.dirty.set(true);
            },
        )));
        if !pane.model().locked {
            if let Some(tree) = pane.model().tile_tree(canvas.graph()) {
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
                    if let Some(kind) = f.retry {
                        f.save(kind);
                    }
                }
            },
        )));
        if !pane.status.is_empty() {
            children.push(Box::new(
                el("p", pane.status.clone()).attr("role", "status"),
            ));
        }
        let forme_controls = children.split_off(forme_start);
        children.push(Box::new(cambium::on_key(
            el("section", forme_controls).attr("aria-label", "Forme controls"),
            |p: &mut TreePage, event| {
                if shortcut(p, &event) {
                    event.prevent_default();
                    event.stop_propagation();
                }
            },
        )));
    }
    // Only Forme chrome owns these shortcuts; another field's controls keep
    // their own edit/history routing.
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
        .unwrap_or_else(|| pane.model())
        .tile_tree(&graph);
    Some(match tree {
        None => Box::new(el(
            "p",
            "Select nodes in the Mere and open them in the workbench",
        )),
        Some(tree) => {
            let moving = pane.moving;
            let locked = pane.model().locked;
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
                cambium::on_key(
                    el("div", frisket_with(&tree, tile_event, reading)),
                    |p: &mut TreePage, event| {
                        if shortcut(p, &event) {
                            event.prevent_default();
                            event.stop_propagation();
                        }
                    },
                )
                .attr("class", "forme-workbench"),
            )
        },
    })
}
pub(super) const SHEET: &str = " .forme-workbench { width:100%;height:100%;min-height:0; } \
    .forme-workbench .frisket-content { background:inherit; } .reading-pane { padding:16px;color:inherit;height:100%;overflow:auto; } \
    .reading-pane button { padding:5px 10px;border:1px solid currentColor; } \
    .reading-address { overflow-wrap:anywhere; } .reading-body { white-space:pre-wrap;overflow-wrap:anywhere; } ";
