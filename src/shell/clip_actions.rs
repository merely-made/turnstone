// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::retained_clip::owner_worker::Pending;

type ReadResult = Result<crate::retained_clip::ClipView, crate::research_storage::SourceReadError>;
#[derive(Clone)]
struct Snapshot {
    root: std::path::PathBuf,
    session: crate::panes::SessionId,
    graph: crate::panes::GraphId,
    generation: uuid::Uuid,
    identity: Arc<crate::identity::RootIdentity>,
    bound: Arc<crate::custody_binding::BoundSealer>,
    epoch: u64,
    clip: eidetic::ManifestId,
}
#[derive(Clone, Copy)]
pub(super) enum ReadKind {
    ChooseExport,
    Play,
}
pub(super) struct PendingClipAction {
    snapshot: Snapshot,
    kind: ReadKind,
    response: Pending<ReadResult>,
}
pub(super) struct PendingExportPrepare {
    snapshot: Snapshot,
    path: std::path::PathBuf,
    replace: bool,
    response: Pending<Result<crate::retained_clip::owner_worker::PreparedExport, String>>,
}
pub(super) struct PendingExportCommit {
    clip: eidetic::ManifestId,
    path: std::path::PathBuf,
    response: Pending<Result<(), String>>,
}

fn cancel_clip_play(
    actions: &mut HashMap<uuid::Uuid, PendingClipAction>,
    clip: eidetic::ManifestId,
) {
    actions.retain(|_, pending| {
        !(matches!(pending.kind, ReadKind::Play) && pending.snapshot.clip == clip)
    });
}
impl Shell {
    pub(super) fn cancel_pending_clip_play(&mut self, clip: eidetic::ManifestId) {
        cancel_clip_play(&mut self.pending_clip_actions, clip);
    }
    fn clip_action_snapshot(&self, clip: eidetic::ManifestId) -> Result<Snapshot, String> {
        if self.app.clip_export_target()? != clip {
            return Err("The selected retained clip changed".into());
        }
        let root = self.app.clip_destination()?;
        let identity = self.app.identity.clone().ok_or(crate::identity::PENDING)?;
        let binding = self
            .custody_binding
            .as_ref()
            .ok_or(crate::identity::PENDING)?;
        if !Arc::ptr_eq(&identity, &binding.root) {
            return Err(crate::identity::PENDING.into());
        }
        let bound = binding.bound.clone();
        let epoch = bound.epoch().ok_or(crate::identity::PENDING)?;
        Ok(Snapshot {
            root,
            session: self.app.session_id,
            graph: self.app.graph_runtimes.active_graph(),
            generation: self.app.source_capture_generation,
            identity,
            bound,
            epoch,
            clip,
        })
    }
    fn clip_action_current(&self, snapshot: &Snapshot) -> bool {
        self.app.session_id == snapshot.session
            && self.app.graph_runtimes.active_graph() == snapshot.graph
            && self.app.source_capture_generation == snapshot.generation
            && self.app.clip_export_target().ok() == Some(snapshot.clip)
            && self.app.clip_destination().ok().as_ref() == Some(&snapshot.root)
            && self
                .app
                .identity
                .as_ref()
                .is_some_and(|identity| Arc::ptr_eq(identity, &snapshot.identity))
            && self
                .custody_binding
                .as_ref()
                .is_some_and(|binding| Arc::ptr_eq(&binding.bound, &snapshot.bound))
            && snapshot.bound.epoch() == Some(snapshot.epoch)
    }
    pub(super) fn request_clip_action_read(&mut self, clip: eidetic::ManifestId, kind: ReadKind) {
        let result = (|| {
            let snapshot = self.clip_action_snapshot(clip)?;
            let limits = self.app.research.storage_limits()?;
            let response = self.clip_owner.read(
                snapshot.root.clone(),
                clip,
                crate::retained_clip::limits()?,
                snapshot.bound.clone(),
                limits.clone(),
                limits.budget(),
                (&limits.clip_queue).into(),
            )?;
            Ok(PendingClipAction {
                snapshot,
                kind,
                response,
            })
        })();
        match result {
            Ok(pending) => {
                self.pending_clip_actions
                    .insert(uuid::Uuid::new_v4(), pending);
            },
            Err(error) => self.app.note_retained_clip_refusal(error),
        }
    }
    pub(super) fn request_clip_export(
        &mut self,
        clip: eidetic::ManifestId,
        path: std::path::PathBuf,
        replace: bool,
    ) {
        let result = (|| {
            let snapshot = self.clip_action_snapshot(clip)?;
            let limits = self.app.research.storage_limits()?;
            let response = self.clip_owner.prepare_export(
                snapshot.root.clone(),
                path.clone(),
                replace,
                clip,
                crate::retained_clip::limits()?,
                snapshot.bound.clone(),
                snapshot.epoch,
                limits,
            )?;
            Ok(PendingExportPrepare {
                snapshot,
                path,
                replace,
                response,
            })
        })();
        match result {
            Ok(pending) => {
                self.pending_clip_export_prepares
                    .insert(uuid::Uuid::new_v4(), pending);
            },
            Err(error) => self.app.note_retained_clip_refusal(error),
        }
    }
    pub(super) fn drain_clip_actions(&mut self) {
        let ready: Vec<_> = self
            .pending_clip_actions
            .iter()
            .filter_map(|(id, pending)| match pending.response.try_recv() {
                Ok(response) => Some((*id, response.into_inner().map_err(|e| e.to_string()))),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some((*id, Err("Retained clip owner stopped".into())))
                },
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            })
            .collect();
        for (id, result) in ready {
            let Some(pending) = self.pending_clip_actions.remove(&id) else {
                continue;
            };
            if !self.clip_action_current(&pending.snapshot) {
                continue;
            }
            match (pending.kind, result) {
                (_, Err(error)) => self.app.note_retained_clip_refusal(error),
                (ReadKind::ChooseExport, Ok(_)) => {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title("Export selected clip")
                        .add_filter("Selected clip package", &["mereclip"])
                        .set_file_name("selected-clip.mereclip")
                        .save_file()
                    {
                        self.act(crate::action::Action::ExportClipPackage {
                            clip: pending.snapshot.clip,
                            path,
                            replace: true,
                        });
                    }
                },
                (ReadKind::Play, Ok(clip)) => match clip.body {
                    crate::retained_clip::ClipBody::Voice(bytes) => {
                        if let Err(error) = self
                            .app
                            .redshank
                            .play_retained_voice_bytes(&pending.snapshot.clip.0.to_hex(), bytes)
                        {
                            self.app.note_retained_clip_refusal(error);
                        }
                    },
                    _ => self
                        .app
                        .note_retained_clip_refusal("This clip has no voice body".into()),
                },
            }
        }
        self.drain_clip_exports();
    }
    fn drain_clip_exports(&mut self) {
        let ready: Vec<_> = self
            .pending_clip_export_prepares
            .iter()
            .filter_map(|(id, pending)| match pending.response.try_recv() {
                Ok(response) => Some((*id, Ok(response))),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some((*id, Err("Retained clip owner stopped".to_string())))
                },
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            })
            .collect();
        for (id, result) in ready {
            let Some(pending) = self.pending_clip_export_prepares.remove(&id) else {
                continue;
            };
            if !self.clip_action_current(&pending.snapshot) {
                continue;
            }
            let prepared = match result {
                Ok(prepared) => prepared,
                Err(error) => {
                    self.app.note_retained_clip_refusal(error);
                    continue;
                },
            };
            match prepared.value() {
                Err(error) => {
                    self.app.note_retained_clip_refusal(error.clone());
                    continue;
                },
                Ok(export)
                    if !export.matches(
                        &pending.snapshot.bound,
                        pending.snapshot.epoch,
                        pending.snapshot.clip,
                        &pending.snapshot.root,
                        &pending.path,
                        pending.replace,
                    ) =>
                {
                    self.app
                        .note_retained_clip_refusal("Prepared export changed".into());
                    continue;
                },
                Ok(_) => {},
            }
            // No event dispatch or await may occur between current validation and claim.
            let Some(claim) = pending.snapshot.bound.claim() else {
                self.app
                    .note_retained_clip_refusal(crate::identity::PENDING.into());
                continue;
            };
            if claim.epoch() != pending.snapshot.epoch {
                self.app
                    .note_retained_clip_refusal(crate::identity::PENDING.into());
                continue;
            }
            match self.clip_owner.commit_export(claim, prepared) {
                Ok(response) => {
                    self.pending_clip_export_commits.insert(
                        id,
                        PendingExportCommit {
                            clip: pending.snapshot.clip,
                            path: pending.path,
                            response,
                        },
                    );
                },
                Err(error) => self.app.note_retained_clip_refusal(error),
            }
        }
        let ready: Vec<_> = self
            .pending_clip_export_commits
            .iter()
            .filter_map(|(id, pending)| match pending.response.try_recv() {
                Ok(response) => Some((*id, response.into_inner())),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some((*id, Err("Retained clip owner stopped".into())))
                },
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            })
            .collect();
        for (id, result) in ready {
            let Some(pending) = self.pending_clip_export_commits.remove(&id) else {
                continue;
            };
            // A claimed publication reports its actual outcome after view changes.
            match result {
                Ok(()) => self
                    .app
                    .note_clip_package_exported(pending.clip, &pending.path),
                Err(error) => self.app.note_retained_clip_refusal(error),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::Duration;

    #[test]
    fn stop_cancels_queued_play_without_cancelling_other_actions() {
        let root = tempfile::tempdir().unwrap();
        let app = crate::app::App::test_stub_at(root.path().to_owned());
        let identity = app.identity.clone().unwrap();
        let bound = crate::custody_binding::BoundSealer::bind(
            identity.clone(),
            Arc::new(AtomicU64::new(1)),
        )
        .unwrap();
        let clip = eidetic::ManifestId::of_blob(b"one clip");
        let other_clip = eidetic::ManifestId::of_blob(b"another clip");
        let snapshot = Snapshot {
            root: root.path().to_owned(),
            session: app.session_id,
            graph: app.graph_runtimes.active_graph(),
            generation: app.source_capture_generation,
            identity,
            bound,
            epoch: 1,
            clip,
        };
        let worker = crate::retained_clip::owner_worker::Worker::spawn(Arc::new(|| {}));
        let limits = crate::retained_clip::owner_worker::QueueLimits {
            jobs: 6,
            bytes: 100,
        };
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let blocker = worker
            .submit(limits, 1, move |_, _| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
            .unwrap();
        entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let played = Arc::new(AtomicBool::new(false));
        let flag = played.clone();
        let play = worker
            .submit(limits, 1, move |_, _| -> ReadResult {
                flag.store(true, Ordering::SeqCst);
                Err("cancelled".into())
            })
            .unwrap();
        let export = worker
            .submit(limits, 1, |_, _| -> ReadResult { Err("export".into()) })
            .unwrap();
        let other = worker
            .submit(limits, 1, |_, _| -> ReadResult { Err("other".into()) })
            .unwrap();
        let play_id = uuid::Uuid::new_v4();
        let export_id = uuid::Uuid::new_v4();
        let other_id = uuid::Uuid::new_v4();
        let mut actions = HashMap::new();
        actions.insert(
            play_id,
            PendingClipAction {
                snapshot: snapshot.clone(),
                kind: ReadKind::Play,
                response: play,
            },
        );
        actions.insert(
            export_id,
            PendingClipAction {
                snapshot: snapshot.clone(),
                kind: ReadKind::ChooseExport,
                response: export,
            },
        );
        actions.insert(
            other_id,
            PendingClipAction {
                snapshot: Snapshot {
                    clip: other_clip,
                    ..snapshot
                },
                kind: ReadKind::Play,
                response: other,
            },
        );
        cancel_clip_play(&mut actions, clip);
        assert!(!actions.contains_key(&play_id));
        assert!(actions.contains_key(&export_id));
        assert!(actions.contains_key(&other_id));
        release_tx.send(()).unwrap();
        blocker
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .into_inner();
        actions
            .get(&other_id)
            .unwrap()
            .response
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        assert!(!played.load(Ordering::SeqCst));
        worker.close();
    }
}
