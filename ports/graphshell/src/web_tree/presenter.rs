// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Host-configured stable faces with an adjacent disclosed-data inspector.
use super::*;
use cambium::button;
use graphshell::host_presenter::{HostPresenter, navigation_url};
use graphshell::projection_compile::HostDatasetV1;
use std::collections::BTreeMap;

pub(super) const WIDTH: u32 = 304;
pub(super) const SHEET: &str = "\
.tree-browsing .tree-command-menu [role=option], .tree-browsing .command-actions button, .tree-browsing .tree-presenter-panel button { min-height:44px; } \
.tree-browse-menu button { min-height:44px; min-width:44px; padding:8px 12px; } \
.tree-presenter-panel { width:304px; box-sizing:border-box; padding:12px; } \
.tree-presenter-panel h2 { font-size:16px; margin:0 0 8px; } \
.tree-presenter-panel h3 { margin:8px 0 4px; font-size:14px; } \
.tree-presenter-panel p { margin:5px 0; overflow-wrap:anywhere; } \
.tree-presenter-panel ul { margin:4px 0; padding:0 0 0 16px; } \
.tree-presenter-panel li { margin:4px 0; overflow-wrap:anywhere; } \
.tree-presenter-panel a { color:#79a9be; display:block; margin:6px 0; } \
.tree-presenter-panel button[aria-pressed=true] { background:#2d5a63; border-color:#79a9be; } \
.tree-presenter-panel .presenter-evidence { font-size:12px; color:#9fb0bb; } \
.tree-presenter .history-control { flex-wrap:wrap; } \
.tree-presenter .history-slider { max-width:100%; } \
.tree-presenter-stacked { flex-direction:column; } \
.tree-presenter-stacked .tree-graph { min-height:240px; } \
.tree-presenter-stacked .tree-presenter-panel { width:100%; height:240px; border-left:0; border-top:1px solid #2c3b44; }";

pub(super) struct Options {
    pub caption: String,
    pub browsing: bool,
    pub summary_field: String,
    pub links: Vec<(String, String)>,
}
pub(super) struct Pane {
    pub model: HostPresenter,
    pub options: Options,
    pub roster_open: bool,
    pub evidence_open: bool,
    pub folds_open: bool,
    pub history_open: bool,
    pub checkpoint_source_open: bool,
    pub selection_open: bool,
}

pub(super) fn requested(root: &Element) -> Result<Option<Options>, String> {
    let params = web_sys::UrlSearchParams::new_with_str(
        &crate::window()?
            .location()
            .search()
            .map_err(|_| "Cannot read presenter options")?,
    )
    .map_err(|_| "Cannot read presenter options")?;
    let profile = params
        .get("presenter")
        .or_else(|| root.get_attribute("data-presenter"));
    match profile.as_deref() {
        None => return Ok(None),
        Some("inspector" | "browse") => {},
        Some(other) => {
            return Err(format!(
                "Unknown presenter {other}; expected inspector or browse"
            ));
        },
    }
    let caption = root
        .get_attribute("data-caption")
        .unwrap_or_else(|| "Project graph".into());
    let summary_field = root
        .get_attribute("data-summary-field")
        .unwrap_or_else(|| "summary".into());
    // The host names field roles. No repository or domain facts are hardcoded.
    let links = [
        ("Project page", "data-project-link-field"),
        ("Source repository", "data-source-link-field"),
    ]
    .into_iter()
    .filter_map(|(label, attr)| root.get_attribute(attr).map(|field| (label.into(), field)))
    .collect();
    Ok(Some(Options {
        caption,
        browsing: profile.as_deref() == Some("browse"),
        summary_field,
        links,
    }))
}

pub(super) fn docked(page: &TreePage) -> bool {
    page.presenter.is_some() && page.size.0 >= 860
}

fn bindings(canvas: &Canvas) -> BTreeMap<String, uuid::Uuid> {
    canvas
        .graph()
        .nodes()
        .filter_map(|(_, node)| {
            Some((
                node.url().strip_prefix("urn:host-dataset:")?.to_owned(),
                node.id,
            ))
        })
        .collect()
}

