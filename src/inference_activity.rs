// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Process-local counts of the inference client operations this host sends.
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct Counters {
    routes: AtomicUsize,
    publications: AtomicUsize,
    generations: AtomicUsize,
}
impl Counters {
    fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "client_compiled": cfg!(feature = "inference"),
            "route_attempts": self.routes.load(Ordering::Relaxed),
            "corpus_publications": self.publications.load(Ordering::Relaxed),
            "generation_requests": self.generations.load(Ordering::Relaxed),
            // This client has no model-load request. A resident's own actions
            // are outside these process-local counters and need separate proof.
            "resident_model_loads": null,
        })
    }
}
static COUNTERS: Counters = Counters {
    routes: AtomicUsize::new(0),
    publications: AtomicUsize::new(0),
    generations: AtomicUsize::new(0),
};
#[cfg(feature = "inference")]
pub(crate) fn record_route_attempt() {
    let _ = COUNTERS
        .routes
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            Some(n.saturating_add(1))
        });
}
#[cfg(feature = "inference")]
pub(crate) fn record_publication() {
    let _ = COUNTERS
        .publications
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            Some(n.saturating_add(1))
        });
}
#[cfg(feature = "inference")]
pub(crate) fn record_generation() {
    let _ = COUNTERS
        .generations
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            Some(n.saturating_add(1))
        });
}
pub(crate) fn snapshot() -> serde_json::Value {
    COUNTERS.snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_free_activity_counts_are_separate_and_do_not_claim_resident_loads() {
        let c = Counters::default();
        c.routes.fetch_add(2, Ordering::Relaxed);
        c.publications.fetch_add(1, Ordering::Relaxed);
        let value = c.snapshot();
        assert_eq!(value["route_attempts"], 2);
        assert_eq!(value["corpus_publications"], 1);
        assert_eq!(value["generation_requests"], 0);
        assert!(value["resident_model_loads"].is_null());
    }
}
