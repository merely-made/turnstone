// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A single custody binding for the source owner and private workshop.
//! A lock, disconnect, profile change, or host generation change permanently
//! retires this binding; a new connection must explicitly rebind it.
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};
use std::time::Duration;

pub trait BindingAuthority: eidetic::PayloadSealer + Send + Sync {
    fn custody_epoch(&self) -> Option<u64>;
    /// Fresh bounded observation of unlocked profile and exact master key.
    fn check_bound_status(&self) -> bool;
}

impl BindingAuthority for crate::identity::RootIdentity {
    fn custody_epoch(&self) -> Option<u64> {
        self.custody_epoch()
    }
    fn check_bound_status(&self) -> bool {
        self.custody_binding_current()
    }
}

/// Zero permanently means exhausted. A generation is never reused.
pub fn advance_generation(host: &AtomicU64) -> u64 {
    let mut observed = host.load(Ordering::SeqCst);
    loop {
        if observed == 0 {
            return 0;
        }
        let next = observed.checked_add(1).unwrap_or(0);
        match host.compare_exchange(observed, next, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return next,
            Err(current) => observed = current,
        }
    }
}

/// Proof that this binding was current at the local publication ordering point.
pub struct PublicationClaim {
    bound: Arc<BoundSealer>,
    epoch: u64,
}
impl PublicationClaim {
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn belongs_to(&self, bound: &Arc<BoundSealer>) -> bool {
        Arc::ptr_eq(&self.bound, bound)
    }
}

