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
            Effect::ImportClipPackage(path) => {
                let admitted = self.app.clip_destination().and_then(|root| {
                    crate::retained_clip::import_sealed_with_limits(
                        &root,
                        &path,
                        crate::retained_clip::limits()?,
                        crate::denizen::now_ms(),
                        self.app.clip_sealer()?,
                        &self.app.research.storage_limits()?,
                    )
                });
                match admitted {
                    Ok(id) => {
                        self.act(Action::OpenAddress(crate::retained_clip::address(id)));
                        self.run_effects(vec![Effect::SaveSession]);
                    },
                    Err(error) => self.app.note_retained_clip_refusal(error),
                }
            },
            Effect::ControlRetainedClip(control) => {
                use crate::retained_clip::ClipRequest;
                let result = self.app.clip_destination().and_then(|root| match control {
                    ClipRequest::Play(id) => {
                        let clip = crate::retained_clip::read_sealed_with_limits(
                            &root,
                            id,
                            crate::retained_clip::limits()?,
                            self.app.clip_sealer()?,
                            &self.app.research.storage_limits()?,
                        )?;
                        let crate::retained_clip::ClipBody::Voice(bytes) = clip.body else {
                            return Err("This clip has no voice body".into());
                        };
                        self.app
                            .redshank
                            .play_retained_voice_bytes(&id.0.to_hex(), bytes)
                    },
                    ClipRequest::Stop(id) => self.app.redshank.stop_retained_voice(&id.0.to_hex()),
                    ClipRequest::Read(_) | ClipRequest::Export(_) => {
                        Err("Unsupported retained clip control".into())
                    },
                });
                if let Err(error) = result {
                    self.app.note_retained_clip_refusal(error);
                }
            },
            Effect::ChooseClipExportFile { clip } => {
                let available = self.app.clip_destination().and_then(|root| {
                    crate::retained_clip::read_sealed_with_limits(
                        &root,
                        clip,
                        crate::retained_clip::limits()?,
                        self.app.clip_sealer()?,
                        &self.app.research.storage_limits()?,
                    )
                    .map(|_| ())
                });
                if let Err(error) = available {
                    self.app.note_retained_clip_refusal(error);
                    return;
                }
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Export selected clip")
                    .add_filter("Selected clip package", &["mereclip"])
                    .set_file_name("selected-clip.mereclip")
                    .save_file()
                {
                    // The native save dialog owns its overwrite confirmation.
                    self.act(Action::ExportClipPackage {
                        clip,
                        path,
                        replace: true,
                    });
                }
            },
            Effect::ExportClipPackage {
                clip,
                path,
                replace,
            } => {
                let result = self.app.clip_destination().and_then(|root| {
                    crate::retained_clip::export_sealed_with_limits(
                        &root,
                        clip,
                        &path,
                        crate::retained_clip::limits()?,
                        replace,
                        self.app.clip_sealer()?,
                        &self.app.research.storage_limits()?,
                    )
                });
                match result {
                    Ok(()) => self.app.note_clip_package_exported(clip, &path),
                    Err(error) => self.app.note_retained_clip_refusal(error),
                }
            },
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
