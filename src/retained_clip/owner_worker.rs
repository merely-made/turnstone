// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Bounded admission and a single physical retained-clip owner.
//! The standard unbounded sender does not wait for the consumer; atomic
//! reservations bound every accepted job and its payload until delivery.

use super::super::research_storage::{SourceReadError, StorageLimits};
#[cfg(test)]
use std::time::Duration;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
};

const CLOSED: usize = 1usize << (usize::BITS - 1);
const ACTIVE_MASK: usize = CLOSED - 1;
struct Task {
    run: Box<dyn FnOnce(&mut Owner) + Send>,
    must_complete: bool,
}
enum Message {
    Run(Task),
    Wake,
}
struct Shared {
    sender: mpsc::Sender<Message>,
    admissions: AtomicUsize,
    outstanding: AtomicUsize,
    bytes: AtomicUsize,
    wake: armillary::Wake,
}
/// An admission begun before close must finish sending before the worker exits.
struct Admission {
    shared: Arc<Shared>,
}
impl Drop for Admission {
    fn drop(&mut self) {
        let previous = self.shared.admissions.fetch_sub(1, Ordering::SeqCst);
        if previous & CLOSED != 0 && previous & ACTIVE_MASK == 1 {
            let _ = self.shared.sender.send(Message::Wake);
        }
    }
}
impl Shared {
    fn begin(self: &Arc<Self>) -> Result<Admission, &'static str> {
        self.admissions
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |state| {
                if state & CLOSED != 0 || state & ACTIVE_MASK == ACTIVE_MASK {
                    None
                } else {
                    Some(state + 1)
                }
            })
            .map_err(|_| "Retained clip worker is stopped")?;
        Ok(Admission {
            shared: self.clone(),
        })
    }
    fn close(&self) {
        let previous = self.admissions.fetch_or(CLOSED, Ordering::SeqCst);
        if previous & CLOSED == 0 {
            let _ = self.sender.send(Message::Wake);
        }
    }
    fn closed(&self) -> bool {
        self.admissions.load(Ordering::SeqCst) & CLOSED != 0
    }
}
/// Charge the job until its delivered response is consumed or discarded.
struct Reservation {
    shared: Arc<Shared>,
    bytes: usize,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.shared.outstanding.fetch_sub(1, Ordering::SeqCst);
        self.shared.bytes.fetch_sub(self.bytes, Ordering::SeqCst);
    }
}
pub(crate) struct Pending<T> {
    receiver: mpsc::Receiver<Response<T>>,
    cancelled: Arc<AtomicBool>,
}
impl<T> Pending<T> {
    pub(crate) fn try_recv(&self) -> Result<Response<T>, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
    pub(crate) fn recv(&self) -> Result<Response<T>, mpsc::RecvError> {
        self.receiver.recv()
    }
    #[cfg(test)]
    pub(crate) fn recv_timeout(
        &self,
        duration: Duration,
    ) -> Result<Response<T>, mpsc::RecvTimeoutError> {
        self.receiver.recv_timeout(duration)
    }
}
impl<T> Drop for Pending<T> {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
}
pub(crate) struct Response<T> {
    value: T,
    _reservation: Reservation,
}
impl<T> Response<T> {
    pub(crate) fn value(&self) -> &T {
        &self.value
    }
    pub(crate) fn into_inner(self) -> T {
        self.value
    }
}
#[derive(Clone, Copy)]
pub(crate) struct QueueLimits {
    pub(crate) jobs: usize,
    pub(crate) bytes: usize,
}
impl From<&crate::research_storage::ClipQueueLimits> for QueueLimits {
    fn from(value: &crate::research_storage::ClipQueueLimits) -> Self {
        Self {
            jobs: value.max_jobs,
            bytes: value.max_bytes,
        }
    }
}
#[derive(Clone)]
pub(crate) struct Worker {
    shared: Arc<Shared>,
}
impl Worker {
    pub(crate) fn spawn(wake: armillary::Wake) -> Self {
        let (sender, receiver) = mpsc::channel();
        let shared = Arc::new(Shared {
            sender,
            admissions: AtomicUsize::new(0),
            outstanding: AtomicUsize::new(0),
            bytes: AtomicUsize::new(0),
            wake,
        });
        let thread_shared = shared.clone();
        std::thread::Builder::new()
            .name("retained-clip-owner".into())
            .spawn(move || {
                let mut owner = Owner::default();
                let mut failed = false;
                loop {
                    let state = thread_shared.admissions.load(Ordering::SeqCst);
                    let finished = state & CLOSED != 0 && state & ACTIVE_MASK == 0;
                    let message = if finished {
                        receiver.try_recv().ok()
                    } else {
                        receiver.recv().ok()
                    };
                    let Some(message) = message else {
                        break;
                    };
                    let Message::Run(task) = message else {
                        continue;
                    };
                    if failed || (thread_shared.closed() && !task.must_complete) {
                        drop(task);
                        continue;
                    }
                    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        (task.run)(&mut owner)
                    }))
                    .is_err()
                    {
                        failed = true;
                        thread_shared.close();
                        (thread_shared.wake)();
                    }
                }
            })
            .expect("spawn retained-clip owner");
        Self { shared }
    }
    fn reserve(&self, limits: QueueLimits, bytes: usize) -> Result<Reservation, &'static str> {
        self.shared
            .outstanding
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                n.checked_add(1).filter(|next| *next <= limits.jobs)
            })
            .map_err(|_| "Retained clip queue limit exceeded")?;
        if self
            .shared
            .bytes
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                n.checked_add(bytes).filter(|next| *next <= limits.bytes)
            })
            .is_err()
        {
            self.shared.outstanding.fetch_sub(1, Ordering::SeqCst);
            return Err("Retained clip queue limit exceeded");
        }
        Ok(Reservation {
            shared: self.shared.clone(),
            bytes,
        })
    }
    fn send_reserved<T: Send + 'static>(
        &self,
        reservation: Reservation,
        must_complete: bool,
        task: impl FnOnce(&mut Owner, &AtomicBool) -> T + Send + 'static,
    ) -> Result<Pending<T>, &'static str> {
        let _admission = self.shared.begin()?;
        let (tx, rx) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let task_cancelled = cancelled.clone();
        let wake = self.shared.wake.clone();
        let task = Task {
            must_complete,
            run: Box::new(move |owner| {
                if !must_complete && task_cancelled.load(Ordering::SeqCst) {
                    return;
                }
                let value = task(owner, &task_cancelled);
                let _ = tx.try_send(Response {
                    value,
                    _reservation: reservation,
                });
                wake();
            }),
        };
        self.shared
            .sender
            .send(Message::Run(task))
            .map_err(|_| "Retained clip worker is stopped")?;
        Ok(Pending {
            receiver: rx,
            cancelled,
        })
    }
    /// Reserve named payload bytes through delivery; no allocator/RSS claim.
    pub(crate) fn submit<T: Send + 'static>(
        &self,
        limits: QueueLimits,
        bytes: usize,
        task: impl FnOnce(&mut Owner, &AtomicBool) -> T + Send + 'static,
    ) -> Result<Pending<T>, &'static str> {
        let reservation = self.reserve(limits, bytes)?;
        self.send_reserved(reservation, false, task)
    }
    fn submit_reserved<T: Send + 'static>(
        &self,
        reservation: Reservation,
        task: impl FnOnce(&mut Owner, &AtomicBool) -> T + Send + 'static,
    ) -> Result<Pending<T>, &'static str> {
        self.send_reserved(reservation, true, task)
    }
    pub(crate) fn close(&self) {
        self.shared.close();
    }
}

