// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    /// Execute one turnstone scenario step against the Shell — the app-specific
    /// verbs the shared taproot loop hands to `Driveable::app_step`. This is
    /// turnstone's former `scenario.rs` `tick()` (asserts) and `scenario_pump`'s
    /// `Tick` execution (interactions), unified into one pass: an assert reads
    /// the observation snapshot and returns `Err` on mismatch; an interaction
    /// drives the Shell directly. The generic verbs (act/settle/capture/log,
    /// assert event/text/snap) never arrive — the shared loop owns them; their
    /// arms below are defensive.
    pub(super) fn run_scenario_step(&mut self, step: &crate::scenario::Step) -> Result<(), String> {
        use crate::action::CaretMove;
        use crate::scenario::{CmpOp, EditKey, Step};

        match step {
            Step::Open(..)
            | Step::SelectGraphMember(..)
            | Step::ClickNoteMember
            | Step::ActivateGraphFold(..)
            | Step::DoubleClickGraphNode(..)
            | Step::RecordGraphScene(..)
            | Step::RecordGraphFolds(..)
            | Step::ImportClip(..)
            | Step::ExportClip(..)
            | Step::Omnibar { .. }
            | Step::Type(..)
            | Step::Insert(..)
            | Step::Key(..)
            | Step::KeyWindow(..)
            | Step::Script(..)
            | Step::Click(..)
            | Step::ClickContentWindow(..)
            | Step::Press(..)
            | Step::Release(..)
            | Step::Move(..)
            | Step::Touch(..)
            | Step::RightClick(..)
            | Step::ClickRow(..)
            | Step::ClickTab(..)
            | Step::ClickNode(..)
            | Step::ClickReadingReference(..)
            | Step::Drag(..)
            | Step::DragTab(..)
            | Step::DragTabOut(..)
            | Step::HoverFile(..)
            | Step::DropFile(..)
            | Step::Scroll(..)
            | Step::ScrollReader(..)
            | Step::Divider(..)
            | Step::RecordReaderAppearances(..)
            | Step::RecordIdle(..)
            | Step::AssertOmnibar(..)
            | Step::AssertScrolled(..)
            | Step::AssertText(..) => self.scenario_open(step)?,
            Step::AssertFocused(..)
            | Step::AssertBrowserFetch(..)
            | Step::AssertLinkPreview(..)
            | Step::AssertSurface(..)
            | Step::AssertFocus(..)
            | Step::AssertPane(..)
            | Step::AssertMaximized(..)
            | Step::AssertNoRow(..)
            | Step::AssertRow(..)
            | Step::AssertTab(..)
            | Step::AssertRatio(..)
            | Step::AssertActiveRatio(..)
            | Step::AssertSuggestions(..)
            | Step::AssertVisible
            | Step::AssertContentLive
            | Step::AssertWbCells(..)
            | Step::AssertWbCell(..)
            | Step::AssertWbFraction(..)
            | Step::AssertWindows(..)
            | Step::AssertSessions(..)
            | Step::AssertSession(..)
            | Step::AssertNodes(..)
            | Step::AssertEnergy(..)
            | Step::AssertPhysicsRunning(..)
            | Step::AssertOverlaps(..)
            | Step::AssertA11y(..)
            | Step::CapturesDiffer(..)
            | Step::ScrollPaneWindow(..)
            | Step::Act(..)
            | Step::Settle(..)
            | Step::Log(..)
            | Step::QueueAct(..)
            | Step::DeliverAct
            | Step::Capture(..)
            | Step::CaptureLens(..)
            | Step::AssertLensPane(..)
            | Step::AssertNoPane(..)
            | Step::AssertLensSurface(..)
            | Step::AssertNoLensPane(..) => self.scenario_assert_focused(step)?,
            Step::AssertNoSurface(..)
            | Step::AssertReaderAppearances(..)
            | Step::AssertReaderSources(..)
            | Step::AssertReaderRole(..)
            | Step::AssertReaderViewport(..)
            | Step::AssertReaderScroll(..)
            | Step::AssertContentSessionsIdle
            | Step::AssertContentReady
            | Step::AssertEvent(..)
            | Step::WaitRow(..)
            | Step::WaitStatus(..)
            | Step::WaitKnot(..)
            | Step::WaitSourceCapture(..)
            | Step::WaitSavedSources(..)
            | Step::WaitResearchSearch(..) | Step::WaitSourceTarget(..)
            | Step::WaitStructuredResult(..)
            | Step::WaitSavedReader(..)
            | Step::WaitFile(..)
            | Step::RecordPlace(..)
            | Step::RecordSourceCaptures(..)
            | Step::RecordSavedSources(..)
            | Step::RecordStructuredResearch(..)
            | Step::RecordStructuredStatus(..)
            | Step::RecordRuntime(..)
            | Step::RecordComparison(..)
            | Step::ChooseSuggestion(..)
            | Step::AssertNoAction(..)
            | Step::ChooseCapturedSource { .. }
            | Step::ChooseSavedResult(..)
            | Step::TouchFile(..) => self.scenario_assert_no_surface(step)?,
        }
        self.request_redraw();
        Ok(())
    }

    pub(super) fn scenario_open(&mut self, step: &crate::scenario::Step) -> Result<(), String> {
        use super::super::drive::{cmp_f32, cmp_usize};
        use crate::action::CaretMove;
        use crate::scenario::{CmpOp, EditKey, Step};
        match step {
            Step::Open(url) => self.act(Action::OpenAddress(url.clone())),
            Step::SelectGraphMember(url) => {
                let pane = self.app.graph_command_pane().ok_or("no graph owner pane")?;
                self.app
                    .with_graph_pane(pane, |canvas| {
                        if url == "none" {
                            canvas.clear_selection();
                            return Ok(());
                        }
                        let id = canvas
                            .graph()
                            .get_node_by_url(url)
                            .map(|(_, node)| node.id)
                            .ok_or("source member absent")?;
                        if canvas.fold_of_member(id).is_some() {
                            return Err("member already folded");
                        }
                        canvas.toggle_select_member(id);
                        Ok(())
                    })
                    .ok_or("graph absent")??;
                self.app.focus = crate::surface::FocusTarget::Graph(pane);
                self.request_redraw();
            },
            Step::ClickNoteMember => self.scenario_click_note_member()?,
            Step::ActivateGraphFold(index) => {
                let pane = self.app.graph_command_pane().ok_or("no graph owner pane")?;
                let badge = self
                    .app
                    .with_graph_pane(pane, |canvas| canvas.fold_badges().get(*index).cloned())
                    .flatten()
                    .ok_or("fold badge absent")?;
                self.app.graph_pane_pointer_down(
                    pane,
                    mere::canvas::PointerButton::Left,
                    badge.at.0,
                    badge.at.1,
                );
                self.release_graph_pointer(
                    pane,
                    mere::canvas::PointerButton::Left,
                    badge.at.0,
                    badge.at.1,
                );
                self.request_redraw();
            },
            Step::DoubleClickGraphNode(address) => {
                let pane = self.app.graph_command_pane().ok_or("no graph owner pane")?;
                let point = self
                    .app
                    .with_graph_pane(pane, |canvas| {
                        let (key, _) = canvas.graph().get_node_by_url(address)?;
                        canvas.screen_position_of(key)
                    })
                    .flatten()
                    .ok_or("graph node is not painted")?;
                let scale = self.app.pane_layout_scale(pane);
                let surface = self
                    .surface_plan()
                    .into_iter()
                    .find(|s| s.kind == crate::surface::SurfaceKind::Graph(pane))
                    .ok_or("graph surface is not visible")?;
                let (x, y) = (
                    surface.rect.x + point.0 * scale,
                    surface.rect.y + point.1 * scale,
                );
                if !surface.rect.contains(x, y) {
                    return Err("graph node is outside its painted surface".into());
                }
                self.deliver_move(x, y);
                for _ in 0..2 {
                    self.deliver_press(x, y, winit::event::MouseButton::Left);
                    self.deliver_release(x, y, winit::event::MouseButton::Left);
                }
                self.request_redraw();
            },
            Step::RecordGraphScene(name) => {
                let pane = self.app.scene_command_pane().ok_or("no graph owner pane")?;
                let graph = self.app.graph_for_pane(pane).ok_or("graph absent")?;
                let value = self
                    .app
                    .with_graph_pane(pane, |canvas| {
                        serde_json::json!({
                            "facts": crate::graph_scenes::facts(canvas),
                            "geometry": canvas.cartography_geometry(),
                            "graph": canvas.graph().to_snapshot_at(0),
                            "folds": canvas.fold_records(),
                            "physics_paused": canvas.physics_paused(),
                            "selected": canvas.selected_members(),
                            "graph_id": graph.as_uuid(),
                        })
                    })
                    .ok_or("graph absent")?;
                let runtime = self.app.graph_runtimes.get(graph).ok_or("runtime absent")?;
                let session = runtime.session.ok_or("session absent")?;
                let dir = crate::session::session_dir(&self.app.data_root, session);
                let mut value = value;
                value["main_presented_frames"] = serde_json::json!(self.main_receipt.presented);
                for file in ["facets.json", "view-intent.json"] {
                    if let Ok(bytes) = std::fs::read(dir.join(file)) {
                        value[format!("stored_{file}")] =
                            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                    }
                }
                std::fs::write(
                    self.shared_out_dir.join(format!("{name}.scene.json")),
                    serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
            },
            Step::RecordGraphFolds(name, expected) => {
                let pane = self.app.graph_command_pane().ok_or("no graph owner pane")?;
                let graph = self.app.graph_for_pane(pane).ok_or("graph absent")?;
                let value = self
                    .app
                    .with_graph_pane(pane, |canvas| {
                        if canvas.fold_records().len() != *expected {
                            return Err(format!(
                                "expected {expected} folds, got {}",
                                canvas.fold_records().len()
                            ));
                        }
                        let mut positions: Vec<_> = canvas
                            .graph()
                            .nodes()
                            .map(|(key, node)| {
                                let pos = canvas.world_position_of(key).map(|p| [p.x, p.y]);
                                (node.id.to_string(), pos)
                            })
                            .collect();
                        positions.sort_by(|a, b| a.0.cmp(&b.0));
                        let routes: Vec<_> = canvas
                            .fold_routes()
                            .iter()
                            .map(|route| {
                                serde_json::json!({
                                    "from": format!("{:?}", route.from),
                                    "to": format!("{:?}", route.to),
                                    "family": format!("{:?}", route.family),
                                    "count": route.count,
                                })
                            })
                            .collect();
                        let relations: Vec<_> = canvas
                            .graph()
                            .projected_relations()
                            .map(|(_, relation)| {
                                serde_json::json!({
                                    "from": format!("{:?}", relation.from),
                                    "to": format!("{:?}", relation.to),
                                    "family": format!("{:?}", relation.kind.family()),
                                })
                            })
                            .collect();
                        let member_keys: Vec<_> = canvas.graph().nodes().map(|(key, node)| {
                        serde_json::json!({"id": node.id, "key": format!("{:?}", key)})
                    }).collect();
                        Ok(serde_json::json!({
                            "member_keys": member_keys,
                            "graph": canvas.graph().to_snapshot_at(0),
                            "positions": positions,
                            "records": canvas.fold_records(),
                            "routes": routes,
                            "relations": relations,
                            "hidden": canvas.fold_hidden_keys().len(),
                            "physics_paused": canvas.physics_paused(),
                        }))
                    })
                    .ok_or("graph absent")??;
                let runtime = self.app.graph_runtimes.get(graph).ok_or("runtime absent")?;
                let session = runtime.session.ok_or("session absent")?;
                let dir = crate::session::session_dir(&self.app.data_root, session);
                let saved: serde_json::Value = serde_json::from_slice(
                    &std::fs::read(dir.join("folds.json")).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                if saved["records"] != value["records"] {
                    return Err("native and stored fold records differ".into());
                }
                let mut value = value;
                value["stored"] = saved;
                value["presented_frames"] = serde_json::json!(self.main_receipt.presented);
                for file in ["graph.json", "facets.json"] {
                    let bytes = std::fs::read(dir.join(file)).map_err(|e| e.to_string())?;
                    value[format!("stored_{file}_blake3")] =
                        serde_json::json!(blake3::hash(&bytes).to_hex().to_string());
                }
                value["graph_id"] = serde_json::json!(graph.as_uuid());
                value["session_id"] = serde_json::json!(session.as_uuid());
                std::fs::write(
                    self.shared_out_dir.join(format!("{name}.folds.json")),
                    serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
            },
            Step::ImportClip(path) => {
                self.act(Action::ImportClipPackage(std::path::PathBuf::from(path)))
            },
            Step::ExportClip(path) => {
                let clip = self.app.clip_export_target()?;
                self.act(Action::ExportClipPackage {
                    clip,
                    path: path.into(),
                    replace: false,
                });
            },
            Step::Omnibar { command } => self.act(Action::OmnibarOpen { command: *command }),
            Step::Type(text) => {
                if self.app.user_agent_decision.is_open() {
                    if self.app.user_agent_decision.accepts_text() {
                        self.act(Action::InsertAuthentication(text.clone()));
                    } else {
                        return Err("type: the active decision has no text field".into());
                    }
                } else if self.app.document_find.open {
                    for character in text.chars() {
                        let key = winit::keyboard::Key::Character(character.to_string().into());
                        if let Some(event) = self.cambium_key(&key) {
                            self.deliver_find_key(event);
                        }
                    }
                } else if !self.app.omnibar.open && self.type_surface_text(text) {
                    // Commit through the same producer key stream as winit.
                } else {
                    for c in text.chars() {
                        self.act(Action::OmnibarChar(c));
                    }
                }
            },
            Step::Insert(text) => {
                if self.app.user_agent_decision.is_open() {
                    if self.app.user_agent_decision.accepts_text() {
                        self.act(Action::InsertAuthentication(text.clone()));
                    } else {
                        return Err("insert: the active decision has no text field".into());
                    }
                } else if self.app.document_find.open {
                    self.deliver_find_key(cambium_winit::ime_event_from_winit(
                        &winit::event::Ime::Commit(text.clone()),
                    ));
                } else if self.app.omnibar.open {
                    self.act(Action::OmnibarInsert(text.clone()));
                } else if self.deliver_contributed_ime(&winit::event::Ime::Commit(text.clone())) {
                    self.request_redraw();
                } else if self.deliver_document_ime(&winit::event::Ime::Commit(text.clone())) {
                    self.request_redraw();
                } else if self.type_surface_text(text) {
                    // Hosted committed text is distinct from OS IME/preedit.
                } else {
                    return Err("insert: no focused text editor".into());
                }
            },
            Step::Key(key) => self.scenario_key_in_window(0, key)?,
            Step::KeyWindow(window, key) => self.scenario_key_in_window(*window, key)?,
            Step::Script(source) => self.run_scenario_script(source),
            Step::Click(x, y) => {
                self.deliver_press(*x, *y, MouseButton::Left);
                self.deliver_release(*x, *y, MouseButton::Left);
            },
            Step::ClickContentWindow(window, x, y) => {
                self.scenario_click_content_window(*window, *x, *y)?
            },
            Step::Press(x, y) => self.deliver_press(*x, *y, MouseButton::Left),
            Step::Release(x, y) => self.deliver_release(*x, *y, MouseButton::Left),
            Step::Move(x, y) => self.deliver_move(*x, *y),
            Step::Touch(id, phase, x, y, pressure) => {
                self.deliver_touch_contact(*id, *phase, *x, *y, *pressure, None)
            },
            Step::RightClick(x, y) => {
                self.deliver_press(*x, *y, MouseButton::Right);
                self.deliver_release(*x, *y, MouseButton::Right);
            },
            Step::ClickRow(substr) => self.click_pane_row(substr),
            Step::ClickTab(label) => self.click_pane_tab(label),
            Step::ClickNode(substr) => self.click_pane_node(substr),
            Step::ClickReadingReference(address) => {
                let target = self
                    .app
                    .focused_read_target()
                    .ok_or("No focused retained reading")?;
                self.sync_content_placements();
                let id = target.appearance(0);
                let surface = self
                    .surface_plan()
                    .into_iter()
                    .find(|s| s.id == id)
                    .ok_or("Reading appearance is not visible")?;
                let _ = self.content_sessions.pool.prepare(&id);
                let links = self
                    .content_sessions
                    .appearance(target.member, id)
                    .and_then(pelt_core::PeltContent::document)
                    .ok_or("No retained reading controller")?
                    .links();
                let link = links
                    .into_iter()
                    .find(|link| {
                        super::super::content_link_target(&self.app, target.member, &link.url)
                            == *address
                    })
                    .ok_or("Reference is not in the retained document")?;
                let scale = self
                    .content_sessions
                    .pool
                    .placement(&id)
                    .ok_or("No reading placement")?
                    .scale_factor;
                let [left, top, width, height] = link.rect;
                let (x, y) = (
                    surface.rect.x + (left + width * 0.5) * scale,
                    surface.rect.y + (top + height * 0.5) * scale,
                );
                if !surface.rect.contains(x, y) {
                    return Err(
                        "Reference is outside the visible reading; scroll or Find it first".into(),
                    );
                }
                self.deliver_move(x, y);
                self.deliver_press(x, y, MouseButton::Left);
                self.deliver_release(x, y, MouseButton::Left);
            },
            Step::Drag(from, to) => {
                self.deliver_press(from.0, from.1, MouseButton::Left);
                let mid = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
                self.deliver_move(mid.0, mid.1);
                self.deliver_move(to.0, to.1);
                self.deliver_release(to.0, to.1, MouseButton::Left);
            },
            Step::DragTab(from, onto, edge) => {
                self.drag_workbench_tab(from, onto, edge.as_deref());
            },
            Step::DragTabOut(from) => self.drag_workbench_tab_out(from),
            Step::HoverFile(x, y, path) => self.hover_file(*x, *y, std::path::Path::new(path)),
            Step::DropFile(x, y, path) => self.drop_file(*x, *y, std::path::Path::new(path)),
            Step::Scroll(x, y, dx, dy) => self.deliver_wheel(*x, *y, *dx, *dy),
            Step::ScrollReader(role, dy) => {
                let role = super::super::reader_observe::ReaderAppearanceRole::parse(role)
                    .ok_or_else(|| format!("scroll-reader: unknown Reader role '{role}'"))?;
                self.scroll_reader_appearance(role, *dy)?;
            },
            Step::Divider(ratio) => self.act(Action::SetActivePaneDivider(*ratio)),
            Step::RecordReaderAppearances(name) => {
                let observations = self.reader_appearance_observations();
                let path = self.shared_out_dir.join(format!("{name}.txt"));
                std::fs::write(
                    &path,
                    format!(
                        "RESULT ok\n{}\n",
                        Self::describe_reader_appearances(&observations)
                    ),
                )
                .map_err(|error| {
                    format!(
                        "record-reader-appearances '{}': could not write {}: {error}",
                        name,
                        path.display()
                    )
                })?;
            },
            Step::RecordIdle(name) => {
                let public_fixture = public_fixture_a11y_receipt()?;
                std::fs::write(
                    self.shared_out_dir.join(format!("{name}.cue-frames.json")),
                    serde_json::to_vec_pretty(&self.cue_frames.diagnostic())
                        .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                let diagnosis = self.idle_diagnosis();
                let path = self.shared_out_dir.join(format!("{name}.txt"));
                std::fs::write(&path, format!("RESULT ok\n{}\n", diagnosis.describe())).map_err(
                    |error| {
                        format!(
                            "record-idle '{}': could not write {}: {error}",
                            name,
                            path.display()
                        )
                    },
                )?;
                let path = self
                    .shared_out_dir
                    .join(format!("{name}.foreign-a11y.json"));
                let mut diagnostic = self.surface_a11y.publication_diagnostic(public_fixture);
                #[cfg(all(
                    feature = "servo",
                    any(windows, target_os = "macos", target_os = "linux")
                ))]
                if let Some(factory) = &self.servo_factory {
                    diagnostic["servo_focus_policy"] =
                        serde_json::json!(factory.a11y_focus_policy());
                }
                let bytes =
                    serde_json::to_vec_pretty(&diagnostic).map_err(|error| error.to_string())?;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|error| {
                        format!(
                            "record-idle '{name}': refusing to overwrite or unable to create {}: {error}",
                            path.display()
                        )
                    })?;
                std::io::Write::write_all(&mut file, &bytes).map_err(|error| error.to_string())?;
                #[cfg(all(any(feature = "scry", feature = "servo"), windows))]
                {
                    let surfaces = self.surface_plan().into_iter().filter_map(|surface| {
                        let crate::surface::SurfaceKind::Content(node) = surface.kind else { return None; };
                        let rect = surface.rect;
                        Some(serde_json::json!({"node":node,"surface_id":surface.id.0,"x":rect.x,"y":rect.y,"width":rect.w,"height":rect.h}))
                    }).collect::<Vec<_>>();
                    #[cfg(feature = "scry")]
                    let stats = self.scry_frame_importers.iter().map(|(node, importer)| {
                        let stats = importer.stats();
                        serde_json::json!({"node":node,"fence_handle":importer.fence_handle(),"frames":stats.frames,"imports":stats.imports,"waits":stats.waits})
                    }).collect::<Vec<_>>();
                    #[cfg(not(feature = "scry"))]
                    let stats: Vec<serde_json::Value> = Vec::new();
                    #[cfg(feature = "servo")]
                    let servo_views = super::super::servo::active_views();
                    #[cfg(not(feature = "servo"))]
                    let servo_views = 0usize;
                    std::fs::write(self.shared_out_dir.join(format!("{name}.surface-frames.json")),
                        serde_json::to_vec_pretty(&serde_json::json!({"live_producers":self.content_sessions.surface_count(),"cached_frames":self.surface_frames.len(),"scry_importers":stats,"servo_active_views":servo_views,"content_surfaces":surfaces})).map_err(|error|error.to_string())?)
                        .map_err(|error|error.to_string())?;
                }
            },
            Step::AssertOmnibar(open) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.omnibar.open != *open {
                    let state = if *open { "open" } else { "closed" };
                    return Err(format!("assert omnibar {state}: it is not"));
                }
            },
            Step::AssertScrolled(want_moved) => match self.content_scroll_moved {
                Some(moved) if moved == *want_moved => {},
                Some(_) => {
                    let (want, got) = if *want_moved {
                        ("moved", "did not")
                    } else {
                        ("still", "moved")
                    };
                    return Err(format!("assert scrolled {want}: the focused page {got}"));
                },
                None => {
                    return Err(
                        "assert scrolled: no content scroll key has been delivered".to_string()
                    );
                },
            },
            Step::AssertText(want) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.omnibar.text != *want {
                    return Err(format!(
                        "assert omnibar-text '{want}': the omnibar holds '{}'",
                        snap.omnibar.text
                    ));
                }
            },
            _ => unreachable!("dispatch group disagrees with exhaustive routing"),
        }
        Ok(())
    }
}