impl TreePage {
    pub(super) fn capture_host_selection(&mut self) -> bool {
        let Some(pane) = &mut self.presenter else {
            return false;
        };
        let canvas = self.shared.canvas.borrow();
        pane.model.capture(
            &canvas.selected_members(),
            canvas
                .focused_url()
                .and_then(|url| url.strip_prefix("urn:host-dataset:")),
        )
    }

    pub(super) fn restore_host_selection(&mut self) {
        let Some(pane) = &mut self.presenter else {
            return;
        };
        let mut canvas = self.shared.canvas.borrow_mut();
        pane.model.set_visible(bindings(&canvas));
        canvas.set_selected_members(&pane.model.visible_members());
        self.picked = canvas.focused_url().map(str::to_owned);
        self.shared.dirty.set(true);
    }

    pub(super) fn rebind_host_presenter(&mut self, envelope: HostDatasetV1) -> Result<(), String> {
        if let Some(pane) = &mut self.presenter {
            pane.model.rebind(envelope)?;
        }
        Ok(())
    }
}

fn toggle(page: &mut TreePage, id: &str) {
    if let Some(pane) = &mut page.presenter {
        if let Err(error) = pane.model.toggle(id) {
            pane.model.notice = error;
        }
    }
    page.restore_host_selection();
}

fn remove(page: &mut TreePage, id: &str) {
    if page
        .presenter
        .as_ref()
        .is_some_and(|p| p.model.selected.contains(id))
    {
        toggle(page, id);
        *page.shared.command_request.borrow_mut() = Some(commands::Request::Selection);
    }
}

pub(super) fn browsing(page: &TreePage) -> bool {
    page.presenter
        .as_ref()
        .is_some_and(|pane| pane.options.browsing)
}

pub(super) fn toolbar(page: &TreePage) -> Child {
    if browsing(page) {
        return Box::new(
            el(
                "div",
                button("Graph menu", |page: &mut TreePage, _| {
                    commands::open_keyboard(page)
                })
                .attr("aria-haspopup", "dialog")
                .attr("aria-expanded", page.commands.open.is_some().to_string())
                .attr("aria-keyshortcuts", "Control+k Meta+k Shift+F10"),
            )
            .attr("class", "tree-browse-menu"),
        );
    }
    Box::new(
        el(
            "div",
            (
                button("Graph commands", |page: &mut TreePage, _| {
                    commands::open_keyboard(page)
                })
                .attr("aria-haspopup", "dialog")
                .attr("aria-expanded", page.commands.open.is_some().to_string())
                .attr("aria-keyshortcuts", "Control+k Meta+k Shift+F10"),
                button("Fit visible graph", |page: &mut TreePage, _| {
                    CanvasCommand::FitVisible
                        .apply(&mut page.shared.canvas.borrow_mut(), page.shared.size.get());
                    page.shared.dirty.set(true);
                }),
                button("Graph tools", |page: &mut TreePage, _| {
                    page.tools_open = !page.tools_open
                })
                .attr("aria-expanded", page.tools_open.to_string()),
            ),
        )
        .attr("class", "tree-controls"),
    )
}

