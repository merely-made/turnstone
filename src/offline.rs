// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Explicit native receiver policy. Local storage and custody IPC remain
//! available; document/feed/media acquisition answers with typed refusal.

use fetch::{FetchCommand, FetchUpdate};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
    mpsc::Receiver,
};

pub fn enabled() -> bool {
    match std::env::var("TURNSTONE_OFFLINE") {
        Err(std::env::VarError::NotPresent) => false,
        Ok(value) if value == "0" => false,
        Ok(value) if value == "1" => true,
        _ => panic!("TURNSTONE_OFFLINE must be 0 or 1"),
    }
}

pub fn spawn(
    wake: armillary::Wake,
    attempts: Arc<AtomicUsize>,
) -> (armillary::ActorHandle<FetchCommand>, Receiver<FetchUpdate>) {
    armillary::spawn_named(
        "offline-fetch",
        wake,
        move |commands, out: armillary::Emitter<FetchUpdate>| {
            for command in commands {
                let reason = "Network acquisition is disabled in this receiver".to_string();
                let update = match command {
                    FetchCommand::Page { request, url, .. } => {
                        FetchUpdate::Page(fetch::FetchOutcome {
                            request,
                            url,
                            result: Err(fetch::FetchFailure::Failed(reason)),
                        })
                    },
                    FetchCommand::Subresource(url) => {
                        FetchUpdate::Subresource(fetch::SubresourceOutcome {
                            url,
                            result: Err(reason),
                        })
                    },
                    FetchCommand::Favicon { request, .. } => {
                        FetchUpdate::Favicon(fetch::FaviconOutcome {
                            request,
                            result: Err(reason),
                        })
                    },
                    FetchCommand::Submit {
                        request,
                        submission,
                    } => FetchUpdate::Submission(fetch::SubmissionOutcome {
                        request,
                        url: submission.url().into(),
                        result: Err(fetch::FetchFailure::Failed(reason)),
                    }),
                    FetchCommand::CancelPage { .. } => continue,
                };
                attempts.fetch_add(1, Ordering::Relaxed);
                out.emit(update);
            }
        },
    )
}

pub struct NoNetwork(pub Arc<AtomicUsize>);
impl NoNetwork {
    fn denied<T>(&self) -> Result<T, fetch::FetchError> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Err(fetch::FetchError::Unreachable(
            "Network acquisition is disabled in this receiver".into(),
        ))
    }
}
impl fetch::Fetch for NoNetwork {
    fn read_range(
        &self,
        _: &str,
        _: fetch::Range,
        _: Option<&str>,
    ) -> Result<fetch::RangeReply, fetch::FetchError> {
        self.denied()
    }
    fn read_all(&self, _: &str, _: Option<&str>, _: u64) -> Result<fetch::Body, fetch::FetchError> {
        self.denied()
    }
    fn read_into(
        &self,
        _: &str,
        _: &mut dyn std::io::Write,
        _: Option<u64>,
    ) -> Result<fetch::Facts, fetch::FetchError> {
        self.denied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fetch::Fetch;
    #[test]
    fn blocking_port_refuses_acquisition_and_records_attempts() {
        let count = Arc::new(AtomicUsize::new(0));
        let port = NoNetwork(count.clone());
        assert!(
            port.read_all("https://example.test/recording.wav", None, 100)
                .is_err()
        );
        assert_eq!(count.load(Ordering::Relaxed), 1);
    }
}
