//! The trail-memory port: browsing capture behind an armillary actor (the
//! search surface wiring plan's W1 slice).
//!
//! Capture IS `eidetic::browsing` — the shell drains the app's semantic
//! events each frame and forwards the navigation ones here; the actor owns a
//! session-scoped `eidetic_fjall::FjallStore` at `sessions/<id>/memory`,
//! buffers traversals into per-owner `BrowsingTrace` segments through
//! [`eidetic::BrowsingMemory`], and flushes a segment when it fills and on
//! every lifecycle edge (switch, close, release). `from` chains inside the
//! actor: an owner's last destination is the next event's origin, and a
//! fresh actor starts at `None` (an origin event) rather than inventing one.
//!
//! Failures warn and drop the event rather than wedging navigation: capture
//! is an observer of browsing, never a gate on it — which is also why it
//! rides the observation drain instead of lowering an Effect. The store the
//! recall lane (W2) re-mints its `TrailIndex` from is exactly this one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{SystemTime, UNIX_EPOCH};

use armillary::{ActorHandle, Emitter, Wake, spawn_named};
use eidetic::{
    BrowsingMemory, PageRef, TraceEvent, TraceTransition, bootstrap_browsing_schema,
};
use eidetic_fjall::FjallStore;

use crate::action::Update;

/// Traversals per stored trace segment. Segments are the flush granularity:
/// small enough that a crash loses minutes, large enough that a stored trace
/// is a meaningful corridor slice.
const SEGMENT_SIZE: usize = 32;

/// Commands the trail-memory actor takes (the shell sends these; ordering on
/// the one channel is the consistency story, the bin actor's exact shape).
pub enum TrailCommand {
    /// Record one traversal for `owner` (an opaque persona tag — turnstone
    /// passes the root identity's public key hex).
    Record {
        owner: String,
        url: String,
        transition: TraceTransition,
        at_ms: u64,
    },
    /// Flush every open segment to the store (a lifecycle edge).
    Flush,
    /// Flush, re-point the store at another session's memory dir (a session
    /// switch), and restart origin chaining.
    Reopen(PathBuf),
    /// Flush, drop the open store, and ack — the close path's handshake:
    /// Windows cannot rename a directory whose files are open, so the shell
    /// releases the memory store BEFORE moving the session dir to the trash.
    Release(std::sync::mpsc::SyncSender<()>),
}

/// One session's trail-memory directory (under its `sessions/<id>/` dir).
pub fn memory_dir(session_dir: &Path) -> PathBuf {
    session_dir.join("memory")
}

