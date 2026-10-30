// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn workbench_view(state: &WorkbenchState) -> WbView {
    let mut children: Vec<WbView> = Vec::new();
    for (i, cell) in state.cells.iter().enumerate() {
        let items: Vec<_> = cell
            .labels
            .iter()
            .zip(&cell.members)
            .map(|(label, member)| TabItem::new(label).with_key(member.to_string()))
            .collect();
        let targets: Vec<_> = cell
            .members
            .iter()
            .filter_map(|member| {
                cell.target.map(|target| crate::reading_view::ReadTarget {
                    pane: target.pane,
                    member: *member,
                })
            })
            .collect();
        let strip = tab_bar_view(
            TabBar::new(&items, cell.strip.selected)
                .with_label("Reading documents")
                .with_names(cambium::TabBarNames {
                    label: Some("reading-tab-label"),
                    ..Default::default()
                }),
            move |state: &mut WorkbenchState, selected| state.cells[i].strip.selected = selected,
            Some(move |state: &mut WorkbenchState, index: usize| {
                if let Some(target) = targets.get(index) {
                    state.read_actions.push(crate::action::Action::Reading(
                        crate::reading_view::ReadAction::Close(*target),
                    ));
                }
            }),
        );
        let body_h = cell.body.h;
        let body = el::<_, WorkbenchState, ()>("div", cell.hint.clone())
            .attr("class", "wb-body")
            .attr(
                "data-native-role",
                cell.appearance_role
                    .map(|role| role.to_string())
                    .unwrap_or_default(),
            )
            .attr(
                "data-native-node",
                cell.members
                    .get(cell.strip.selected)
                    .map(Uuid::to_string)
                    .unwrap_or_default(),
            )
            .attr("style", format!("height: {body_h}px;"));
        children.push(Box::new(placed_with(
            Placement::new(cell.rect.x, cell.rect.y),
            format!(
                "width: {}px; height: {}px; overflow: hidden;",
                cell.rect.w, cell.rect.h
            ),
            arrangement(
                cell.rect.w,
                cell.rect.h,
                vec![
                    if cell.tabs.w <= 0.0 || cell.tabs.h <= 0.0 {
                        Box::new(el("div", ()).attr("style", "display:none;")) as WbView
                    } else {
                        Box::new(placed_with(
                            Placement::new(cell.tabs.x - cell.rect.x, cell.tabs.y - cell.rect.y),
                            format!(
                                "width:{}px;height:{}px;overflow:hidden;",
                                cell.tabs.w, cell.tabs.h
                            ),
                            el::<_, WorkbenchState, ()>("div", strip)
                                .attr(
                                    "class",
                                    if cell.vertical {
                                        "wb-tabs-vertical"
                                    } else {
                                        "wb-tabs-horizontal"
                                    },
                                )
                                .attr("style", "width:100%;height:100%;"),
                        )) as WbView
                    },
                    Box::new(placed_with(
                        Placement::new(cell.toolbar.x - cell.rect.x, cell.toolbar.y - cell.rect.y),
                        format!(
                            "width:{}px;height:{}px;overflow:hidden;",
                            cell.toolbar.w, cell.toolbar.h
                        ),
                        reading_toolbar(state, i),
                    )) as WbView,
                    Box::new(placed_with(
                        Placement::new(cell.body.x - cell.rect.x, cell.body.y - cell.rect.y),
                        format!(
                            "width:{}px;height:{}px;overflow:hidden;",
                            cell.body.w, cell.body.h
                        ),
                        body,
                    )) as WbView,
                ],
            ),
        )));
    }
    Box::new(
        el::<_, WorkbenchState, ()>(
            "div",
            arrangement(state.viewport_w, state.viewport_h, children),
        )
        .attr("class", "pane cambium-document-rail"),
    )
}
