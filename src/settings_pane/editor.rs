// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Queue-only controls for the authored Settings arrangement.
//!
//! The event callbacks in this module record intent. The pane host performs
//! validation, route preparation, confirmation, and publication later.

use std::collections::{BTreeSet, VecDeque};

use cambium::composition::{ComponentKey, ContainerKey, ContainerKind, PlacementNode};
use cambium::{PointerClick, clickable, el};
use uuid::Uuid;

mod container_view;
pub(super) use container_view::container_toggle_controls;

use super::arrangement::ArrangementOperation;
use super::{SettingsState, SettingsView};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConfirmationDecision {
    Apply,
    Discard,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ConfirmationTarget {
    Delete(ComponentKey),
    ReloadSaved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EditConfirmation {
    pub target: ConfirmationTarget,
    pub base_revision: u64,
}

#[derive(Clone, Debug)]
pub(super) enum EditorIntent {
    ToggleEditor,
    Structure(ArrangementOperation),
    RequestDelete(ComponentKey),
    ConfirmDelete { decision: ConfirmationDecision },
    Undo,
    Redo,
    ToggleContainer(ContainerKey),
    Save,
    ReloadSaved,
}

#[derive(Default)]
pub(super) struct EditorState {
    pub enabled: bool,
    pub queue: VecDeque<EditorIntent>,
    pub confirmation: Option<EditConfirmation>,
    pub unsaved: bool,
    pub save_request: Option<Uuid>,
    pub notice: String,
}

impl EditorState {
    pub(super) fn enqueue(&mut self, intent: EditorIntent) {
        self.queue.push_back(intent);
    }

    pub(super) fn take_intents(&mut self) -> impl Iterator<Item = EditorIntent> + '_ {
        self.queue.drain(..)
    }
}

pub(super) fn editor_toolbar(state: &SettingsState) -> SettingsView {
    let disabled_save = !state.editor.unsaved || state.editor.save_request.is_some();
    let disabled_undo = !state.arrangement.can_undo();
    let disabled_redo = !state.arrangement.can_redo();
    let mut controls: Vec<SettingsView> = vec![
        action_button(
            if state.editor.enabled {
                "Finish arranging"
            } else {
                "Edit arrangement"
            },
            "toggle-editor",
            None,
            false,
            |s| s.editor.enqueue(EditorIntent::ToggleEditor),
        ),
        action_button("Undo", "undo", None, disabled_undo, |s| {
            s.editor.enqueue(EditorIntent::Undo)
        }),
        action_button("Redo", "redo", None, disabled_redo, |s| {
            s.editor.enqueue(EditorIntent::Redo)
        }),
        action_button("Save arrangement", "save", None, disabled_save, |s| {
            s.editor.enqueue(EditorIntent::Save)
        }),
        action_button("Reload saved", "reload-saved", None, false, |s| {
            s.editor.enqueue(EditorIntent::ReloadSaved)
        }),
    ];
    if !state.editor.notice.is_empty() {
        controls.push(Box::new(
            el::<_, SettingsState, ()>("span", state.editor.notice.clone())
                .attr("role", "status")
                .attr("aria-live", "polite"),
        ));
    }
    if let Some(confirmation) = &state.editor.confirmation {
        let pending = match &confirmation.target {
            ConfirmationTarget::Delete(key) => state
                .rows
                .get(key)
                .is_some_and(|row| row.session.snapshot().pending_generation.is_some()),
            ConfirmationTarget::ReloadSaved => false,
        };
        let prompt = match &confirmation.target {
            ConfirmationTarget::Delete(_) => {
                "Delete this setting row? Its current editor must be clean."
            },
            ConfirmationTarget::ReloadSaved => "Discard unsaved arrangement changes and reload?",
        };
        controls.push(Box::new(
            el::<_, SettingsState, ()>("span", prompt).attr("role", "alert"),
        ));
        if pending {
            controls.push(Box::new(el::<_, SettingsState, ()>(
                "span",
                "A settings write is pending. Wait for it to finish before confirming deletion.",
            ).attr("role", "status").attr("aria-live", "polite")));
        }
        let choices = match &confirmation.target {
            ConfirmationTarget::Delete(_) => [
                ("Apply edit and delete", ConfirmationDecision::Apply),
                ("Discard edit and delete", ConfirmationDecision::Discard),
                ("Cancel", ConfirmationDecision::Cancel),
            ],
            ConfirmationTarget::ReloadSaved => [
                ("Apply edits and reload", ConfirmationDecision::Apply),
                ("Discard edits and reload", ConfirmationDecision::Discard),
                ("Cancel", ConfirmationDecision::Cancel),
            ],
        };
        for (label, decision) in choices {
            controls.push(action_button(
                label,
                "confirm-edit",
                None,
                pending && decision != ConfirmationDecision::Cancel,
                move |s| {
                    s.editor.enqueue(EditorIntent::ConfirmDelete { decision });
                },
            ));
        }
    }
    Box::new(
        el::<_, SettingsState, ()>("div", controls).attr("class", "settings-arrangement-toolbar"),
    )
}

pub(super) fn row_controls(state: &SettingsState, key: &ComponentKey) -> SettingsView {
    if !state.editor.enabled {
        return Box::new(el::<_, SettingsState, ()>("span", ()));
    }
    let model = state.arrangement.accepted();
    let Some(descriptor) = model.components.iter().find(|item| &item.key == key) else {
        return Box::new(el::<_, SettingsState, ()>("span", ()));
    };
    let hidden = model.unplaced.contains(key);
    let location = find_location(&model.placement, key, Vec::new());
    let can_up = !hidden && location.as_ref().is_some_and(|(_, index)| *index > 0);
    let can_down = !hidden
        && location
            .as_ref()
            .is_some_and(|(siblings, index)| *index + 1 < siblings.len());
    let mut controls: Vec<RowControl> = vec![
        row_control(
            "Move up",
            "move-up",
            key,
            !can_up,
            EditorIntent::Structure(ArrangementOperation::MoveUp(key.clone())),
        ),
        row_control(
            "Move down",
            "move-down",
            key,
            !can_down,
            EditorIntent::Structure(ArrangementOperation::MoveDown(key.clone())),
        ),
    ];
    if hidden {
        controls.push(row_control(
            "Show",
            "show",
            key,
            false,
            EditorIntent::Structure(ArrangementOperation::Show {
                key: key.clone(),
                container: root(),
                index: usize::MAX,
            }),
        ));
    } else {
        controls.push(row_control(
            "Hide",
            "hide",
            key,
            false,
            EditorIntent::Structure(ArrangementOperation::Hide(key.clone())),
        ));
    }
    controls.push(row_control(
        "Duplicate",
        "duplicate",
        key,
        false,
        EditorIntent::Structure(ArrangementOperation::Duplicate(key.clone())),
    ));

    for destination in std::iter::once(root()).chain(containers(&model.placement)) {
        let label = if destination.kind == ContainerKind::Root {
            "Move to root".to_owned()
        } else {
            format!(
                "Move to {} {}",
                if destination.kind == ContainerKind::Group {
                    "group"
                } else {
                    "drawer"
                },
                destination.key
            )
        };
        controls.push(row_control(
            &label,
            "move-to",
            key,
            hidden,
            EditorIntent::Structure(ArrangementOperation::MoveTo {
                key: key.clone(),
                container: destination,
                index: usize::MAX,
            }),
        ));
    }

    let session = state.rows.get(key).map(|row| row.session.snapshot());
    let pending = session
        .as_ref()
        .is_some_and(|snapshot| snapshot.pending_generation.is_some());
    let dirty = session.as_ref().is_some_and(|snapshot| snapshot.dirty);
    let draft_block = dirty || pending;
    controls.push(row_control(
        if pending {
            "Delete (write pending)"
        } else if dirty {
            "Delete (draft must be settled)"
        } else {
            "Delete"
        },
        "delete",
        key,
        pending,
        EditorIntent::RequestDelete(key.clone()),
    ));
    let status = if pending {
        Some("A settings write is pending. Wait for it to finish before deleting or rebinding.")
    } else if dirty {
        Some("This row has an unapplied draft. Rebind is disabled until the draft is settled.")
    } else {
        None
    };

    let targets = state.snapshots.iter().flat_map(|(reference, snapshot)| {
        snapshot.specs.iter().map(move |entry| {
            (
                reference.clone(),
                entry.spec.id.clone(),
                entry.spec.label.clone(),
            )
        })
    });
    for (reference, setting_id, label) in targets {
        let same = descriptor
            .binding
            .get("reference")
            .and_then(serde_json::Value::as_str)
            == Some(reference.as_str())
            && descriptor
                .binding
                .get("setting_id")
                .and_then(serde_json::Value::as_str)
                == Some(setting_id.as_str());
        if !same {
            controls.push(row_control(
                &format!("Rebind: {label}"),
                "rebind",
                key,
                draft_block,
                EditorIntent::Structure(ArrangementOperation::Rebind {
                    key: key.clone(),
                    reference,
                    setting_id,
                    draft_clean: !draft_block,
                    no_pending_write: !draft_block,
                }),
            ));
        }
    }
    let mut views = controls.iter().map(row_control_view).collect::<Vec<_>>();
    if let Some(message) = status {
        views.push(Box::new(
            el::<_, SettingsState, ()>("span", message)
                .attr("role", "status")
                .attr("aria-live", "polite"),
        ) as SettingsView);
    }
    Box::new(
        el::<_, SettingsState, ()>("div", views)
            .attr("class", "settings-row-editor")
            .attr("data-component", key.0.clone()),
    )
}

#[derive(Clone)]
struct RowControl {
    label: String,
    action: String,
    key: ComponentKey,
    disabled: bool,
    intent: EditorIntent,
}

fn row_control(
    label: &str,
    action: &str,
    key: &ComponentKey,
    disabled: bool,
    intent: EditorIntent,
) -> RowControl {
    RowControl {
        label: label.to_owned(),
        action: action.to_owned(),
        key: key.clone(),
        disabled,
        intent,
    }
}

fn row_control_view(control: &RowControl) -> SettingsView {
    let intent = control.intent.clone();
    action_button(
        &control.label,
        &control.action,
        Some(&control.key),
        control.disabled,
        move |state| state.editor.enqueue(intent.clone()),
    )
}

pub(super) fn container_controls(state: &SettingsState) -> SettingsView {
    if !state.editor.enabled {
        return Box::new(el::<_, SettingsState, ()>("span", ()));
    }
    let mut controls: Vec<SettingsView> = vec![
        action_button("Add group", "add-group", None, false, |s| {
            let key = unused_container_name(s, ContainerKind::Group);
            s.editor
                .enqueue(EditorIntent::Structure(ArrangementOperation::AddGroup {
                    key,
                    parent: root(),
                    index: usize::MAX,
                }));
        }),
        action_button("Add drawer", "add-drawer", None, false, |s| {
            let key = unused_container_name(s, ContainerKind::Drawer);
            s.editor
                .enqueue(EditorIntent::Structure(ArrangementOperation::AddDrawer {
                    key,
                    parent: root(),
                    index: usize::MAX,
                }));
        }),
    ];
    for (reference, snapshot) in &state.snapshots {
        if reference != super::APPLICATION_REFERENCE {
            continue;
        }
        for entry in &snapshot.specs {
            let reference = reference.clone();
            let setting_id = entry.spec.id.clone();
            let label = format!("Add {}", entry.spec.label);
            controls.push(action_button(
                &label,
                "add-setting",
                None,
                false,
                move |s| {
                    s.editor
                        .enqueue(EditorIntent::Structure(ArrangementOperation::AddSetting {
                            reference: reference.clone(),
                            setting_id: setting_id.clone(),
                            container: root(),
                            index: usize::MAX,
                        }));
                },
            ));
        }
    }
    for container in containers(&state.arrangement.accepted().placement) {
        let label = container_label(&container);
        let empty = find_container_children(&state.arrangement.accepted().placement, &container)
            .is_some_and(|items| items.is_empty());
        controls.push(action_button(
            &format!("Remove empty {label}"),
            "remove-empty-container",
            None,
            !empty,
            {
                let container = container.clone();
                move |s| {
                    s.editor.enqueue(EditorIntent::Structure(
                        ArrangementOperation::RemoveEmptyContainer(container.clone()),
                    ))
                }
            },
        ));
    }
    Box::new(el::<_, SettingsState, ()>("div", controls).attr("class", "settings-container-editor"))
}

fn container_label(container: &ContainerKey) -> String {
    let prefix = format!("{} ", container_kind_name(container));
    if container.key.starts_with(&prefix) {
        container.key.clone()
    } else {
        format!("{prefix}{}", container.key)
    }
}

fn container_kind_name(container: &ContainerKey) -> &'static str {
    match container.kind {
        ContainerKind::Group => "Group",
        ContainerKind::Drawer => "Drawer",
        ContainerKind::Root => "Root",
    }
}