#[derive(Default)]
pub(crate) struct Owner {
    current: Option<(
        PathBuf,
        (u64, u64),
        muniment::FjallOwnerLimits,
        muniment::FjallStore,
    )>,
}
impl Owner {
    pub(crate) fn store(
        &mut self,
        root: &Path,
        create: bool,
        limits: &StorageLimits,
    ) -> Result<&muniment::FjallStore, SourceReadError> {
        let resolved = (|| -> Result<PathBuf, SourceReadError> {
            let root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
            let path = root.join(super::STORE_DIR);
            if !create && !path.is_dir() {
                return Err("This mere has no retained clip store".into());
            }
            if create {
                std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            }
            Ok(std::fs::canonicalize(path).map_err(|e| e.to_string())?)
        })();
        let path = match resolved {
            Ok(path) => path,
            Err(error) => {
                self.current = None;
                return Err(error);
            },
        };
        // Where stable directory identity is unavailable, retire the previous
        // owner before the next operation rather than reuse its handle.
        #[cfg(not(unix))]
        {
            self.current = None;
        }
        let identity = match directory_identity(&path) {
            Ok(identity) => identity,
            Err(error) => {
                self.current = None;
                return Err(error.into());
            },
        };
        if self
            .current
            .as_ref()
            .is_some_and(|(held, held_identity, _, _)| *held == path && *held_identity != identity)
        {
            self.current = None;
            return Err("Retained clip directory changed while in use".into());
        }
        let policy = limits.owner();
        if self
            .current
            .as_ref()
            .is_none_or(|(held, held_identity, held_policy, _)| {
                *held != path || *held_identity != identity || *held_policy != policy
            })
        {
            // Dropping the old owner before opening is required by Fjall's exclusive lock.
            self.current = None;
            let store = muniment::FjallStore::open_with_limits(&path, policy.clone())
                .map_err(|error| SourceReadError::open_retry("Retained clip", error))?;
            self.current = Some((path, identity, policy, store));
        }
        Ok(&self.current.as_ref().unwrap().3)
    }
}

#[cfg(unix)]
fn directory_identity(path: &Path) -> Result<(u64, u64), String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_dir() {
        return Err("Retained clip path is not a directory".into());
    }
    Ok((metadata.dev(), metadata.ino()))
}
#[cfg(not(unix))]
fn directory_identity(path: &Path) -> Result<(u64, u64), String> {
    if !std::fs::metadata(path).map_err(|e| e.to_string())?.is_dir() {
        return Err("Retained clip path is not a directory".into());
    }
    // The prior owner is retired before every call on these platforms.
    Ok((0, 0))
}

#[path = "owner_ops.rs"]
mod ops;
pub(crate) use ops::{OwnedNative, PreparedExport};
#[cfg(test)]
#[path = "owner_worker_tests.rs"]
mod tests;