pub struct BoundSealer {
    authority: Arc<dyn BindingAuthority>,
    root_epoch: u64,
    host: Arc<AtomicU64>,
    host_epoch: u64,
    retired: AtomicBool,
}
impl BoundSealer {
    pub fn bind(
        authority: Arc<dyn BindingAuthority>,
        host: Arc<AtomicU64>,
    ) -> Result<Arc<Self>, &'static str> {
        let root_epoch = authority.custody_epoch().ok_or("custody unavailable")?;
        let host_epoch = host.load(Ordering::SeqCst);
        if host_epoch == 0
            || !authority.check_bound_status()
            || authority.custody_epoch() != Some(root_epoch)
            || host.load(Ordering::SeqCst) != host_epoch
        {
            return Err("custody binding changed");
        }
        Ok(Arc::new(Self {
            authority,
            root_epoch,
            host,
            host_epoch,
            retired: AtomicBool::new(false),
        }))
    }
    pub fn epoch(&self) -> Option<u64> {
        if self.retired.load(Ordering::SeqCst) {
            return None;
        }
        if self.host.load(Ordering::SeqCst) != self.host_epoch
            || self.authority.custody_epoch() != Some(self.root_epoch)
        {
            self.retire();
            return None;
        }
        Some(self.host_epoch)
    }
    /// Linearize an already validated local operation against retirement.
    /// After a successful claim the caller reports the actual I/O outcome;
    /// retirement cannot retract a claimed write.
    pub fn claim(self: &Arc<Self>) -> Option<PublicationClaim> {
        let epoch = self.epoch()?;
        self.claim_epoch(epoch)?;
        Some(PublicationClaim {
            bound: self.clone(),
            epoch,
        })
    }
    fn claim_epoch(&self, epoch: u64) -> Option<u64> {
        self.retired
            .compare_exchange(false, false, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| epoch)
    }
    pub fn retire(&self) {
        if !self.retired.swap(true, Ordering::SeqCst) {
            let next = self.host_epoch.checked_add(1).unwrap_or(0);
            let _ = self.host.compare_exchange(
                self.host_epoch,
                next,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
        }
    }
    fn check_status(&self) -> bool {
        self.epoch().is_some() && self.authority.check_bound_status() && self.epoch().is_some()
    }
}
impl eidetic::PayloadSealer for BoundSealer {
    fn seal(
        &self,
        hash: &eidetic::Hash,
        bytes: &[u8],
    ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
        let epoch = self
            .epoch()
            .ok_or_else(|| eidetic::Error::new("custody binding unavailable"))?;
        let result = self.authority.seal(hash, bytes).map_err(|error| {
            self.retire();
            error
        })?;
        if self.epoch() != Some(epoch) {
            return Err(eidetic::Error::new("custody binding changed"));
        }
        Ok(result)
    }
    fn unseal(
        &self,
        hash: &eidetic::Hash,
        marker: &eidetic::SealedBlobRef,
        bytes: &[u8],
    ) -> eidetic::Result<Vec<u8>> {
        let epoch = self
            .epoch()
            .ok_or_else(|| eidetic::Error::new("custody binding unavailable"))?;
        let mut clear = self
            .authority
            .unseal(hash, marker, bytes)
            .map_err(|error| {
                self.retire();
                error
            })?;
        if self.epoch() != Some(epoch) {
            clear.fill(0);
            return Err(eidetic::Error::new("custody binding changed"));
        }
        Ok(clear)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Lost,
}
pub struct Watcher {
    stop: mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Watcher {
    pub fn is_finished(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(std::thread::JoinHandle::is_finished)
    }
    pub fn request_stop(&self) {
        let _ = self.stop.send(());
    }
    pub fn start(
        bound: Arc<BoundSealer>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> std::io::Result<(Self, mpsc::Receiver<Event>)> {
        let (stop, stopping) = mpsc::channel();
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("turnstone-custody-watch".into())
            .spawn(move || {
                loop {
                    match stopping.recv_timeout(Duration::from_millis(250)) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        Err(mpsc::RecvTimeoutError::Timeout) => {},
                    }
                    let healthy = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        bound.check_status()
                    }))
                    .unwrap_or(false);
                    if !healthy {
                        bound.retire();
                        let _ = tx.send(Event::Lost);
                        wake();
                        return;
                    }
                }
            })?;
        Ok((
            Self {
                stop,
                thread: Some(thread),
            },
            rx,
        ))
    }
}
impl Drop for Watcher {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Completion of the off-UI binding operation. The owning Shell verifies that
/// this still corresponds to its current identity before adopting it.
pub enum SetupEvent {
    Ready {
        root: Arc<crate::identity::RootIdentity>,
        bound: Arc<BoundSealer>,
        watcher: Watcher,
        events: mpsc::Receiver<Event>,
    },
    Refused,
}
pub struct Setup {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Setup {
    pub fn is_finished(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(std::thread::JoinHandle::is_finished)
    }
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
    /// Obtain the current identity only on this joined background task.
    /// All connection/status attempts have finite deadlines.
    pub fn connect(
        host: Arc<AtomicU64>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> std::io::Result<(Self, mpsc::Receiver<SetupEvent>)> {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let thread = std::thread::Builder::new()
            .name("turnstone-custody-connect".into())
            .spawn(move || {
                let event = loop {
                    if cancelled.load(Ordering::SeqCst) {
                        return;
                    }
                    if host.load(Ordering::SeqCst) == 0 {
                        break SetupEvent::Refused;
                    }
                    let Some(root) = crate::identity::bind_timeout(Duration::from_secs(2)) else {
                        std::thread::sleep(Duration::from_millis(250));
                        continue;
                    };
                    if cancelled.load(Ordering::SeqCst) {
                        return;
                    }
                    let authority: Arc<dyn BindingAuthority> = root.clone();
                    match BoundSealer::bind(authority, host.clone()) {
                        Ok(bound) => {
                            break match Watcher::start(bound.clone(), wake.clone()) {
                                Ok((watcher, events)) => SetupEvent::Ready {
                                    root,
                                    bound,
                                    watcher,
                                    events,
                                },
                                Err(_) => {
                                    bound.retire();
                                    SetupEvent::Refused
                                },
                            };
                        },
                        Err(_) => std::thread::sleep(Duration::from_millis(250)),
                    }
                };
                if cancelled.load(Ordering::SeqCst) {
                    return;
                }
                let _ = tx.send(event);
                wake();
            })?;
        Ok((
            Self {
                stop,
                thread: Some(thread),
            },
            rx,
        ))
    }

    pub fn start(
        root: Arc<crate::identity::RootIdentity>,
        host: Arc<AtomicU64>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> std::io::Result<(Self, mpsc::Receiver<SetupEvent>)> {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let thread = std::thread::Builder::new()
            .name("turnstone-custody-bind".into())
            .spawn(move || {
                let authority: Arc<dyn BindingAuthority> = root.clone();
                let event = loop {
                    if cancelled.load(Ordering::SeqCst) {
                        return;
                    }
                    if host.load(Ordering::SeqCst) == 0 {
                        break SetupEvent::Refused;
                    }
                    match BoundSealer::bind(authority.clone(), host.clone()) {
                        Ok(bound) => {
                            break match Watcher::start(bound.clone(), wake.clone()) {
                                Ok((watcher, events)) => SetupEvent::Ready {
                                    root,
                                    bound,
                                    watcher,
                                    events,
                                },
                                Err(_) => {
                                    bound.retire();
                                    SetupEvent::Refused
                                },
                            };
                        },
                        Err(_) => std::thread::sleep(Duration::from_millis(250)),
                    }
                };
                if cancelled.load(Ordering::SeqCst) {
                    return;
                }
                let _ = tx.send(event);
                wake();
            })?;
        Ok((
            Self {
                stop,
                thread: Some(thread),
            },
            rx,
        ))
    }
}
impl Drop for Setup {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eidetic::PayloadSealer;
    struct Fixture {
        epoch: AtomicU64,
        status: AtomicBool,
        change_during_open: AtomicBool,
    }
    impl PayloadSealer for Fixture {
        fn seal(
            &self,
            _: &eidetic::Hash,
            clear: &[u8],
        ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
            Ok((
                clear.to_vec(),
                eidetic::SealedBlobRef {
                    epoch: eidetic::SealEpochId([1; 16]),
                    format: "fixture".into(),
                },
            ))
        }
        fn unseal(
            &self,
            _: &eidetic::Hash,
            _: &eidetic::SealedBlobRef,
            bytes: &[u8],
        ) -> eidetic::Result<Vec<u8>> {
            if self.change_during_open.load(Ordering::SeqCst) {
                self.epoch.fetch_add(1, Ordering::SeqCst);
            }
            Ok(bytes.to_vec())
        }
    }
    impl BindingAuthority for Fixture {
        fn custody_epoch(&self) -> Option<u64> {
            Some(self.epoch.load(Ordering::SeqCst))
        }
        fn check_bound_status(&self) -> bool {
            self.status.load(Ordering::SeqCst)
        }
    }
    fn fixture() -> Arc<Fixture> {
        Arc::new(Fixture {
            epoch: AtomicU64::new(1),
            status: AtomicBool::new(true),
            change_during_open: AtomicBool::new(false),
        })
    }
    #[test]
    fn lock_and_reunlock_never_revives_old_binding_or_returns_cleartext() {
        let authority = fixture();
        let host = Arc::new(AtomicU64::new(1));
        let bound = BoundSealer::bind(authority.clone(), host.clone()).unwrap();
        let hash = eidetic::Hash::of(b"secret");
        let (sealed, marker) = bound.seal(&hash, b"secret").unwrap();
        authority.change_during_open.store(true, Ordering::SeqCst);
        assert!(bound.unseal(&hash, &marker, &sealed).is_err());
        assert_eq!(bound.epoch(), None);
        assert_eq!(host.load(Ordering::SeqCst), 2);
        authority.change_during_open.store(false, Ordering::SeqCst);
        assert!(bound.unseal(&hash, &marker, &sealed).is_err());
        assert!(BoundSealer::bind(authority, host).is_ok());
    }
    #[test]
    fn status_loss_wakes_idle_owner_and_watcher_joins() {
        let authority = fixture();
        let host = Arc::new(AtomicU64::new(1));
        let bound = BoundSealer::bind(authority.clone(), host.clone()).unwrap();
        let (wake_tx, wake_rx) = mpsc::channel();
        let (watcher, events) = Watcher::start(
            bound.clone(),
            Arc::new(move || {
                let _ = wake_tx.send(());
            }),
        )
        .unwrap();
        authority.status.store(false, Ordering::SeqCst);
        assert_eq!(
            events.recv_timeout(Duration::from_secs(3)).unwrap(),
            Event::Lost
        );
        wake_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(bound.epoch(), None);
        assert_eq!(host.load(Ordering::SeqCst), 2);
        drop(watcher);
    }
    #[test]
    fn idle_watcher_drops_and_joins_without_a_loss_event() {
        let authority = fixture();
        let host = Arc::new(AtomicU64::new(1));
        let bound = BoundSealer::bind(authority, host).unwrap();
        let (watcher, _events) = Watcher::start(bound, Arc::new(|| {})).unwrap();
        let start = std::time::Instant::now();
        drop(watcher);
        assert!(start.elapsed() < Duration::from_secs(3));
    }
    #[test]
    fn claim_and_retirement_have_one_atomic_order() {
        let host = Arc::new(AtomicU64::new(1));
        let bound = BoundSealer::bind(fixture(), host.clone()).unwrap();
        let checked = bound.epoch().unwrap();
        bound.retire();
        assert_eq!(
            bound.claim_epoch(checked),
            None,
            "retirement after the epoch check defeats claim"
        );
        let fresh = BoundSealer::bind(fixture(), host).unwrap();
        let claimed = fresh.claim();
        assert!(claimed.is_some());
        fresh.retire();
        assert!(fresh.claim().is_none());
        assert!(
            claimed.is_some(),
            "later retirement does not retract the claim"
        );
    }
    #[test]
    fn exhausted_host_generation_never_reuses_a_binding() {
        let host = AtomicU64::new(u64::MAX);
        assert_eq!(advance_generation(&host), 0);
        assert_eq!(advance_generation(&host), 0);
        assert_eq!(host.load(Ordering::SeqCst), 0);
    }
}