fn action_button(
    label: &str,
    action: &str,
    key: Option<&ComponentKey>,
    disabled: bool,
    callback: impl Fn(&mut SettingsState) + 'static,
) -> SettingsView {
    let key_attr = key.map(|key| key.0.clone()).unwrap_or_default();
    let view = el::<_, SettingsState, ()>("button", label.to_owned())
        .attr("type", "button")
        .attr("data-action", action)
        .attr("data-component", key_attr)
        .attr("aria-label", label);
    let view = if disabled {
        view.attr("disabled", "disabled")
    } else {
        view
    };
    let view = clickable(view, move |state: &mut SettingsState, _: PointerClick| {
        callback(state)
    });
    Box::new(view)
}

fn root() -> ContainerKey {
    ContainerKey {
        kind: ContainerKind::Root,
        key: "root".into(),
    }
}
fn unused_container_name(state: &SettingsState, kind: ContainerKind) -> String {
    let names = containers(&state.arrangement.accepted().placement)
        .into_iter()
        .filter(|container| container.kind == kind)
        .map(|container| container.key)
        .collect::<BTreeSet<_>>();
    let stem = if kind == ContainerKind::Group {
        "Group"
    } else {
        "Drawer"
    };
    (1..)
        .map(|index| format!("{stem} {index}"))
        .find(|name| !names.contains(name))
        .unwrap_or_else(|| format!("{stem} 1"))
}