/// Map a drained [`AppEvent`](crate::observe::AppEvent) onto a traversal,
/// when it is one at all. `AddressOpened` reads as a typed address: the
/// omnibar, a trail row, and a grid click all lower through
/// `Action::OpenAddress`, and the event does not carry which.
pub fn navigation(event: &crate::observe::AppEvent) -> Option<(String, TraceTransition)> {
    use crate::observe::AppEvent;
    match event {
        AppEvent::AddressOpened(url) => Some((url.clone(), TraceTransition::UrlTyped)),
        AppEvent::NavigatedBack(url) => Some((url.clone(), TraceTransition::Back)),
        AppEvent::NavigatedForward(url) => Some((url.clone(), TraceTransition::Forward)),
        AppEvent::Reloaded(url) => Some((url.clone(), TraceTransition::Reload)),
        _ => None,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Open the store, seed the schema engram, and load the stored corpus. A
/// `None` disables capture until the next `Reopen` — a browser that cannot
/// remember must still browse.
fn open_memory(dir: &Path) -> Option<(FjallStore, BrowsingMemory)> {
    if let Err(err) = std::fs::create_dir_all(dir) {
        tracing::warn!(%err, dir = %dir.display(), "trail memory: create dir failed; capture disabled until reopen");
        return None;
    }
    let mut store = match FjallStore::open(dir) {
        Ok(store) => store,
        Err(err) => {
            tracing::warn!(%err, dir = %dir.display(), "trail memory: open failed; capture disabled until reopen");
            return None;
        }
    };
    if let Err(err) = pollster::block_on(bootstrap_browsing_schema(&mut store)) {
        tracing::warn!(%err, "trail memory: schema bootstrap failed; capture disabled until reopen");
        return None;
    }
    match pollster::block_on(BrowsingMemory::load(&mut store, SEGMENT_SIZE)) {
        Ok(memory) => Some((store, memory)),
        Err(err) => {
            tracing::warn!(%err, "trail memory: corpus load failed; capture disabled until reopen");
            None
        }
    }
}

/// Flush open segments; a failure warns and keeps them buffered (they retry
/// on the next lifecycle edge).
fn flush(store: &mut FjallStore, memory: &mut BrowsingMemory) {
    if let Err(err) = pollster::block_on(memory.flush(store, now_ms())) {
        tracing::warn!(%err, "trail memory: flush failed; open segments retained");
    }
}

/// Spawn the trail-memory actor over the session memory at `dir`, waking the
/// event loop on store activity like the bin does. W1 emits no updates:
/// failures warn and capture continues; the recall pane (W2) is the first
/// reader of what lands here.
pub fn spawn_trail(wake: Wake, dir: PathBuf) -> (ActorHandle<TrailCommand>, Receiver<Update>) {
    spawn_named(
        "trail-memory",
        wake,
        move |commands, _out: Emitter<Update>| {
            let mut state = open_memory(&dir);
            // Per-owner origin chain: the last destination becomes the next
            // event's `from`.
            let mut last_to: HashMap<String, PageRef> = HashMap::new();
            while let Ok(command) = commands.recv() {
                match command {
                    TrailCommand::Record {
                        owner,
                        url,
                        transition,
                        at_ms,
                    } => {
                        let Some((store, memory)) = state.as_mut() else {
                            continue;
                        };
                        let to = PageRef { url, title: None };
                        let event = TraceEvent {
                            from: last_to.insert(owner.clone(), to.clone()),
                            to,
                            transition,
                            at_ms,
                            dwell_ms: None,
                            candidates: Vec::new(),
                        };
                        if memory.record_traversal(&owner, event) {
                            flush(store, memory);
                        }
                    }
                    TrailCommand::Flush => {
                        if let Some((store, memory)) = state.as_mut() {
                            flush(store, memory);
                        }
                    }
                    TrailCommand::Reopen(dir) => {
                        if let Some((store, memory)) = state.as_mut() {
                            flush(store, memory);
                        }
                        last_to.clear();
                        state = open_memory(&dir);
                    }
                    TrailCommand::Release(ack) => {
                        if let Some((store, memory)) = state.as_mut() {
                            flush(store, memory);
                        }
                        state = None;
                        last_to.clear();
                        let _ = ack.send(());
                    }
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir()
            .join("turnstone-trail-memory-tests")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Two records chain `from`, Flush persists, Release closes the store,
    /// and a fresh load reads one trace whose events carry the chain — the
    /// round trip W2's re-mint depends on.
    #[test]
    fn records_chain_and_round_trip() {
        let dir = temp_dir();
        let wake: Wake = Arc::new(|| {});
        let (handle, _rx) = spawn_trail(wake, dir.clone());
        handle.command(TrailCommand::Record {
            owner: "p".into(),
            url: "https://a.example/".into(),
            transition: TraceTransition::UrlTyped,
            at_ms: 1,
        });
        handle.command(TrailCommand::Record {
            owner: "p".into(),
            url: "https://b.example/".into(),
            transition: TraceTransition::Back,
            at_ms: 2,
        });
        handle.command(TrailCommand::Flush);
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(0);
        handle.command(TrailCommand::Release(ack_tx));
        ack_rx.recv().unwrap();

        let mut store = FjallStore::open(&dir).unwrap();
        let memory = pollster::block_on(BrowsingMemory::load(&mut store, SEGMENT_SIZE)).unwrap();
        let traces: Vec<_> = memory.traces().collect();
        assert_eq!(traces.len(), 1, "one flushed segment");
        let events = &traces[0].events;
        assert_eq!(events.len(), 2);
        assert!(events[0].from.is_none(), "first event is an origin");
        assert_eq!(events[0].to.url, "https://a.example/");
        assert_eq!(
            events[1].from.as_ref().map(|p| p.url.as_str()),
            Some("https://a.example/"),
            "the chain: last destination becomes the next origin"
        );
        assert_eq!(events[1].transition, TraceTransition::Back);
    }

    /// A segment that fills flushes without an explicit Flush command.
    #[test]
    fn full_segment_flushes_itself() {
        let dir = temp_dir();
        let wake: Wake = Arc::new(|| {});
        let (handle, _rx) = spawn_trail(wake, dir.clone());
        for i in 0..SEGMENT_SIZE {
            handle.command(TrailCommand::Record {
                owner: "p".into(),
                url: format!("https://page{i}.example/"),
                transition: TraceTransition::LinkClick,
                at_ms: i as u64,
            });
        }
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(0);
        handle.command(TrailCommand::Release(ack_tx));
        ack_rx.recv().unwrap();

        let mut store = FjallStore::open(&dir).unwrap();
        let memory = pollster::block_on(BrowsingMemory::load(&mut store, SEGMENT_SIZE)).unwrap();
        let total: usize = memory.traces().map(|t| t.events.len()).sum();
        assert_eq!(total, SEGMENT_SIZE);
    }

    /// The event mapping: the four navigation events map, the rest do not.
    #[test]
    fn navigation_mapping() {
        use crate::observe::AppEvent;
        assert_eq!(
            navigation(&AppEvent::AddressOpened("https://x/".into())),
            Some(("https://x/".into(), TraceTransition::UrlTyped))
        );
        assert_eq!(
            navigation(&AppEvent::NavigatedBack("https://x/".into())),
            Some(("https://x/".into(), TraceTransition::Back))
        );
        assert_eq!(
            navigation(&AppEvent::NavigatedForward("https://x/".into())),
            Some(("https://x/".into(), TraceTransition::Forward))
        );
        assert_eq!(
            navigation(&AppEvent::Reloaded("https://x/".into())),
            Some(("https://x/".into(), TraceTransition::Reload))
        );
        assert_eq!(navigation(&AppEvent::WindowOpened), None);
    }
}
