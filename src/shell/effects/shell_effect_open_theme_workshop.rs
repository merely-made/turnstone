// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn effect_open_theme_workshop(&mut self, effect: Effect) {
        match effect {
            Effect::OpenThemeWorkshop => self.theme_editor_requested = true,
            Effect::OpenAlembicWorkshop => self.alembic_workshop_requested = true,
            Effect::ReplaceGeminiTrust {
                node,
                fetch_url,
                owner_url,
                target,
                pinned,
                seen,
            } => match self.gemini_trust.accept_change(&target, &pinned, &seen) {
                Ok(()) => {
                    let fetch = self.app.fetch_page_effect(node, fetch_url, owner_url);
                    for command in browse::fetch_commands_for(&fetch, &mut self.pending_fetches) {
                        self.command_fetch(command);
                    }
                },
                Err(error) => {
                    let effects = self.app.apply_update(Update::ContentFailed {
                        node,
                        error: format!("could not replace Gemini trust for {target}: {error}"),
                    });
                    self.run_effects(effects);
                },
            },
            Effect::SaveSession => self.save_session(),
            Effect::PersistSettingsPaneRecord { pane, request_id } => {
                let result = self.persist_settings_pane_record(pane);
                self.settings_pane_record_result(pane, request_id, result);
            },
            Effect::SettingsPaneRecordResult {
                pane,
                request_id,
                result,
            } => self.settings_pane_record_result(pane, request_id, result),
            Effect::SaveChromeLayout(layout) => {
                if let Err(error) = layout.save(&self.app.data_root) {
                    tracing::warn!(%error, "chrome placement could not be saved");
                }
            },
            Effect::SaveGraphFolds { graph } => self.save_graph_folds(graph),
            Effect::CopyText(text) => {
                use genet_clipboard::TextClipboard as _;
                let wrote = match &mut self.clipboard {
                    Ok(clipboard) => clipboard.set_text(&text).map_err(|e| e.to_string()),
                    Err(why) => Err(why.clone()),
                };
                if let Err(error) = wrote {
                    tracing::warn!(%error, "the clipboard refused a copy");
                    let follow_up = self.app.refuse_place(format!("clipboard: {error}"));
                    self.run_effects(follow_up);
                }
            },
            Effect::CreateResearchNote { generation } => self.create_research_note(generation),
            Effect::ChooseResearchNote { choice } => self.choose_research_note(choice),
            Effect::ChooseKnotDocumentFile { read_only } => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(
                        "Djot, Scrolltext, Gemtext, Micron, or legacy .knot",
                        &["knot", "djot", "scroll", "gmi", "gemini", "mu", "micron"],
                    )
                    .pick_file()
                {
                    let source = if read_only {
                        crate::knot_document_surface::read_only_file_source(path)
                    } else {
                        crate::knot_document_surface::file_source(path)
                    };
                    self.act(Action::SummonContributedPane {
                        kind: crate::panes::PaneKindId::new(
                            crate::knot_document_surface::PANE_KIND,
                        ),
                        source,
                    });
                }
            },
            Effect::RetainProducerSelection { choice, selection } => {
                self.retain_producer_selection(choice, selection)
            },
            Effect::ChooseClipPackageFile => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Selected clip package", &["json", "mereclip"])
                    .pick_file()
                {
                    self.act(Action::ImportClipPackage(path));
                }
            },
            Effect::ImportClipPackage(path) => self.request_clip_import(path),
            Effect::ControlRetainedClip(control) => match control {
                crate::retained_clip::ClipRequest::Play(id) => {
                    self.request_clip_action_read(id, super::super::clip_actions::ReadKind::Play)
                },
                crate::retained_clip::ClipRequest::Stop(id) => {
                    self.cancel_pending_clip_play(id);
                    if let Err(error) = self.app.redshank.stop_retained_voice(&id.0.to_hex()) {
                        self.app.note_retained_clip_refusal(error);
                    }
                },
                _ => self
                    .app
                    .note_retained_clip_refusal("Unsupported retained clip control".into()),
            },
            Effect::ChooseClipExportFile { clip } => self
                .request_clip_action_read(clip, super::super::clip_actions::ReadKind::ChooseExport),
            Effect::ExportClipPackage {
                clip,
                path,
                replace,
            } => self.request_clip_export(clip, path, replace),
            Effect::OpenPlace {
                session,
                generation,
                binding,
            } => {
                self.place_handle
                    .command(crate::place::worker::PlaceWorkerCommand::Open {
                        session,
                        generation,
                        directory: session::session_dir(&self.app.data_root, session),
                        binding,
                    });
            },
            Effect::JoinPlace {
                session,
                generation,
                invite,
            } => {
                self.place_handle
                    .command(crate::place::worker::PlaceWorkerCommand::Join {
                        session,
                        generation,
                        directory: session::session_dir(&self.app.data_root, session),
                        invite,
                    });
            },
            Effect::FoundPlace {
                session,
                generation,
                name,
            } => {
                self.place_handle
                    .command(crate::place::worker::PlaceWorkerCommand::Found {
                        session,
                        generation,
                        directory: session::session_dir(&self.app.data_root, session),
                        name,
                    });
            },
            Effect::OfferPlacePrekey {
                session,
                generation,
                moot,
            } => {
                self.place_handle
                    .command(crate::place::worker::PlaceWorkerCommand::OfferPrekey {
                        session,
                        generation,
                        directory: session::session_dir(&self.app.data_root, session),
                        moot,
                    });
            },
            Effect::InvitePlace {
                session,
                generation,
                prekey,
                access,
                lifetime_ms,
            } => {
                self.place_handle
                    .command(crate::place::worker::PlaceWorkerCommand::Invite {
                        session,
                        generation,
                        directory: session::session_dir(&self.app.data_root, session),
                        prekey,
                        access,
                        lifetime_ms,
                    });
            },
            Effect::WritePlaceArtifact { path, bytes } => {
                if let Err(error) = std::fs::write(&path, &bytes) {
                    let follow_up = self.app.refuse_place(format!("write {path}: {error}"));
                    self.run_effects(follow_up);
                }
            },
            Effect::ReadPlaceArtifact { path, kind } => match read_place_artifact(&path, kind) {
                Ok(action) => self.act(action),
                Err(error) => {
                    let follow_up = self.app.refuse_place(error);
                    self.run_effects(follow_up);
                },
            },
            Effect::LeavePlace {
                session,
                generation,
                retained,
            } => {
                if self.app.session_id != session || self.app.place.generation() != Some(generation)
                {
                    return;
                }
                let released = self.release_place_worker_result();
                let result = released.and_then(|()| {
                    session::mark_place_left(
                        &session::session_dir(&self.app.data_root, session),
                        &retained,
                    )
                    .map(|()| true)
                    .map_err(|error| error.to_string())
                });
                let follow_up = self
                    .app
                    .finish_leave_place(session, generation, retained, result);
                self.run_effects(follow_up);
            },
            Effect::ReconnectPlace {
                session,
                generation,
                binding,
                contact,
            } => {
                if self.app.session_id != session || self.app.place.generation() != Some(generation)
                {
                    return;
                }
                self.place_handle
                    .command(crate::place::worker::PlaceWorkerCommand::Reconnect {
                        session,
                        generation,
                        directory: session::session_dir(&self.app.data_root, session),
                        binding,
                        contact,
                    });
            },
            Effect::ClearPlaceLeftMark { session } => {
                if let Err(error) =
                    session::clear_place_left(&session::session_dir(&self.app.data_root, session))
                {
                    tracing::warn!(%error, "failed to clear the place-left mark on rejoin");
                }
            },
            Effect::RunPlaceCommand {
                session,
                generation,
                request,
                command,
            } => {
                self.place_handle
                    .command(crate::place::worker::PlaceWorkerCommand::Author {
                        session,
                        generation,
                        request,
                        command,
                    });
            },
            Effect::ResyncPlace {
                session,
                generation,
            } => {
                self.place_handle
                    .command(crate::place::worker::PlaceWorkerCommand::Resync {
                        session,
                        generation,
                    });
            },
            Effect::SetPlaceCollection {
                session,
                generation,
                selection,
            } => {
                self.place_handle.command(
                    crate::place::worker::PlaceWorkerCommand::SetCollection {
                        session,
                        generation,
                        selection,
                    },
                );
            },
            Effect::ClosePlace { .. } => self.release_place_worker(),
            Effect::StoreImage { hex, bytes } => {
                session::save_image_blob(&self.app.session_dir(), &hex, &bytes);
            },
            Effect::StoreDownload {
                node,
                url,
                content_type,
                content_disposition,
                received_at_ms,
                bytes,
            } => {
                self.download_handle
                    .command(crate::download::DownloadCommand {
                        node,
                        url,
                        content_type,
                        content_disposition,
                        received_at_ms,
                        session_dir: self.app.session_dir(),
                        download_dir: crate::download::configured_download_dir(&self.app.data_root),
                        bytes,
                    });
            },
            Effect::RecordDeleted { record } => {
                self.bin_handle
                    .command(crate::recycle::BinCommand::Record(record));
            },
            Effect::EmptyRecycleBin => {
                self.bin_handle.command(crate::recycle::BinCommand::Empty);
            },
            Effect::RecallQuery { query } => {
                let sources = recall_sources(&self.app);
                let config = crate::trail_memory::RecallConfig::default();
                self.trail_handle
                    .command(crate::trail_memory::TrailCommand::Recall {
                        query,
                        limit: RECALL_ROW_LIMIT,
                        sources,
                        config,
                    });
            },
            Effect::RecordPageText { url, text } => {
                self.trail_handle
                    .command(crate::trail_memory::TrailCommand::RecordText { url, text });
            },
            _ => unreachable!("dispatch group disagrees with exhaustive routing"),
        }
    }
}
