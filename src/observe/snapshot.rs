// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Read the application snapshot. Pure; the app is not disturbed.
pub fn snapshot(app: &App) -> Snapshot {
    let focused = app.graph_runtimes.focused_member().and_then(|member| {
        let url = app.graph_runtimes.focused_url()?.to_string();
        let caption = crate::app::focused_caption(&app.graph_runtimes)?;
        Some(FocusedNode {
            member,
            url,
            caption,
            kept: app.node_is_kept(member),
            page_zoom_percent: crate::app::page_zoom_percent(
                app.browser.get(member).and_then(|state| state.page_scale),
            ),
            applied_page_zoom_percent: app
                .content
                .page_zoom(member)
                .map(|zoom| crate::app::page_zoom_percent(Some(zoom.applied))),
        })
    });
    let content = app
        .graph_runtimes
        .graph()
        .nodes()
        .filter_map(|(_, n)| {
            let state = match app.content.get(n.id)? {
                NodeContent::Requested => "requested".to_string(),
                NodeContent::Live => "live".to_string(),
                NodeContent::AwaitingInput => "awaiting-input".to_string(),
                NodeContent::AwaitingIdentity => "awaiting-identity".to_string(),
                NodeContent::AwaitingTrust => "awaiting-trust".to_string(),
                NodeContent::Failed(err) => format!("failed: {err}"),
            };
            Some((n.id, state))
        })
        .collect();
    // The surface list, derived from app truth (the shell owns the live sessions
    // and the window size; observe reports what a frame would compose). The base
    // is the frisket tree — the Orrery leaf is the canvas, every other leaf a
    // pane — then content over the canvas when the focused node is Live, then
    // chrome on top when it has something to show.
    let visible = app
        .frisket
        .outer_workspace()
        .ok()
        .map(|frame| frame.active_ids())
        .unwrap_or_default();
    let visible_pane = |pane: crate::panes::PaneId| {
        app.maximized.map_or_else(
            || visible.contains(&workbench::TileId(pane.0)),
            |id| id == pane,
        )
    };
    let mut surfaces: Vec<String> = app
        .frisket
        .iter_leaves()
        .filter(|(pane, _, _)| visible_pane(*pane))
        .map(|(_, content, _)| {
            match content {
                crate::panes::PaneContent::Orrery => "canvas".to_string(),
                // A pinned Tile pane with a live session composites as a
                // content surface at the pane's rect (the plan's mapping).
                crate::panes::PaneContent::Tile(m)
                    if matches!(app.content.get(*m), Some(NodeContent::Live)) =>
                {
                    "content".to_string()
                },
                _ => "pane".to_string(),
            }
        })
        .collect();
    // A split tree has seams: one divider surface per split node.
    if app.frisket.split_at(&[]).is_some() && app.maximized.is_none() {
        surfaces.push("divider".to_string());
    }
    // Content surfaces, honestly: a live tile composites where its workbench
    // PANE lives (primary or a lens it tore out to), and the focused node's
    // inset is suppressed while that node tiles in a visible workbench — the
    // one-session-one-surface rule, across windows, mirrored from the shell's
    // plan so this report never claims a surface the frame won't compose.
    let wb_in_primary = app.frisket.iter_leaves().any(|(pane, c, _)| {
        visible_pane(pane) && matches!(c, crate::panes::PaneContent::Workbench)
    });
    let wb_in_lens = app.lenses.iter().flatten().any(|space| {
        space
            .iter_leaves()
            .any(|(_, c, _)| matches!(c, crate::panes::PaneContent::Workbench))
    });
    let live = |m: uuid::Uuid| matches!(app.content.get(m), Some(NodeContent::Live));
    let tiled: Vec<uuid::Uuid> = app
        .active_workbench()
        .map(|workbench| workbench.to_arrangement().1)
        .and_then(|geometry| geometry)
        .map(|geometry| {
            crate::workbench_tiling::place_workbench(
                Some(&geometry),
                crate::surface::Rect::full(1, 1),
            )
            .cells
            .iter()
            .filter_map(|cell| cell.active_member())
            .collect()
        })
        .unwrap_or_default();
    // Pinned Tile panes claim their member wherever their space shows (the
    // same one-session-one-surface rule the workbench tiles follow).
    let tile_panes: Vec<uuid::Uuid> = app
        .frisket
        .iter_leaves()
        .chain(app.lenses.iter().flatten().flat_map(|s| s.iter_leaves()))
        .filter_map(|(_, c, _)| match c {
            crate::panes::PaneContent::Tile(m) => Some(*m),
            _ => None,
        })
        .collect();
    let tile_content_here = wb_in_primary && tiled.iter().any(|m| live(*m));
    let focused_inset = app.graph_runtimes.focused_member().is_some_and(|m| {
        live(m)
            && !app.map_navigation_active()
            && !((wb_in_primary || wb_in_lens) && tiled.contains(&m))
            && !tile_panes.contains(&m)
    });
    if tile_content_here || focused_inset {
        surfaces.push("content".to_string());
    }
    if app.document_find.open
        || app.user_agent_decision.is_open()
        || app.omnibar.open && app.shell_chrome_config().projects_omnibar()
        || app.shell_chrome_config().projects_shellbar()
            && crate::app::focused_caption(&app.graph_runtimes).is_some()
    {
        surfaces.push("chrome".to_string());
    }
    Snapshot {
        source_captures: {
            let mut captures: Vec<_> = app.source_captures.values()
                .filter(|attempt| attempt.request.session == app.session_id)
                .map(|attempt| SourceCaptureView {
                    request: attempt.request.id, ordinal: attempt.ordinal, node: attempt.request.node,
                    session: attempt.request.session, graph: attempt.request.graph,
                    url: attempt.request.url.clone(), source: attempt.request.source.clone(),
                    keep: attempt.request.keep,
                    phase: attempt.phase, error: attempt.error.clone(), stored: attempt.stored.clone(),
                }).collect();
            captures.sort_by_key(|capture| capture.request);
            captures
        },
        graph_scene: crate::graph_scenes::facts(app.scene_command_pane()
            .and_then(|pane| app.graph_for_pane(pane))
            .and_then(|graph| app.graph_runtimes.canvas(graph))
            .unwrap_or(&app.graph_runtimes)),
        place_status: app.place.status_lines(),
        place: place_facts(app),
        focused,
        omnibar: OmnibarView {
            open: app.omnibar.open,
            text: app.omnibar.presented_text(),
            cursor: app.omnibar.presented_cursor(),
            selected: app.omnibar.selected,
            suggestions: app
                .omnibar
                .suggestions
                .iter()
                .map(suggestion_line)
                .collect(),
        },
        document_find: app.document_find.open.then(|| {
            let current = app
                .document_find
                .model
                .current
                .filter(|index| *index < app.document_find.model.count);
            let current_match =
                current.and_then(|index| app.document_find.model.matches.get(index));
            DocumentFindView {
                target: app
                    .document_find
                    .target
                    .expect("an open find has a captured target"),
                query: app.document_find.query().to_owned(),
                caret: app.document_find.input.caret(),
                selection: app.document_find.input.selected_text().to_owned(),
                count: app.document_find.model.count,
                current,
                pending: app.document_find.pending,
                status: app.document_find.status(),
                current_role: current_match.and_then(|item| item.role.clone()),
                current_label: current_match.map(|item| item.label.clone()),
            }
        }),
        document_capabilities: app
            .graph_runtimes
            .focused_member()
            .and_then(|node| app.content.facts(node))
            .map(|facts| DocumentCapabilitiesView {
                find_in_page: facts.capabilities.find_in_page.describe(),
                page_zoom: facts.capabilities.page_zoom.describe(),
                page_capture: facts.capabilities.page_capture.describe(),
                navigation: facts.capabilities.navigation.describe(),
            }),
        engine_inventory: app.engine_inventory.clone(),
        viewer_choices: crate::inspector_controls::viewer_options(app,
            app.graph_runtimes.focused_member()
                .and_then(|node| app.browser.get(node))
                .and_then(|state| state.viewer_override.as_deref()),
        ),
        user_agent_decision: app.user_agent_decision.active().map(|decision| {
            let request = decision.request();
            let authentication_field = matches!(
                decision,
                crate::user_agent_decision::PendingUserAgentDecision::Authentication { .. }
            )
            .then_some(match app.user_agent_decision.authentication.field {
                crate::user_agent_decision::AuthenticationField::Username => "username",
                crate::user_agent_decision::AuthenticationField::Password => "password",
            });
            UserAgentDecisionView {
                node: request.node,
                request: request.id.get(),
                kind: decision.kind(),
                prompt: decision.prompt(),
                queued: app.user_agent_decision.len(),
                submitting: app.user_agent_decision.submitting == Some(request),
                authentication_field,
                remember_for_process: app.user_agent_decision.authentication.remember_for_process,
                error: app.user_agent_decision.error.clone(),
            }
        }),
        content,
        node_count: app.graph_runtimes.graph().nodes().count(),
        physics_energy: app.graph_runtimes.layout_stats().energy,
        physics_paused: app.graph_runtimes.physics_paused(),
        layout_overlaps: app.graph_runtimes.layout_stats().overlaps,
        feed_subscriptions: app.feeds.len(),
        feed_unread: app.feeds.unread_count(),
        graph_visible: app.graph_runtimes.graph_visible(),
        surfaces,
        focus: app.focus.label().to_string(),
        panes: app
            .frisket
            .iter_leaves()
            .map(|(_, content, _)| content.tag().to_string())
            .collect(),
        active_pane: app.active_pane.and_then(|id| {
            app.frisket
                .iter_leaves()
                .find(|(pid, _, _)| *pid == id)
                .map(|(_, content, _)| content.tag().to_string())
        }),
        floating_panes: app.frisket.outer_workspace().ok().into_iter()
            .flat_map(|frame|frame.workspace.visible_floating(true).into_iter().map(|f|crate::panes::PaneId(f.tile.id.0)).collect::<Vec<_>>())
            .filter_map(|pane|app.pane_content(pane).map(|c|c.tag().to_string())).collect(),
        maximized: app.maximized.is_some(),
        trail_rows: app
            .frisket
            .iter_leaves()
            .any(|(_, c, _)| matches!(c, crate::panes::PaneContent::Trail))
            .then(|| {
                crate::trail_view::trail_rows(app)
                    .into_iter()
                    .map(|r| r.text)
                    .collect()
            })
            .unwrap_or_default(),
        roster_rows: app
            .frisket
            .iter_leaves()
            .any(|(_, c, _)| matches!(c, crate::panes::PaneContent::Roster))
            .then(|| {
                crate::roster_view::roster_rows(app)
                    .into_iter()
                    .map(|r| r.text)
                    .collect()
            })
            .unwrap_or_default(),
        inspector_rows: app.frisket.iter_leaves()
            .filter(|(_, content, _)| matches!(content, crate::panes::PaneContent::Inspector))
            .flat_map(|(pane, _, _)| {
                let mut lines = app.pane_binding_snapshot(pane).map(|binding| vec![format!("Pane source: {}: {}", binding.mode, binding.source)]).unwrap_or_default();
                lines.extend(crate::inspector_view::inspector_sections_for_pane(app, pane).into_iter().flat_map(|section| section.rows).map(|(key,value)| format!("{key}: {value}")));
                lines
            }).collect(),
        arrange_rows: app
            .frisket
            .iter_leaves()
            .any(|(_, c, _)| {
                matches!(c, crate::panes::PaneContent::Registered(kind) if kind.as_str() == crate::panes::kind::ARRANGE)
            })
            .then(|| crate::arrange_pane::arrange_rows(app))
            .unwrap_or_default(),
        roster_tab: crate::cambium_pane::tab_label(app.roster_tab),
        split_ratio: match &app.frisket.root {
            crate::panes::PaneNode::Split { ratio, .. } => Some(*ratio),
            crate::panes::PaneNode::Leaf { .. } => None,
            crate::panes::PaneNode::Workspace(_) => app.frisket.split_at(&[]).map(|(_, ratio)| ratio),
        },
        workbench_cells: workbench_cells(app),
        workbench_fractions: app
            .active_workbench()
            .map(mere::platen::Workbench::weights)
            .unwrap_or_default(),
        a11y: crate::a11y::a11y_lines(app),
        can_back: app.focused_can_back(),
        can_forward: app.focused_can_forward(),
        browser_fetch: app.graph_runtimes.focused_member().and_then(|node| {
            app.content.fetch_phase(node).map(|phase| match phase {
                crate::content::PageFetchPhase::Requested => "requested".to_string(),
                crate::content::PageFetchPhase::Streaming { received_bytes, .. } => {
                    format!("streaming bytes={received_bytes}")
                }
                crate::content::PageFetchPhase::Settled { received_bytes } => {
                    format!("settled bytes={received_bytes}")
                }
                crate::content::PageFetchPhase::Loading { progress_millis } => progress_millis
                    .map(|value| format!("loading percent={}", value / 10))
                    .unwrap_or_else(|| "loading".to_string()),
                crate::content::PageFetchPhase::Stopped { received_bytes } => {
                    format!("stopped bytes={received_bytes}")
                }
            })
        }),
        link_preview: app.link_preview.clone(),
        available_actions: app
            .available_actions()
            .into_iter()
            .map(|(label, _)| label)
            .collect(),
        windows: app.window_count,
        lens_panes: app
            .lenses
            .iter()
            .enumerate()
            .filter_map(|(ordinal, space)| space.as_ref().map(|s| (ordinal, s)))
            .flat_map(|(ordinal, space)| {
                space
                    .iter_leaves()
                    .map(move |(_, content, _)| format!("{ordinal}:{}", content.tag()))
                    .collect::<Vec<_>>()
            })
            .collect(),
        lens_floating_panes: app.lenses.iter().enumerate().filter_map(|(i,s)|s.as_ref().map(|s|(i,s)))
            .flat_map(|(i,space)|space.outer_workspace().ok().into_iter().flat_map(|frame|frame.workspace.visible_floating(true).iter().map(|f|crate::panes::PaneId(f.tile.id.0)).collect::<Vec<_>>())
                .filter_map(move |pane|app.pane_content(pane).map(|c|format!("{i}:{}",c.tag()))).collect::<Vec<_>>()).collect(),
        session: app.session_label(app.session_id),
        session_count: app.sessions.len(),
        active_ratio: app.active_pane.and_then(|active| {
            let layout = app.space(app.space_of(active)?)?;
            let mut path = crate::pane::path_of(layout, active)?;
            // The parent split holds the divider; a root leaf has none.
            path.pop()?;
            crate::pane::place_panes(layout, crate::surface::Rect::full(100, 100), None)
                .dividers
                .iter()
                .find(|d| d.path == path)
                .map(|d| d.ratio)
        }),
    }
}
