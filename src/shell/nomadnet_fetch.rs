//! Native NomadNet requests use the browser's ordinary request identities.
use std::collections::HashMap;
use std::sync::mpsc::Receiver;

use armillary::{ActorHandle, Emitter, Wake};
use fetch::{FetchCommand, FetchFailure, FetchOutcome, FetchUpdate};

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
