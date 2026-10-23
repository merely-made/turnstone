// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A small host adapter for Cambium's shared scope route and visit history.
use super::*;
use cambium::scope_navigation::*;
use uuid::Uuid;
type Navigation = ScopeNavigation<Uuid, Uuid, tabard::ReadingAnchor>;
type Visit = ScopeVisit<Uuid, Uuid, tabard::ReadingAnchor>;
#[derive(Default)]
pub(super) struct State {
    owner: Option<Uuid>,
    navigation: Navigation,
    hint: Option<ScopeDirection>,
    pending: std::collections::VecDeque<ScopeCommand>,
    dirty: bool,
    restoring: bool,
}
fn owner(page: &TreePage) -> Option<Uuid> {
    page.forme.as_ref().map(|p| p.model().document.graph_id)
}
pub(super) fn place(page: &TreePage) -> Option<ScopePlace<Uuid, Uuid>> {
    let graph = owner(page)?;
    let pane = page.forme.as_ref()?;
    let mut place = page
        .instrument
        .scope
        .navigation
        .current()
        .map(|v| v.place.clone())
        .filter(|p| p.graph == graph)
        .unwrap_or(ScopePlace {
            level: ScopeLevel::Graph,
            graph,
            root: None,
            focus: None,
        });
    if pane.workbench {
        let focus = pane.current().focused_reading;
        if focus.is_none() {
            place.level = ScopeLevel::Graph;
            place.focus = None;
            return Some(place);
        }
        if place.level != ScopeLevel::Document || place.focus != focus {
            if place.level != ScopeLevel::Neighborhood {
                place.root = focus;
            }
            place.level = ScopeLevel::Document;
            place.focus = focus;
        }
    } else if place.level != ScopeLevel::Overmap {
        if place.level == ScopeLevel::Document {
            place.level = ScopeLevel::Graph;
        }
        if let Some(focus) = page.shared.canvas.borrow().focused_member() {
            place.select(focus);
        }
    }
    Some(place)
}
fn capture(page: &TreePage) -> Option<Visit> {
    let place = place(page)?;
    let camera = page.shared.canvas.borrow().camera();
    let anchor = place.focus.and_then(|member| {
        super::reading::capture_progress(page)
            .0
            .get(&member)
            .cloned()
    });
    Some(ScopeVisit {
        place,
        camera: Some(ScopeCamera {
            x: camera.offset.0,
            y: camera.offset.1,
            zoom: camera.zoom,
        }),
        anchor,
    })
}
pub(super) fn initialize(page: &mut TreePage) {
    let Some(graph) = owner(page) else {
        return;
    };
    if page.instrument.scope.owner != Some(graph) {
        let saved = page
            .forme
            .as_ref()
            .unwrap()
            .model()
            .scope_navigation
            .clone();
        page.instrument.scope = State {
            owner: Some(graph),
            navigation: saved,
            ..Default::default()
        };
        if let Some(mut visit) = page.instrument.scope.navigation.current().cloned() {
            if visit.place.level == ScopeLevel::Document {
                if let Some(anchor) = visit
                    .place
                    .focus
                    .and_then(|m| {
                        page.forme
                            .as_ref()
                            .and_then(|p| p.model().reading_anchors.get(&m))
                    })
                    .cloned()
                {
                    visit.anchor = Some(anchor);
                }
                page.instrument.scope.navigation.capture(visit.clone());
            }
            page.instrument.scope.restoring = true;
            apply(page, &visit);
            page.instrument.scope.restoring = false;
        } else if let Some(visit) = capture(page) {
            page.instrument.scope.navigation.capture(visit);
        }
    }
}
pub(super) fn record_reading(page: &mut TreePage, member: Uuid) {
    if page.instrument.scope.restoring {
        return;
    }
    initialize(page);
    let Some(mut visit) = capture(page) else {
        return;
    };
    visit.place.root = page
        .instrument
        .scope
        .navigation
        .current()
        .filter(|v| v.place.level == ScopeLevel::Neighborhood)
        .and_then(|v| v.place.root)
        .or(Some(member));
    visit.place.level = ScopeLevel::Document;
    visit.place.focus = Some(member);
    visit.anchor = super::reading::capture_progress(page)
        .0
        .get(&member)
        .cloned();
    page.instrument.scope.navigation.visit(visit);
    page.instrument.scope.dirty = true;
}
pub(super) fn capture_origin(page: &mut TreePage) {
    if page.instrument.scope.restoring {
        return;
    }
    initialize(page);
    if let Some(visit) = capture(page) {
        page.instrument.scope.navigation.capture(visit);
    }
}
pub(super) fn retain_anchor(
    page: &mut TreePage,
    anchors: &std::collections::BTreeMap<Uuid, tabard::ReadingAnchor>,
) {
    let Some(mut visit) = page
        .instrument
        .scope
        .navigation
        .current()
        .cloned()
        .filter(|v| v.place.level == ScopeLevel::Document)
    else {
        return;
    };
    if let Some(anchor) = visit.place.focus.and_then(|m| anchors.get(&m)) {
        if visit.anchor.as_ref() != Some(anchor) {
            visit.anchor = Some(anchor.clone());
            page.instrument.scope.navigation.capture(visit);
            page.instrument.scope.dirty = true;
        }
    }
}
fn label(page: &TreePage, place: &ScopePlace<Uuid, Uuid>) -> String {
    let canvas = page.shared.canvas.borrow();
    let title = |member: Option<Uuid>| {
        member
            .and_then(|id| {
                canvas
                    .graph()
                    .get_node_by_id(id)
                    .map(|(key, _)| canvas.graph().node_display_label(key))
            })
            .unwrap_or_else(|| "document".into())
    };
    match place.level {
        ScopeLevel::Document => title(place.focus),
        ScopeLevel::Neighborhood => format!("Neighborhood of {}", title(place.root)),
        ScopeLevel::Graph => "Local Mere".into(),
        ScopeLevel::Overmap => "Meres".into(),
    }
}
pub(super) fn controls(page: &TreePage) -> Child {
    let nav = &page.instrument.scope.navigation;
    let place = place(page);
    let snapshot = ScopeControls {
        back: nav.destination(false).map(|v| label(page, &v.place)),
        forward: nav.destination(true).map(|v| label(page, &v.place)),
        up: place.as_ref().and_then(|p| p.up()).map(|p| label(page, &p)),
        down: place
            .as_ref()
            .and_then(|p| p.down())
            .map(|p| label(page, &p)),
        hint: page.instrument.scope.hint,
    };
    cambium::scope_navigation::scope_controls(snapshot, command)
}
pub(super) fn trail(page: &TreePage) -> Option<Child> {
    let place = place(page)?;
    let crumbs = [
        ScopeLevel::Overmap,
        ScopeLevel::Graph,
        ScopeLevel::Neighborhood,
        ScopeLevel::Document,
    ]
    .into_iter()
    .filter(|l| *l as u8 >= place.level as u8)
    .map(|level| ScopeCrumb {
        label: if level == ScopeLevel::Neighborhood {
            "Neighborhood".into()
        } else {
            label(
                page,
                &ScopePlace {
                    level,
                    ..place.clone()
                },
            )
        },
        level,
        current: level == place.level,
    })
    .collect();
    Some(cambium::scope_navigation::scope_trail(crumbs, command))
}
pub(super) fn command(page: &mut TreePage, command: ScopeCommand) {
    if let ScopeCommand::Hint(hint) = command {
        page.instrument.scope.hint = hint;
        page.shared.dirty.set(true);
        return;
    }
    if page.product.as_ref().is_none_or(|p| p.selection_locked()) {
        return;
    }
    initialize(page);
    if page
        .forme
        .as_ref()
        .is_none_or(|p| p.saving || p.retry.is_some())
    {
        if page.instrument.scope.pending.len() < 32 {
            page.instrument.scope.pending.push_back(command);
        }
        return;
    }
    let Some(origin) = capture(page) else {
        return;
    };
    page.instrument.scope.navigation.capture(origin.clone());
    let history = match command {
        ScopeCommand::Navigate(ScopeDirection::Back) => Some(false),
        ScopeCommand::Navigate(ScopeDirection::Forward) => Some(true),
        _ => None,
    };
    let target = if let Some(forward) = history {
        page.instrument
            .scope
            .navigation
            .destination(forward)
            .cloned()
    } else {
        let place = match command {
            ScopeCommand::Navigate(ScopeDirection::Up) => origin.place.up(),
            ScopeCommand::Navigate(ScopeDirection::Down) => origin.place.down(),
            ScopeCommand::To(level) if level != origin.place.level => Some(ScopePlace {
                level,
                ..origin.place.clone()
            }),
            _ => None,
        };
        place.map(|place| {
            let saved = page.instrument.scope.navigation.saved(&place);
            ScopeVisit {
                camera: saved.and_then(|v| v.camera),
                anchor: saved.and_then(|v| v.anchor.clone()),
                place,
            }
        })
    };
    let Some(target) = target else {
        return;
    };
    page.instrument.scope.restoring = true;
    let applied = apply(page, &target);
    page.instrument.scope.restoring = false;
    if !applied {
        return;
    }
    if let Some(forward) = history {
        page.instrument.scope.navigation.step(forward);
    } else {
        page.instrument.scope.navigation.visit(target);
    }
    page.instrument.scope.hint = None;
    page.instrument.scope.dirty = true;
    page.shared.dirty.set(true);
}
fn apply(page: &mut TreePage, visit: &Visit) -> bool {
    if owner(page) != Some(visit.place.graph) {
        return false;
    }
    let p = &visit.place;
    if !page
        .shared
        .canvas
        .borrow()
        .graph()
        .nodes()
        .any(|(_, n)| Some(n.id) == p.focus)
        && p.focus.is_some()
    {
        return false;
    }
    if p.level == ScopeLevel::Document {
        let Some(member) = p.focus else {
            return false;
        };
        super::forme::read_related(page, member);
        if let Some(anchor) = visit.anchor.clone().or_else(|| {
            page.forme
                .as_ref()
                .and_then(|p| p.model().reading_anchors.get(&member).cloned())
        }) {
            super::reading::restore_anchor(page, member, anchor);
        }
    } else {
        if let Some(pane) = &mut page.forme {
            pane.workbench = false;
        }
        let mut canvas = page.shared.canvas.borrow_mut();
        if p.level == ScopeLevel::Neighborhood {
            let Some(root) = p.root.or(p.focus) else {
                return false;
            };
            if canvas.graph().get_node_by_id(root).is_none() {
                return false;
            }
            let Some(members) = mere::navigation::neighborhood_members(canvas.graph(), root) else {
                return false;
            };
            canvas.scope_to_exact_members(members);
        } else {
            canvas.clear_scope();
        }
        if let Some(member) = p.focus {
            canvas.select_member(member);
        }
        if let Some(camera) = visit.camera {
            canvas.set_camera(mere::canvas::CameraView {
                offset: (camera.x, camera.y),
                zoom: camera.zoom,
            });
        } else {
            canvas.fit_visible();
        }
    }
    if p.level != ScopeLevel::Document {
        super::forme::selection_changed(page);
    }
    true
}
pub(super) fn refresh(page: &TreePage) {
    let Some(place) = place(page).filter(|p| p.level == ScopeLevel::Neighborhood) else {
        return;
    };
    let Some(root) = place.root.or(place.focus) else {
        return;
    };
    let mut canvas = page.shared.canvas.borrow_mut();
    let Some(members) = mere::navigation::neighborhood_members(canvas.graph(), root) else {
        return;
    };
    if canvas.scope_members().as_ref() != Some(&members) {
        canvas.scope_to_exact_members(members);
        page.shared.dirty.set(true);
    }
}
pub(super) fn needs_poll(page: &TreePage) -> bool {
    owner(page).is_some()
        && (page.instrument.scope.owner != owner(page)
            || page
                .forme
                .as_ref()
                .is_some_and(|p| !p.saving && p.retry.is_none())
                && (page.instrument.scope.dirty || !page.instrument.scope.pending.is_empty()))
}
pub(super) fn poll(page: &mut TreePage) {
    initialize(page);
    if page
        .forme
        .as_ref()
        .is_none_or(|p| p.saving || p.retry.is_some())
    {
        return;
    }
    if let Some(command) = page.instrument.scope.pending.pop_front() {
        self::command(page, command);
    }
    if page.instrument.scope.dirty && page.forme.as_ref().is_some_and(|p| !p.saving) {
        let nav = page.instrument.scope.navigation.clone();
        page.forme
            .as_mut()
            .unwrap()
            .save(graphshell::forme_session::SaveKind::Scope(nav));
        page.instrument.scope.dirty = false;
    }
}
pub(super) fn overmap(page: &TreePage) -> Option<Child> {
    if place(page)?.level != ScopeLevel::Overmap {
        return None;
    }
    let count = page.shared.canvas.borrow().graph().nodes().count();
    Some(Box::new(
        el(
            "div",
            (
                el("h2", "Meres"),
                cambium::shell_navigation_item(
                    "Local Mere",
                    format!("Current local dataspace · {count} documents"),
                    Default::default(),
                    |page: &mut TreePage, _| {
                        command(page, ScopeCommand::Navigate(ScopeDirection::Down))
                    },
                ),
            ),
        )
        .attr("class", "scope-overmap"),
    ))
}
pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    snapshot
        .with_field(
            "scope-level",
            place(page).map(|p| p.level.label()).unwrap_or("Graph"),
        )
        .with_field(
            "scope-focus",
            place(page)
                .and_then(|p| p.focus)
                .map(|m| m.to_string())
                .unwrap_or_default(),
        )
        .with_field(
            "scope-back",
            page.instrument
                .scope
                .navigation
                .destination(false)
                .is_some()
                .to_string(),
        )
        .with_field(
            "scope-forward",
            page.instrument
                .scope
                .navigation
                .destination(true)
                .is_some()
                .to_string(),
        )
}