pub(super) fn panel(page: &TreePage) -> Option<Child> {
    let pane = page.presenter.as_ref()?;
    let p = &pane.model;
    // Keep subject details ahead of an expanded roster, so choosing a project
    // does not bury its summary beneath every other disclosed occurrence.
    let mut disclosure_rows: Vec<Child> = Vec::new();
    let mut rows: Vec<Child> = vec![
        Box::new(el("h2", "Project inspector")),
        Box::new(
            el(
                "p",
                format!(
                    "{} selected · {} visible / {} disclosed",
                    p.selected.len(),
                    p.visible.len(),
                    p.envelope.dataset.occurrences.len()
                ),
            )
            .attr("role", "status"),
        ),
        Box::new(
            button(
                if pane.roster_open {
                    "Hide project list"
                } else {
                    "Choose projects"
                },
                |page: &mut TreePage, _| {
                    if let Some(pane) = &mut page.presenter {
                        pane.roster_open = !pane.roster_open;
                    }
                },
            )
            .attr("aria-expanded", pane.roster_open.to_string()),
        ),
    ];
    if page.grouping.is_some() {
        disclosure_rows.push(Box::new(
            button(
                if pane.folds_open {
                    "Hide Fold controls"
                } else {
                    "Show Fold controls"
                },
                |page: &mut TreePage, _| {
                    if let Some(pane) = &mut page.presenter {
                        pane.folds_open = !pane.folds_open;
                    }
                },
            )
            .attr("aria-expanded", pane.folds_open.to_string()),
        ));
        if pane.folds_open {
            if let Some(controls) = grouping::controls(page) {
                disclosure_rows.push(controls);
            }
        }
    }
    if pane.roster_open {
        let choices: Vec<Child> = p
            .envelope
            .dataset
            .occurrences
            .iter()
            .map(|row| {
                let id = row.occurrence_id.clone();
                Box::new(el(
                    "li",
                    button(
                        format!("Select {}", p.choice_label(&id)),
                        move |page: &mut TreePage, _| toggle(page, &id),
                    )
                    .attr(
                        "aria-pressed",
                        p.selected.contains(&row.occurrence_id).to_string(),
                    )
                    .attr("class", "presenter-select")
                    .attr("data-occurrence-id", row.occurrence_id.clone()),
                )) as Child
            })
            .collect();
        disclosure_rows.push(Box::new(
            el("ul", choices).attr("aria-label", "Disclosed projects"),
        ));
    }
    let mut selection_rows: Vec<Child> = Vec::new();
    if !p.selected.is_empty() {
        selection_rows.push(Box::new(el("h3", "Selected projects")));
        for id in &p.selected {
            let inspect_id = id.clone();
            let remove_id = id.clone();
            selection_rows.push(Box::new(el(
                "div",
                (
                    el(
                        "p",
                        format!(
                            "{}{}",
                            p.label(id),
                            if p.visible.contains_key(id) {
                                ""
                            } else {
                                " · hidden by this Fold view"
                            }
                        ),
                    ),
                    button(
                        format!("Inspect {}", p.choice_label(id)),
                        move |page: &mut TreePage, _| {
                            if let Some(pane) = &mut page.presenter {
                                if let Err(error) = pane.model.inspect(&inspect_id) {
                                    pane.model.notice = error;
                                }
                            }
                        },
                    )
                    .attr("class", "presenter-inspect")
                    .attr("data-occurrence-id", id.clone()),
                    button(
                        format!("Remove {}", p.choice_label(id)),
                        move |page: &mut TreePage, _| remove(page, &remove_id),
                    ),
                ),
            )));
        }
        selection_rows.push(Box::new(button(
            "Clear project selection",
            |page: &mut TreePage, _| {
                if let Some(pane) = &mut page.presenter {
                    pane.model.clear();
                }
                page.restore_host_selection();
                *page.shared.command_request.borrow_mut() = Some(commands::Request::Selection);
            },
        )));
    }
    if !pane.options.browsing {
        rows.append(&mut selection_rows);
    }
    if !p.notice.is_empty() {
        rows.push(Box::new(el("p", p.notice.clone()).attr("role", "status")));
    }
    if let Some(id) = p.inspected.as_deref().filter(|id| p.details_visible(id)) {
        rows.push(Box::new(el("h3", p.label(id).to_owned())));
        rows.push(Box::new(el(
            "p",
            p.text(id, &pane.options.summary_field)
                .unwrap_or("No summary disclosed")
                .to_owned(),
        )));
        if let Some(scope) = p.text(id, "relationship_scope") {
            rows.push(Box::new(el("p", scope.to_owned())));
        }
        for (label, field) in &pane.options.links {
            if let Some(value) = p.text(id, field) {
                if let Some(url) = navigation_url(value) {
                    // Host browser navigation is explicit and triggered by activation.
                    rows.push(Box::new(cambium::on_click(
                        el("a", label.clone()).attr("href", url.clone()),
                        move |_page: &mut TreePage, event| {
                            event.prevent_default();
                            if let Some(window) = web_sys::window() {
                                let _ = window.location().set_href(&url);
                            }
                        },
                    )));
                } else {
                    rows.push(Box::new(el(
                        "p",
                        format!("{label}: unsupported address · {value}"),
                    )));
                }
            }
        }
        let relations: Vec<_> = p
            .envelope
            .relationships
            .iter()
            .filter(|r| r.from_occurrence == id || r.to_occurrence == id)
            .collect();
        rows.push(Box::new(el(
            "h3",
            format!("Disclosed relationships ({})", relations.len()),
        )));
        for relation in &relations {
            rows.push(Box::new(el(
                "p",
                format!(
                    "{}: {} → {}. {}",
                    relation.label,
                    p.label(&relation.from_occurrence),
                    p.label(&relation.to_occurrence),
                    relation.explanation
                ),
            )));
        }
        let related: std::collections::BTreeSet<_> = relations
            .iter()
            .map(|r| {
                if r.from_occurrence == id {
                    r.to_occurrence.clone()
                } else {
                    r.from_occurrence.clone()
                }
            })
            .collect();
        for related_id in related {
            rows.push(Box::new(button(
                format!("Inspect related {}", p.choice_label(&related_id)),
                move |page: &mut TreePage, _| {
                    if let Some(pane) = &mut page.presenter {
                        if let Err(error) = pane.model.inspect(&related_id) {
                            pane.model.notice = error;
                        }
                    }
                },
            )));
        }
        rows.push(Box::new(
            button(
                if pane.evidence_open {
                    "Hide source details"
                } else {
                    "Show source details"
                },
                |page: &mut TreePage, _| {
                    if let Some(pane) = &mut page.presenter {
                        pane.evidence_open = !pane.evidence_open;
                    }
                },
            )
            .attr("aria-expanded", pane.evidence_open.to_string()),
        ));
        if pane.evidence_open {
            let source = &p.envelope.dataset.source;
            rows.push(Box::new(
                el(
                    "p",
                    format!(
                        "Occurrence {id} · {}/{}/{} · revision {}",
                        source.authority,
                        source.domain,
                        source.resource,
                        p.envelope.dataset.revision.as_str()
                    ),
                )
                .attr("class", "presenter-evidence"),
            ));
            if let Some(occurrence) = p.occurrence(id) {
                rows.push(Box::new(
                    el(
                        "p",
                        format!(
                            "Source {}:{}",
                            occurrence.source.adapter, occurrence.source.id
                        ),
                    )
                    .attr("class", "presenter-evidence"),
                ));
                for (name, value) in &occurrence.values {
                    rows.push(Box::new(
                        el(
                            "p",
                            format!("{name}: {}", serde_json::to_string(value).unwrap()),
                        )
                        .attr("class", "presenter-evidence"),
                    ));
                }
            }
            for relation in relations {
                rows.push(Box::new(
                    el("p", grouping::evidence(relation)).attr("class", "presenter-evidence"),
                ));
            }
            rows.push(Box::new(
                el(
                    "p",
                    "Presentation: adjacent-details. Graph faces and placement are unchanged.",
                )
                .attr("class", "presenter-evidence"),
            ));
        }
    } else {
        rows.push(Box::new(el(
            "p",
            if p.selected.is_empty() {
                "Select a project in the graph or choose projects from the list."
            } else {
                "Choose Inspect for a selected project to read its details."
            },
        )));
    }
    if pane.options.browsing && !p.selected.is_empty() {
        rows.push(Box::new(
            button(
                if pane.selection_open {
                    "Hide selection controls"
                } else {
                    "Manage selection"
                },
                |page: &mut TreePage, _| {
                    if let Some(pane) = &mut page.presenter {
                        pane.selection_open = !pane.selection_open;
                    }
                },
            )
            .attr("aria-expanded", pane.selection_open.to_string()),
        ));
        if pane.selection_open {
            rows.extend(selection_rows);
        }
    }
    rows.extend(disclosure_rows);
    Some(Box::new(
        el("aside", rows)
            .attr("class", "tree-tools tree-presenter-panel")
            .attr("role", "region")
            .attr("aria-label", "Project inspector"),
    ))
}

pub(super) fn needs_capture(page: &TreePage) -> bool {
    let Some(pane) = &page.presenter else {
        return false;
    };
    let canvas = page.shared.canvas.borrow();
    let actual: std::collections::BTreeSet<_> = canvas.selected_members().into_iter().collect();
    let expected: std::collections::BTreeSet<_> =
        pane.model.visible_members().into_iter().collect();
    actual != expected
}
