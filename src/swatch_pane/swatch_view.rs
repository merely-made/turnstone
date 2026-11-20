// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn swatch_view(state: &SwatchState) -> SwatchView {
    // The canvas owns hover emphasis; this pane keeps only what the graph
    // decides (selection, arriving through `state.swatch`) and the intents it
    // has to act on.
    let swatch = graph_canvas(
        &state.swatch,
        |state: &mut SwatchState, event: GraphCanvasEvent<Uuid>| match event {
            GraphCanvasEvent::Activate(id) => {
                if let Some(selection) = &mut state.neighborhood
                    && let Some(address) = state
                        .swatch
                        .graph
                        .nodes
                        .iter()
                        .find(|node| node.id == id)
                        .and_then(|node| node.key.clone())
                {
                    selection.focus = Some(address);
                    selection.link = None;
                    return;
                }
                if let Some(activate) = state.activate_of.get(&id) {
                    state.pending.push(SwatchIntent::Activate(activate.clone()));
                }
            },
            GraphCanvasEvent::RelationActivate(id) => {
                if let Some(selection) = &mut state.neighborhood
                    && selection.details.contains_key(&id)
                {
                    selection.link = Some(id);
                }
            },
            GraphCanvasEvent::Expand => state.pending.push(SwatchIntent::Expand),
            GraphCanvasEvent::Drag(_)
            | GraphCanvasEvent::Pan { .. }
            | GraphCanvasEvent::Zoom { .. } => {},
        },
    );
    // The swatch fills the top (its own leaf is sized to `swatch_h` in sync).
    let mut children: Vec<SwatchView> = vec![Box::new(
        cambium::el::<_, SwatchState, ()>("div", swatch).attr(
            "style",
            format!("position: absolute; left: {SWATCH_PAD}px; top: {SWATCH_PAD}px;"),
        ),
    )];

    // Composed sections stack below the swatch in a scrollable column (the
    // gloss-composite: the minimap plus, say, the recycle bin's Removed rows).
    if !state.sections.is_empty() {
        let mut section_kids: Vec<SwatchView> = Vec::new();
        for (_id, title, rows) in &state.sections {
            section_kids.push(Box::new(
                cambium::el::<_, SwatchState, ()>("div", title.to_string()).attr(
                    "style",
                    "color: #7d8590; padding: 6px 12px 2px; font-size: 11px;",
                ),
            ));
            if rows.is_empty() {
                section_kids.push(Box::new(
                    cambium::el::<_, SwatchState, ()>("div", "nothing here".to_string()).attr(
                        "style",
                        "color: #484f58; padding: 2px 12px; font-size: 12px;",
                    ),
                ));
            } else {
                for row in rows {
                    // The row's activation rides it as DATA (the swatch node's
                    // rule): the click handler pushes the intent the provider
                    // declared, so a new provider needs no handler code here.
                    // `section-row` is the probe class a receipt addresses.
                    let row_view = cambium::el::<_, SwatchState, ()>("div", row.text.clone())
                        .attr("class", "section-row")
                        .attr("data-diagnostic-id", row.id.clone().unwrap_or_default())
                        .attr(
                            "style",
                            "color: #c9d1d9; padding: 2px 12px; font-size: 12px;",
                        );
                    let Some(activate) = row.activate.clone() else {
                        // Inspection and other read-only sections install no
                        // activation handler. Actions remain provider data.
                        section_kids.push(Box::new(row_view));
                        continue;
                    };
                    section_kids.push(Box::new(cambium::on_click(
                        row_view,
                        move |state: &mut SwatchState, _click: cambium::PointerClick| {
                            match &activate {
                                crate::sections::SectionActivate::Open(url) => {
                                    state.pending.push(SwatchIntent::Activate(
                                        SwatchActivate::Open(url.clone()),
                                    ));
                                },
                                crate::sections::SectionActivate::Recover(id) => {
                                    state
                                        .pending
                                        .push(SwatchIntent::Activate(SwatchActivate::Recover(*id)));
                                },
                            }
                        },
                    )));
                }
            }
        }
        if let Some(selection) = &state.neighborhood
            && let Some(inspection) = selection
                .link
                .as_ref()
                .and_then(|id| selection.details.get(id))
        {
            section_kids.push(Box::new(
                cambium::el::<_, SwatchState, ()>(
                    "section",
                    cambium::detail_panel(&inspection.details()),
                )
                .attr("aria-label", "Link inspector"),
            ));
        }
        if let Some(selection) = &state.neighborhood {
            section_kids.extend(super::validation::controls(&selection.validation));
        }
        children.push(Box::new(
            cambium::el::<_, SwatchState, ()>("div", section_kids)
                .attr("class", SECTIONS_CLASS)
                .attr("data-diagnostics-region", if state.sections.iter().any(|(id, _, _)| *id == "diagnostics") { "true" } else { "" })
                .attr(
                "style",
                format!(
                    "position: absolute; left: 0px; top: {}px; width: {}px; height: {}px; overflow-y: auto;",
                    state.swatch_h,
                    state.viewport_w,
                    (state.viewport_h - state.swatch_h).max(0.0),
                ),
            ),
        ));
    }

    Box::new(
        cambium::el::<_, SwatchState, ()>("div", children)
            .attr("class", "pane")
            .attr(
                "style",
                format!(
                    "position: relative; width: {}px; height: {}px;",
                    state.viewport_w, state.viewport_h
                ),
            ),
    )
}
