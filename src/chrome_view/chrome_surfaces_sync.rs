// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ChromeSurfaces {
    /// Mirror app truth into the chrome state and rebuild every projection
    /// (one update, every window re-reads — the one-state contract). The
    /// caller passes each live window's size, slot-indexed.
    pub fn sync(&mut self, app: &App, sizes: &[(usize, f32, f32)]) {
        for &(slot, _, _) in sizes {
            self.ensure_slot(slot);
        }
        let omnibar: &OmnibarState = &app.omnibar;
        let mut omnibar_input = TextInput::new(omnibar.presented_text());
        omnibar_input.set_caret_byte(omnibar.presented_cursor(), false);
        if let Some(preedit) = omnibar.presented_preedit() {
            omnibar_input.set_preedit(preedit);
        }
        let chrome = app.shell_chrome_config();
        // Only the `>` lane's command rows carry Keep/Drop.
        let keep_states = (omnibar.open && omnibar.text.trim().starts_with('>'))
            .then(|| app.command_keep_states());
        let rows: Vec<RowView> = omnibar
            .suggestions
            .iter()
            .enumerate()
            .map(|(i, s)| RowView {
                text: row_text(s),
                class: match s {
                    Suggestion::Hint(_) | Suggestion::Prompt(_) => "omni-row-muted",
                    _ if is_install_review(s) && i == omnibar.selected => {
                        "omni-row-sel omni-row-review"
                    },
                    _ if is_install_review(s) => "omni-row omni-row-review",
                    _ if i == omnibar.selected => "omni-row-sel",
                    _ => "omni-row",
                },
                commit: match s {
                    Suggestion::Hint(_) | Suggestion::Prompt(_) => None,
                    _ => Some(i),
                },
                keep: match s {
                    Suggestion::Act { label, action }
                        if !is_install_review(s)
                            && *action != crate::action::Action::OmnibarShowAllCommands =>
                    {
                        keep_states.as_ref().and_then(|states| states.of(label))
                    },
                    _ => None,
                },
            })
            .collect();
        let open = omnibar.open;
        let browser_target = app.browser_command_target();
        let focused = browser_target.as_ref().map(|(member, _)| *member);
        let caption = crate::app::navigation_caption(app);
        let visible = app
            .frisket
            .outer_workspace()
            .ok()
            .map(|frame| frame.active_ids())
            .unwrap_or_default();
        let tools = crate::panes::pane_palette_entries()
            .into_iter()
            .filter_map(|(_, kind)| {
                let definition = crate::panes::pane_definition(kind.as_str())?;
                if matches!(
                    definition.id,
                    crate::panes::kind::GRAPH | crate::panes::kind::WORKBENCH
                ) {
                    return None;
                }
                let shown = app.frisket.iter_leaves().any(|(pane, content, _)| {
                    content.kind_id().as_str() == definition.id
                        && visible.contains(&workbench::TileId(pane.0))
                });
                Some((definition.id, definition.display_name.to_string(), shown))
            })
            .collect::<Vec<_>>();
        let map_active = app
            .active_pane
            .and_then(|pane| match app.pane_content(pane) {
                Some(crate::panes::PaneContent::Orrery) => Some(true),
                Some(crate::panes::PaneContent::Workbench) => Some(false),
                _ => None,
            })
            .unwrap_or_else(|| {
                visible.iter().any(|id| {
                    matches!(
                        app.pane_content(crate::panes::PaneId(id.0)),
                        Some(crate::panes::PaneContent::Orrery)
                    )
                })
            });
        // The document navigator projects graph membership, not open tabs.
        let navigation_graph = app
            .active_pane
            .and_then(|pane| {
                let context = if app.has_pane_binding(pane) {
                    app.pane_bound_context(pane)
                } else {
                    app.pane_context.resolve_context(
                        pane,
                        &crate::panes::ContextBinding::Own,
                        crate::panes::SourceSelector::Graph,
                    )
                };
                context
                    .filter(|context| context.member == focused)
                    .and_then(|context| context.graph)
            })
            .or_else(|| {
                focused.and_then(|member| app.graph_runtimes.graph_containing_member(member))
            })
            .or_else(|| app.active_pane.and_then(|pane| app.graph_for_pane(pane)))
            .unwrap_or_else(|| app.graph_runtimes.active_graph());
        let arranged = app
            .frisket
            .iter_leaves()
            .filter(|(_, _, graph)| *graph == navigation_graph)
            .filter_map(|(pane, _, _)| app.workbench_for_pane(pane))
            .flat_map(|workbench| workbench.open_members())
            .collect::<std::collections::HashSet<_>>();
        let reading_stacks = app
            .primary_graph_for_navigation(focused)
            .and_then(|pane| app.workbench_for_pane(pane))
            .map(|workbench| {
                workbench
                    .slot_views()
                    .map(|slot| {
                        slot.members
                            .iter()
                            .enumerate()
                            .map(|(i, member)| {
                                let title =
                                    app.graph_runtimes
                                        .canvas(navigation_graph)
                                        .and_then(|canvas| {
                                            canvas.graph().get_node_by_id(*member).map(
                                                |(key, _)| canvas.graph().node_display_label(key),
                                            )
                                        })
                                        .unwrap_or_else(|| "Unavailable document".into());
                                (*member, title, slot.active == i)
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut documents = app
            .graph_runtimes
            .canvas(navigation_graph)
            .into_iter()
            .flat_map(|canvas| {
                canvas.graph().nodes().map(|(_, node)| {
                    (
                        node.id,
                        if node.title.trim().is_empty() || node.title == node.url() {
                            node.url().to_string()
                        } else {
                            node.title.clone()
                        },
                        arranged.contains(&node.id),
                    )
                })
            })
            .collect::<Vec<_>>();
        documents.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        let connections = focused
            .and_then(|member| {
                app.graph_runtimes.canvas(navigation_graph).map(|canvas| {
                    crate::relationship_navigation::relationships(
                        navigation_graph,
                        canvas.graph(),
                        member,
                    )
                })
            })
            .unwrap_or_default();
        let focused_kept = focused.is_some_and(|member| app.node_is_kept(member));
        let can_back = focused.is_some_and(|member| app.member_can_back(member));
        let can_forward = focused.is_some_and(|member| app.member_can_forward(member));
        let fetching = focused.is_some_and(|node| app.content.fetch_in_progress(node));
        let fetch_status = focused.and_then(|node| match app.content.fetch_phase(node) {
            Some(crate::content::PageFetchPhase::Requested) => Some("Requested".to_string()),
            Some(crate::content::PageFetchPhase::Streaming { received_bytes, .. }) => {
                Some(format!("Streaming  {received_bytes} bytes"))
            },
            Some(crate::content::PageFetchPhase::Loading { progress_millis }) => {
                Some(match progress_millis {
                    Some(value) => format!("Loading  {}%", value / 10),
                    None => "Loading".to_string(),
                })
            },
            Some(crate::content::PageFetchPhase::Stopped { received_bytes }) => {
                Some(format!("Stopped  {received_bytes} bytes"))
            },
            Some(crate::content::PageFetchPhase::Settled { .. }) | None => None,
        });
        let focused_url = browser_target.map(|(_, url)| url);
        let link_preview = app.link_preview.clone();
        let find = cambium::ReadFind {
            open: app.document_find.open,
            input: app.document_find.input.clone(),
            status: app.document_find.status(),
            ..Default::default()
        };
        let decision = app.user_agent_decision.active().map(|decision| {
            let request = decision.request();
            let prompt = decision.prompt();
            let queued = app.user_agent_decision.len();
            let submitting = app.user_agent_decision.submitting == Some(request);
            let error = app.user_agent_decision.error.clone();
            match decision {
                crate::user_agent_decision::PendingUserAgentDecision::Permission { .. } => {
                    DecisionCard::Permission {
                        request,
                        prompt,
                        queued,
                        submitting,
                        error,
                    }
                },
                crate::user_agent_decision::PendingUserAgentDecision::Authentication { .. } => {
                    let mut username = TextInput::new(
                        app.user_agent_decision
                            .authentication
                            .username
                            .as_str()
                            .to_string(),
                    );
                    username.set_caret_byte(
                        app.user_agent_decision
                            .authentication
                            .username
                            .as_str()
                            .len(),
                        false,
                    );
                    let password_text = app.user_agent_decision.authentication.password_mask();
                    let mut password = TextInput::new(password_text.clone());
                    password.set_caret_byte(password_text.len(), false);
                    DecisionCard::Authentication {
                        request,
                        prompt,
                        queued,
                        username,
                        password,
                        field: app.user_agent_decision.authentication.field,
                        remember_for_process: app
                            .user_agent_decision
                            .authentication
                            .remember_for_process,
                        submitting,
                        error,
                    }
                },
            }
        });
        self.runner.update(|state| {
            if open
                || decision.is_some()
                || state.layout != chrome.layout
                || state.focused_node != focused
            {
                state.menu = None;
                state.documents_start = 0;
            }
            state.focus_cards = app.focus_card_views();
            for fields in state.forme_fields.values_mut() {
                fields.retain(|entry| {
                    app.pane_content(entry.pane) == Some(&crate::panes::PaneContent::Orrery)
                        && app.valid_focus_card(&entry.target)
                });
            }
            state.peek_selected = app.selected_focus_card();
            state.peek_below = app.lower_scope_card();
            state.peek_forme = app.forme_card_target();
            state.relation_targets = connections
                .iter()
                .filter_map(|row| {
                    app.focus_card_target(
                        row.target.graph,
                        row.target.member,
                        crate::focus_card::Origin::Relationship(row.target),
                    )
                    .map(|t| (row.target, t))
                })
                .collect();
            state.layout = chrome.layout;
            state.tools = tools.clone();
            if state.focused_node != focused || state.connections != connections {
                state.relationships_start = 0;
                if state.menu == Some(ChromeMenu::Connections) {
                    state.menu_selected = 0;
                }
            }
            state.connections = connections.clone();
            state.documents = documents.clone();
            state.reading_stacks = reading_stacks.clone();
            state.map_active = map_active;
            state.territory = app
                .graph_runtimes
                .canvas(navigation_graph)
                .is_some_and(|canvas| canvas.territory_field());
            for &(slot, w, h) in sizes {
                while state.windows.len() <= slot {
                    state.windows.push(WindowChrome::default());
                }
                let win = &mut state.windows[slot];
                win.scale = chrome.layout_scale(slot);
                win.w = w / win.scale;
                win.h = h / win.scale;
                win.caption = caption.clone();
                win.primary = slot == 0;
                win.titlebar_insets = chrome
                    .titlebar_insets
                    .get(&slot)
                    .copied()
                    .unwrap_or_default();
            }
            state.open = open;
            state.inline_address = open
                && matches!(omnibar.mode, crate::ui::OmnibarMode::Address)
                && !omnibar.text.trim_start().starts_with('>')
                && chrome.projects_shellbar();
            state.omnibar_input = omnibar_input.clone();
            state.rows = rows.clone();
            state.omnibar_placement = chrome.omnibar.placement.clone();
            state.shellbar_placement = chrome.shellbar.placement.clone();
            state.shellbar_visible = chrome.projects_shellbar();
            state.zoom = chrome.appearance.zoom();
            let mut scope_controls = app.scope_controls();
            if scope_controls.back == state.scope_controls.back
                && scope_controls.forward == state.scope_controls.forward
                && scope_controls.up == state.scope_controls.up
                && scope_controls.down == state.scope_controls.down
            {
                scope_controls.hint = state.scope_controls.hint;
            }
            state.scope_controls = scope_controls;
            state.scope_crumbs = app.scope_crumbs();
            state.can_back = can_back;
            state.can_forward = can_forward;
            state.focused_node = focused;
            state.focused_kept = focused_kept;
            state.fetching = fetching;
            state.fetch_status = fetch_status.clone();
            state.focused_url = focused_url.clone();
            state.link_preview = link_preview.clone();
            state.find = find.clone();
            state.decision = decision.clone();
        });
        self.card_leaves = sprigging::LeafRegistry::new();
        for card in &self.runner.state().focus_cards {
            if let Some(swatch) = &card.swatch {
                self.card_leaves.insert(
                    swatch.leaf_key,
                    Box::new(swatch.paint_leaf(|_| sprigging::ColorF {
                        r: 0.48,
                        g: 0.68,
                        b: 0.86,
                        a: 1.0,
                    })),
                );
            }
        }
        self.appearance = chrome.appearance.clone();
        self.absorb_dom_mutations();
    }

    /// The layout scale `slot`'s chrome was last synced at.
    pub(super) fn window_scale(&self, slot: usize) -> f32 {
        self.runner
            .state()
            .windows
            .get(slot)
            .map_or(1.0, |win| if win.scale > 0.0 { win.scale } else { 1.0 })
    }

    /// One window's chrome scene: ITS window-root laid out at its own size
    /// through a per-root Livery/Buckram subtree cascade — the true forest-dom F2
    /// path. (This replaced the `display: none` visibility flip the chrome
    /// bridged with while the retired genet-stylo sharing cache panicked on
    /// subtree roots; the fix published as 0.19.1 and the flip is gone.)
    pub fn scene(&mut self, slot: usize, w: u32, h: u32) -> netrender::Scene {
        self.absorb_dom_mutations();
        let Some(&id) = self.projections.get(slot) else {
            return netrender::Scene::new(w, h);
        };
        let Some(root) = self.runner.window_root(id) else {
            return netrender::Scene::new(w, h);
        };
        let sheet = crate::ui::chrome_sheet(&self.appearance);
        let scale = self.window_scale(slot);
        let dom = self.dom.borrow();
        let Some(layout) = self.layouts.get_mut(slot) else {
            return netrender::Scene::new(w, h);
        };
        layout.set_scale(scale);
        layout.scene_with_leaves(
            &dom,
            root,
            &sheet,
            w,
            h,
            &mut self.card_leaves,
            &mut self.card_paint,
        )
    }

    pub(crate) fn app_region_at(
        &mut self,
        slot: usize,
        x: f32,
        y: f32,
        w: u32,
        h: u32,
    ) -> cambium_rootstock::AppRegion {
        self.absorb_dom_mutations();
        let Some(&id) = self.projections.get(slot) else {
            return Default::default();
        };
        let Some(root) = self.runner.window_root(id) else {
            return Default::default();
        };
        let sheet = crate::ui::chrome_sheet(&self.appearance);
        let scale = self.window_scale(slot);
        let dom = self.dom.borrow();
        let Some(layout) = self.layouts.get_mut(slot) else {
            return Default::default();
        };
        layout.set_scale(scale);
        layout.app_region_at(&dom, root, &sheet, w, h, x, y)
    }
}
