// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl LivenessWatch {
    pub(super) fn start(wake: Wake) -> Self {
        let watch = Self {
            heard: Arc::new(Mutex::new(Instant::now())),
            listeners: Arc::new(Mutex::new(Vec::new())),
            stop: Arc::new(AtomicBool::new(false)),
        };
        let heard = Arc::clone(&watch.heard);
        let listeners = Arc::clone(&watch.listeners);
        let stop = Arc::clone(&watch.stop);
        let budget = visit_liveness_budget();
        std::thread::Builder::new()
            .name("knot-visit-liveness".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(LIVENESS_TICK);
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    let silent = heard.lock().map(|last| last.elapsed()).unwrap_or_default();
                    if silent < budget {
                        continue;
                    }
                    let sent = listeners
                        .lock()
                        .map(|listeners| {
                            let mut sent = 0usize;
                            for events in listeners.iter() {
                                if events
                                    .send(HubEvent::Unavailable(format!(
                                        "the holder has not answered for {} s",
                                        silent.as_secs()
                                    )))
                                    .is_ok()
                                {
                                    sent += 1;
                                }
                            }
                            sent
                        })
                        .unwrap_or(0);
                    // Nobody is reading this document yet, so there is nothing
                    // to tell; keep watching rather than declaring to no one.
                    if sent == 0 {
                        continue;
                    }
                    tracing::info!(
                        silent_ms = silent.as_millis() as u64,
                        "a visited document's holder went silent; reporting unavailable"
                    );
                    wake();
                    return;
                }
            })
            .expect("spawn the visited-document liveness watch");
        watch
    }

    /// The hub completed a round trip, or is about to start one.
    pub(super) fn heard(&self) {
        if let Ok(mut last) = self.heard.lock() {
            *last = Instant::now();
        }
    }

    pub(super) fn listen(&self, events: Sender<HubEvent>) {
        if let Ok(mut listeners) = self.listeners.lock() {
            listeners.push(events);
        }
    }
}