fn containers(nodes: &[PlacementNode]) -> Vec<ContainerKey> {
    fn walk(nodes: &[PlacementNode], out: &mut Vec<ContainerKey>) {
        for node in nodes {
            match node {
                PlacementNode::Group { key, children } => {
                    let c = ContainerKey {
                        kind: ContainerKind::Group,
                        key: key.0.clone(),
                    };
                    out.push(c);
                    walk(children, out);
                },
                PlacementNode::Drawer { key, children } => {
                    let c = ContainerKey {
                        kind: ContainerKind::Drawer,
                        key: key.0.clone(),
                    };
                    out.push(c);
                    walk(children, out);
                },
                PlacementNode::Component { .. } => {},
            }
        }
    }
    let mut out = Vec::new();
    walk(nodes, &mut out);
    out
}

fn find_location<'a>(
    nodes: &'a [PlacementNode],
    key: &ComponentKey,
    path: Vec<ContainerKey>,
) -> Option<(&'a [PlacementNode], usize)> {
    for (index, node) in nodes.iter().enumerate() {
        match node {
            PlacementNode::Component { key: found } if found == key => return Some((nodes, index)),
            PlacementNode::Group {
                key: container,
                children,
            } => {
                let mut p = path.clone();
                p.push(ContainerKey {
                    kind: ContainerKind::Group,
                    key: container.0.clone(),
                });
                if let Some(found) = find_location(children, key, p) {
                    return Some(found);
                }
            },
            PlacementNode::Drawer {
                key: container,
                children,
            } => {
                let mut p = path.clone();
                p.push(ContainerKey {
                    kind: ContainerKind::Drawer,
                    key: container.0.clone(),
                });
                if let Some(found) = find_location(children, key, p) {
                    return Some(found);
                }
            },
            _ => {},
        }
    }
    None
}

fn find_container_children<'a>(
    nodes: &'a [PlacementNode],
    target: &ContainerKey,
) -> Option<&'a [PlacementNode]> {
    if target.kind == ContainerKind::Root {
        return Some(nodes);
    }
    for node in nodes {
        match node {
            PlacementNode::Group { key, children }
                if target.kind == ContainerKind::Group && key.0 == target.key =>
            {
                return Some(children);
            },
            PlacementNode::Drawer { key, children }
                if target.kind == ContainerKind::Drawer && key.0 == target.key =>
            {
                return Some(children);
            },
            PlacementNode::Group { children, .. } | PlacementNode::Drawer { children, .. } => {
                if let Some(found) = find_container_children(children, target) {
                    return Some(found);
                }
            },
            _ => {},
        }
    }
    None
}
