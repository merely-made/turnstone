// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn adapter_snapshot(&self) -> taproot::ProbeSnapshot {
        let snap = crate::observe::snapshot(&self.app);
        let appearance = self.live_settings.snapshot();
        let kept = snap.focused.as_ref().is_some_and(|node| node.kept);
        let saved_reader_node = self.focused_saved_reader_node();
        let saved_reader = saved_reader_node.and_then(|node| self.app.saved_readers.get(&node));
        let saved_reader_state = match saved_reader {
            None => "None",
            Some(reader) if reader.error.is_some() => "Error",
            Some(reader)
                if reader.request.is_none()
                    && reader.content.is_some()
                    && saved_reader_node
                        .is_some_and(|node| self.saved_reader_engine_live(node)) =>
            {
                "Ready"
            },
            Some(_) => "Pending",
        };
        let saved_reader_text = saved_reader
            .and_then(|reader| reader.content.as_ref())
            .map(|content| content.text.as_str())
            .unwrap_or_default();
        let saved_reader_quote = saved_reader
            .and_then(|reader| {
                reader.content.as_ref().and_then(|content| {
                    crate::saved_sources::validate_selector(content, &reader.target)
                        .ok()
                        .flatten()
                })
            })
            .unwrap_or_default();
        let saved_reader_manifest = saved_reader
            .map(|reader| reader.target.key.annotation_manifest.as_str())
            .unwrap_or_default();
        let saved_reader_origin = saved_reader
            .and_then(|reader| reader.content.as_ref())
            .map(|content| content.origin.as_str())
            .unwrap_or_default();
        let saved_reader_selector = saved_reader
            .and_then(|reader| reader.target.selector.as_ref())
            .map(|selector| format!("{}..{}", selector.start, selector.end))
            .unwrap_or_default();
        let reading_target = self.app.focused_read_target();
        let mut out = taproot::ProbeSnapshot::default()
            .with_field("reading-text-size",reading_target.map(|target|self.app.reading.preferences(target).text_size.to_string()).unwrap_or_default())
            .with_field("reading-column-em",reading_target.map(|target|self.app.reading.preferences(target).column_em.to_string()).unwrap_or_default())
            .with_field("focus-card-forme",self.app.focus_card_views().iter().any(|c|c.forme.is_some()).to_string())
            .with_field("forme-field-controls",self.chrome.forme_field_control_count(0).to_string())
            .with_field("focus-card-forme-cells",self.app.focus_card_views().iter().filter_map(|c|c.forme.as_ref()).map(|f|f.cells.len()).sum::<usize>().to_string())
            .with_field("focus-card-count",self.app.focus_card_views().len().to_string())
            .with_field("focus-card-kept",self.app.focus_card_views().iter().filter(|c|c.kept).count().to_string())
            .with_field("focus-card-scope",self.app.reading.cards.peek.as_ref().and_then(|t|if let crate::focus_card::Origin::Scope(p)=&t.origin {Some(p.level.label())} else {None}).unwrap_or_default())
            .with_field("focus-card-peek",self.app.reading.cards.peek.as_ref().filter(|t|self.app.valid_focus_card(t)).map(|t|t.address.clone()).unwrap_or_default())
            .with_field("scope-trail",self.app.scope_place().trail().iter().map(|p|p.level.label()).collect::<Vec<_>>().join("/"))
            .with_field("scope-level",self.app.scope_place().level.label())
            .with_field("scope-focus",self.app.scope_place().focus.map(|m|m.to_string()).unwrap_or_default())
            .with_field("scope-back",self.app.reading.scope.navigation.destination(false).is_some().to_string())
            .with_field("scope-forward",self.app.reading.scope.navigation.destination(true).is_some().to_string())
            .with_field("reading-history-back",reading_target.is_some_and(|target|self.app.reading.trails.get(&target.pane).is_some_and(|trail|trail.can_back())).to_string())
            .with_field("reading-history-forward",reading_target.is_some_and(|target|self.app.reading.trails.get(&target.pane).is_some_and(|trail|trail.can_forward())).to_string())
            .with_field("reading-anchor",reading_target.and_then(|target|self.app.reading.anchors.get(&target)).map(|anchor|format!("{}:{}:{}",anchor.text,anchor.offset,anchor.exact)).unwrap_or_default())
            .with_field("reading-find",reading_target.and_then(|target|self.app.reading.find.get(&target)).filter(|ui|ui.open).map_or("closed",|ui|ui.status.as_str()))
            .with_field("scene-arrangement", snap.graph_scene.arrangement)
            .with_field("scene-saved-seed", snap.graph_scene.saved_seed.to_string())
            .with_field("scene-regions", snap.graph_scene.regions.to_string())
            .with_field("scene-foreground-pins", snap.graph_scene.foreground_pins.to_string())
            .with_field("scene-retained", snap.graph_scene.retained.to_string())
            .with_field("scene-grammar", snap.graph_scene.grammar.to_string())
            .with_field("scene-territory", snap.graph_scene.territory.to_string())
            .with_field("focus", snap.focus)
            .with_field("saved-sources-status", self.app.saved_sources.status.clone())
            .with_field("saved-reader-state", saved_reader_state)
            .with_field("saved-reader-text", saved_reader_text)
            .with_field("saved-reader-quote", saved_reader_quote)
            .with_field("saved-reader-manifest", saved_reader_manifest)
            .with_field("saved-reader-origin", saved_reader_origin)
            .with_field("saved-reader-selector", saved_reader_selector)
            .with_field(
                "host-surface-producers",
                self.content_sessions.surface_count().to_string(),
            )
            .with_field("host-cue-frames", self.cue_frames.ready_count().to_string())
            .with_field("node-count", snap.node_count.to_string())
            .with_field("network-offline", self.network_offline.to_string())
            .with_field("peer-services", (usize::from(self.publish_service.is_some()) + usize::from(self.shared_knot_service.is_some())).to_string())
            .with_field("offline-acquisition-attempts", self.offline_attempts.load(std::sync::atomic::Ordering::Relaxed).to_string())
            .with_field("producer-retention", self.app.producer_retention_action().map(|(label, _)| label).unwrap_or_else(|| "Unavailable".into()))
            .with_field("retained-voice", self.app.redshank.retained_voice_preview()
                .map(|p| format!("{} {:?} {} ms", p.id, p.state, p.position_ms)).unwrap_or_else(|| "Ready".into()))
            .with_field("inline-roster-tabs", {
                let mut tabs: Vec<_> = self.renderers.roster.iter().map(|(pane, view)|
                    (pane.0, view.selected_tab().1)).collect();
                tabs.sort_by_key(|(pane, _)| *pane);
                tabs.into_iter().map(|(pane, label)| format!("{pane}:{label}"))
                    .collect::<Vec<_>>().join(",")
            })
            .with_field("roster-tab", snap.roster_tab)
            .with_field("native-texture-count", self.renderers.native_images.values().map(|images| images.len()).sum::<usize>().to_string())
            .with_field("inline-component-count", self.surface_plan().iter().filter(|surface|
                self.app.blueprint_space(crate::action::SpaceRef::Primary).is_none() &&
                matches!(surface.kind, crate::surface::SurfaceKind::Pane(pane) if self.renderers.inline_mount(pane).is_some())).count().to_string())
            // The panes and surfaces as joined tags, so a generic scenario can
            // `assert snap panes ~ roster` without an app-specific verb. This is
            // minimal-shared-and-grow: the app adds the fields its scenarios name.
            .with_field("panes", snap.panes.join(","))
            .with_field("surfaces", snap.surfaces.join(","))
            .with_field("floats", snap.floating_panes.join(","))
            .with_field("float-count",snap.floating_panes.len().to_string())
            .with_field("lens-float-count",snap.lens_floating_panes.len().to_string())
            .with_field("lens-floats", snap.lens_floating_panes.join(","))
            // What the app will DO right now, by label — the automation half of
            // a coherent snapshot. `assert snap actions ~ Fit to view` asks whether
            // a verb is on offer before spending a step on it.
            .with_field("actions", snap.available_actions.join(","))
            // The omnibar's offered rows as their display strings, so a
            // recall receipt can name the row it expects ("recall <url>")
            // without a verb of its own — the same minimal-shared-and-grow
            // rule the panes and actions fields follow.
            .with_field("suggestions", snap.omnibar.suggestions.join(","))
            .with_field("kept", kept.to_string())
            // The person's command-menu choices by id, as the view sidecar
            // stores them: the bare `>` lane is bounded by the row limit, so a
            // kept command can be stored and still sit past the visible rows.
            .with_field("commands-added", self.app.command_choices.added.join(","))
            .with_field(
                "commands-removed",
                self.app.command_choices.removed.join(","),
            );
        out = out
            .with_field(
                "appearance-theme",
                appearance.theme_id().unwrap_or_default(),
            )
            .with_field(
                "appearance-mode",
                appearance.theme_mode().unwrap_or_default(),
            )
            .with_field(
                "appearance-workshop-open",
                self.theme_editor.is_some().to_string(),
            )
            .with_field(
                "appearance-workshop-ok",
                self.main_receipt
                    .workshop
                    .map(|ok| ok.to_string())
                    .unwrap_or_default(),
            )
            .with_field("presented-frames", self.main_receipt.presented.to_string())
            .with_field(
                "capture-count",
                self.main_receipt.captures.len().to_string(),
            )
            .with_field("capture-errors", self.main_receipt.errors.len().to_string());
        let appearance_sheet = match self
            .app
            .shell_chrome_config()
            .appearance
            .theme_presentation
            .as_ref()
        {
            Some(tabard::ThemePresentation::AuthoredStylesheet(sheets)) => sheets.join("\n"),
            _ => String::new(),
        };
        out = out
            .with_field("appearance-css", appearance_sheet)
            .with_field(
                "frame-focus-slot",
                self.outer_focus
                    .as_ref()
                    .map(|f| f.slot.to_string())
                    .unwrap_or_else(|| "none".into()),
            )
            .with_field("a11y-lens-windows", self.lens_windows.len().to_string())
            .with_field(
                "frame-control",
                self.outer_focus
                    .as_ref()
                    .and_then(|focus| {
                        self.outer_frames
                            .borrow()
                            .get(&focus.slot)
                            .and_then(|frame| {
                                frame.controls.focus().map(|control| format!("{control:?}"))
                            })
                    })
                    .unwrap_or_else(|| "none".into()),
            );
        if self
            .app
            .blueprint_space(crate::action::SpaceRef::Primary)
            .is_none()
        {
            if let Some(frame) = self.outer_frames.borrow().get(&0) {
                out = out
                    .with_field("frame-css", frame.stylesheet())
                    .with_field("frame-header-height", frame.header_height().to_string());
            }
        }
        #[cfg(all(feature = "scry", windows))]
        {
            let ready = self
                .scry_frame_importers
                .values()
                .filter(|importer| importer.stats().frames > 0)
                .count();
            out = out
                .with_field(
                    "host-scry-producers",
                    self.scry_frame_importers.len().to_string(),
                )
                .with_field("host-scry-imported-pages", ready.to_string());
        }
        #[cfg(all(
            feature = "servo",
            any(windows, target_os = "macos", target_os = "linux")
        ))]
        {
            out = out
                .with_field(
                    "host-servo-views",
                    super::super::servo::active_views().to_string(),
                )
                .with_field(
                    "host-servo-profile",
                    self.servo_factory
                        .as_ref()
                        .map_or("", |factory| factory.profile_name()),
                )
                .with_field(
                    "host-servo-a11y-focus",
                    self.servo_factory
                        .as_ref()
                        .map_or("", |factory| factory.a11y_focus_policy()),
                );
        }
        let (foreign_surfaces, foreign_trees, foreign_nodes) = self.surface_a11y.published_counts();
        out = out
            .with_field("host-foreign-a11y-surfaces", foreign_surfaces.to_string())
            .with_field("host-foreign-a11y-trees", foreign_trees.to_string())
            .with_field("host-foreign-a11y-nodes", foreign_nodes.to_string())
            // These observe successful in-process composition. The OS adapter
            // may be inactive; neither the counts nor text qualify native AT.
            .with_field(
                "host-foreign-a11y-text",
                if public_fixture_a11y_receipt().unwrap_or(false) {
                    self.surface_a11y.published_text()
                } else {
                    String::new()
                },
            );
        if let Some(find) = snap.document_find {
            out = out
                .with_field("document-find-query", find.query)
                .with_field("document-find-caret", find.caret.to_string())
                .with_field("document-find-selection", find.selection)
                .with_field("document-find-count", find.count.to_string())
                .with_field(
                    "document-find-current",
                    find.current
                        .map_or_else(String::new, |index| (index + 1).to_string()),
                )
                .with_field("document-find-status", find.status);
        }
        if let Some(capabilities) = snap.document_capabilities {
            out = out
                .with_field("document-find-capability", capabilities.find_in_page)
                .with_field("page-zoom-capability", capabilities.page_zoom)
                .with_field("page-capture-capability", capabilities.page_capture)
                .with_field("navigation-capability", capabilities.navigation);
        }
        // The requested scale is always known for a focused node; the applied
        // half appears only for engines that read their effective value back,
        // so a scenario asserting `page-zoom-applied` is also asserting the
        // read-back seam exists.
        if let Some(node) = snap.focused.as_ref() {
            out = out.with_field("page-zoom-requested", node.page_zoom_percent.to_string());
            out = out.with_field(
                "host-focused-engine",
                self.app
                    .content
                    .facts(node.member)
                    .map(|facts| facts.engine.clone())
                    .unwrap_or_default(),
            );
            if let Some(applied) = node.applied_page_zoom_percent {
                out = out.with_field("page-zoom-applied", applied.to_string());
            }
        }
        // Page zoom is a DOCUMENT scale. The other two scales a receipt could
        // confuse it with are the chrome's UI zoom and the graph camera's
        // zoom, so both are projected here: an E0.2 receipt asserts they held
        // across a page-zoom step rather than trusting that they did. Chrome
        // zoom is a percentage like the page-zoom pair; the camera keeps three
        // decimals, because it is a continuous gesture value and not a ladder.
        out = out.with_field(
            "ui-zoom",
            ((self.app.shell_chrome_config().appearance.ui_zoom.max(0.0) * 100.0).round() as i64)
                .to_string(),
        );
        if let Some(zoom) = self
            .app
            .graph_for_pane(self.app.default_graph_pane())
            .and_then(|graph| self.app.graph_runtimes.canvas(graph))
            .map(|canvas| canvas.viewport().zoom)
        {
            out = out.with_field("canvas-zoom", format!("{zoom:.3}"));
        }
        if let Some(decision) = snap.user_agent_decision {
            out = out
                .with_field("decision-kind", decision.kind)
                .with_field("decision-node", decision.node.to_string())
                .with_field("decision-request", decision.request.to_string())
                .with_field("decision-prompt", decision.prompt)
                .with_field("decision-queued", decision.queued.to_string())
                .with_field("decision-submitting", decision.submitting.to_string())
                .with_field(
                    "decision-auth-field",
                    decision.authentication_field.unwrap_or_default(),
                )
                .with_field(
                    "decision-process-memory",
                    decision.remember_for_process.to_string(),
                );
        }
        // Fold the url in with the caption, so `assert snap focused ~ example.com`
        // can name the navigated address, not only the display caption.
        out.focused = snap.focused.map(|n| format!("{}  {}", n.caption, n.url));
        // Browser commands and page input have a pane/member owner independent
        // of the graph cursor. Keep the graph snapshot's meaning intact.
        if let Some((member, url)) = self.app.browser_command_target() {
            let title = self
                .app
                .graph_runtimes
                .graph_containing_member(member)
                .and_then(|graph| self.app.graph_runtimes.canvas(graph))
                .and_then(|canvas| canvas.graph().get_node_by_id(member))
                .map(|(_, node)| node.title.clone())
                .unwrap_or_default();
            let state = self
                .app
                .content
                .get(member)
                .map_or("closed", |state| match state {
                    crate::content::NodeContent::Live => "live",
                    crate::content::NodeContent::Requested => "requested",
                    crate::content::NodeContent::Failed(_) => "failed",
                    _ => "awaiting-input",
                });
            out = out
                .with_field("browser-current", format!("{title}  {url}"))
                .with_field("browser-current-content", state)
                .with_field(
                    "browser-page-zoom-requested",
                    crate::app::page_zoom_percent(
                        self.app
                            .browser
                            .get(member)
                            .and_then(|state| state.page_scale),
                    )
                    .to_string(),
                );
            if let Some(facts) = self.app.content.facts(member) {
                out = out
                    .with_field("browser-current-engine", facts.engine.clone())
                    .with_field(
                        "browser-find-capability",
                        facts.capabilities.find_in_page.describe(),
                    )
                    .with_field(
                        "browser-zoom-capability",
                        facts.capabilities.page_zoom.describe(),
                    );
            }
        }
        out
    }

    pub(super) fn adapter_drain_events(&mut self) -> Vec<String> {
        self.observed_events.drain(..).collect()
    }

    pub(super) fn adapter_act(&mut self, label: &str) -> bool {
        // Resolve against THE catalog the palette offers, in its order, so a
        // scenario acts on exactly what a person would see and pick. An exact
        // label wins anywhere before any prefix is considered; ties inside each
        // pass go to the earlier row, which is the contextual one. (This used to
        // resolve static-first while the palette showed dynamic-first — the two
        // could have disagreed about a shadowed label.) The prefix pass keeps
        // `act Switch to session` working without spelling out the whole row.
        let rows = self.app.available_actions();
        let action = rows
            .iter()
            .find(|(l, _)| l == label)
            .or_else(|| rows.iter().find(|(l, _)| l.starts_with(label)))
            .map(|(_, action)| action.clone());
        match action {
            Some(action) => {
                Shell::act(self, action);
                true
            },
            None => false,
        }
    }
}
