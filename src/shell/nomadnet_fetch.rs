//! Native NomadNet requests use the browser's ordinary request identities.
use std::collections::HashMap;
use std::sync::mpsc::Receiver;

use armillary::{ActorHandle, Emitter, Wake};
use fetch::{FetchCommand, FetchFailure, FetchOutcome, FetchUpdate};

/// One already-confirmed native Micron form operation.  The request map stays
/// in the actor command and is never converted into a URL or history entry.
pub(super) enum MicronSubmissionCommand {
    Submit {
        request: u64,
        source: Option<uuid::Uuid>,
        submission: crate::action::MicronSubmission,
    },
    Cancel { request: u64 },
}

pub(super) fn spawn(wake: Wake) -> (ActorHandle<FetchCommand>, Receiver<FetchUpdate>) {
    armillary::spawn(wake, |commands, out: Emitter<FetchUpdate>| {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("NomadNet fetch runtime");
        let mut tasks: HashMap<u64, (String, tokio::task::JoinHandle<()>)> = HashMap::new();
        while let Ok(command) = commands.recv() {
            tasks.retain(|_, (_, task)| !task.is_finished());
            match command {
                FetchCommand::Page { request, url, .. } => {
                    let result_out = out.clone();
                    let task_url = url.clone();
                    let task = runtime.spawn(async move {
                        let result = crate::nomadnet::fetch_page(&task_url)
                            .await
                            .map(|page| fetch::Fetched {
                                content_type: page.content_type,
                                content_disposition: page.content_disposition,
                                bytes: page.bytes,
                                body: page.body,
                            })
                            .map_err(FetchFailure::Failed);
                        result_out.emit(FetchUpdate::Page(FetchOutcome {
                            request,
                            url: task_url,
                            result,
                        }));
                    });
                    if let Some((_, previous)) = tasks.insert(request, (url, task)) {
                        previous.abort();
                    }
                },
                FetchCommand::CancelPage { request } => {
                    if let Some((url, task)) = tasks.remove(&request) {
                        task.abort();
                        out.emit(FetchUpdate::Page(FetchOutcome {
                            request,
                            url,
                            result: Err(FetchFailure::Cancelled),
                        }));
                    }
                },
                _ => {},
            }
        }
    })
}

pub(super) fn spawn_submissions(
    wake: Wake,
) -> (
    ActorHandle<MicronSubmissionCommand>,
    Receiver<crate::action::Update>,
) {
    armillary::spawn(wake, |commands, out: Emitter<crate::action::Update>| {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("NomadNet submission runtime");
        let mut active: Option<(u64, tokio::task::JoinHandle<()>)> = None;
        while let Ok(command) = commands.recv() {
            match command {
                MicronSubmissionCommand::Submit { request, source, submission } => {
                    if let Some((_, task)) = active.take() {
                        task.abort();
                    }
                    let out = out.clone();
                    let task = runtime.spawn(async move {
                        let target = submission.target.clone();
                        let result = crate::nomadnet::submit_form(&target, submission.values)
                            .await
                            .map(crate::action::SmolwebSubmissionReceipt::Success);
                        out.emit(crate::action::Update::SmolwebSubmitted { request, source, target, result });
                    });
                    active = Some((request, task));
                }
                MicronSubmissionCommand::Cancel { request } => {
                    if active.as_ref().is_some_and(|(active_request, _)| *active_request == request) {
                        let (_, task) = active.take().expect("checked active submission");
                        task.abort();
                    }
                }
            }
        }
    })
}

impl super::Shell {
    pub(super) fn command_fetch(&self, command: FetchCommand) {
        match command {
            FetchCommand::Page { ref url, .. } if crate::nomadnet::parse_address(url).is_some() => {
                self.nomadnet_handle.command(command);
            },
            FetchCommand::CancelPage { request } => {
                self.nomadnet_handle
                    .command(FetchCommand::CancelPage { request });
                self.fetch_handle
                    .command(FetchCommand::CancelPage { request });
            },
            other => {
                self.fetch_handle.command(other);
            },
        }
    }
}
