// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use std::time::Duration;

#[test]
fn panic_disconnects_accepted_jobs_and_refuses_new_ones() {
    let worker = Worker::spawn(Arc::new(|| {}));
    let bounds = QueueLimits {
        jobs: 3,
        bytes: 100,
    };
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let first = worker
        .submit(bounds, 10, move |_, _| -> () {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            panic!("deliberate worker failure");
        })
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let second = worker.submit(bounds, 10, |_, _| 42).unwrap();
    release_tx.send(()).unwrap();
    assert!(matches!(
        first.recv_timeout(Duration::from_secs(10)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
    assert!(matches!(
        second.recv_timeout(Duration::from_secs(10)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
    assert!(worker.submit(bounds, 1, |_, _| ()).is_err());
}

#[test]
fn undrained_completion_keeps_reservation_without_blocking_another_job() {
    let worker = Worker::spawn(Arc::new(|| {}));
    let bounds = QueueLimits { jobs: 2, bytes: 10 };
    let (sent_tx, sent_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let first = worker
        .submit(bounds, 5, move |_, _| {
            sent_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            1
        })
        .unwrap();
    sent_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let second = worker.submit(bounds, 5, |_, _| 2).unwrap();
    release_tx.send(()).unwrap();
    assert_eq!(
        second
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .into_inner(),
        2
    );
    let limited = QueueLimits { jobs: 1, bytes: 4 };
    assert!(worker.submit(limited, 1, |_, _| 3).is_err());
    drop(first);
    let next = worker.submit(bounds, 5, |_, _| 3).unwrap();
    assert_eq!(
        next.recv_timeout(Duration::from_secs(10))
            .unwrap()
            .into_inner(),
        3
    );
    worker.close();
}

#[test]
fn policy_change_and_missing_root_retire_previous_owner() {
    let first = tempfile::tempdir().unwrap();
    let missing = tempfile::tempdir().unwrap();
    let mut owner = Owner::default();
    let limits = StorageLimits::default();
    let store = owner.store(first.path(), true, &limits).unwrap();
    pollster::block_on(muniment::Backend::put(store, "oversized", &vec![42; 8192])).unwrap();
    let mut restrictive = limits.clone();
    restrictive.recovery.max_value_bytes = 4;
    assert!(owner.store(first.path(), false, &restrictive).is_err());
    let path = first.path().join(super::super::STORE_DIR);
    let reopened = muniment::FjallStore::open_with_limits(&path, limits.owner()).unwrap();
    drop(reopened);
    owner.store(first.path(), false, &limits).unwrap();
    assert!(owner.store(missing.path(), false, &limits).is_err());
    let reopened = muniment::FjallStore::open_with_limits(&path, limits.owner()).unwrap();
    drop(reopened);
}

#[test]
fn dropped_queued_request_never_executes() {
    let worker = Worker::spawn(Arc::new(|| {}));
    let bounds = QueueLimits {
        jobs: 3,
        bytes: 100,
    };
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let running = worker
        .submit(bounds, 10, move |_, _| {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        })
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let executed = Arc::new(AtomicBool::new(false));
    let flag = executed.clone();
    let cancelled = worker
        .submit(bounds, 10, move |_, _| {
            flag.store(true, Ordering::SeqCst);
        })
        .unwrap();
    drop(cancelled);
    let later = worker.submit(bounds, 10, |_, _| 7).unwrap();
    release_tx.send(()).unwrap();
    running
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .into_inner();
    assert_eq!(
        later
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .into_inner(),
        7
    );
    assert!(!executed.load(Ordering::SeqCst));
    worker.close();
}

#[test]
fn close_waits_for_preclose_admission_before_draining_claimed_work() {
    let worker = Worker::spawn(Arc::new(|| {}));
    let admission = worker.shared.begin().unwrap();
    worker.close();
    let (sent_tx, sent_rx) = mpsc::channel();
    worker
        .shared
        .sender
        .send(Message::Run(Task {
            must_complete: true,
            run: Box::new(move |_| {
                sent_tx.send(()).unwrap();
            }),
        }))
        .unwrap();
    drop(admission);
    sent_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(
        worker
            .submit(QueueLimits { jobs: 1, bytes: 1 }, 1, |_, _| ())
            .is_err()
    );
}

#[test]
fn admission_enforces_count_bytes_and_overflow() {
    let worker = Worker::spawn(Arc::new(|| {}));
    let bounds = QueueLimits { jobs: 3, bytes: 12 };
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let held = worker
        .submit(bounds, 4, move |_, _| {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        })
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let overflow = QueueLimits {
        jobs: 4,
        bytes: usize::MAX,
    };
    assert!(worker.submit(overflow, usize::MAX, |_, _| ()).is_err());
    let a = worker.submit(bounds, 4, |_, _| 1).unwrap();
    let b = worker.submit(bounds, 4, |_, _| 2).unwrap();
    assert!(worker.submit(bounds, 1, |_, _| 3).is_err());
    release_tx.send(()).unwrap();
    held.recv_timeout(Duration::from_secs(10))
        .unwrap()
        .into_inner();
    assert_eq!(
        a.recv_timeout(Duration::from_secs(10))
            .unwrap()
            .into_inner(),
        1
    );
    assert_eq!(
        b.recv_timeout(Duration::from_secs(10))
            .unwrap()
            .into_inner(),
        2
    );
    worker.close();
}

#[test]
fn concurrent_producers_do_not_overbook_reservations() {
    let worker = Worker::spawn(Arc::new(|| {}));
    let bounds = QueueLimits { jobs: 3, bytes: 12 };
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let blocker = worker
        .submit(bounds, 4, move |_, _| {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        })
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(9));
    let threads: Vec<_> = (0..8)
        .map(|n| {
            let worker = worker.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                worker.submit(bounds, 4, move |_, _| n)
            })
        })
        .collect();
    barrier.wait();
    let accepted: Vec<_> = threads
        .into_iter()
        .filter_map(|thread| thread.join().unwrap().ok())
        .collect();
    assert_eq!(accepted.len(), 2);
    release_tx.send(()).unwrap();
    blocker
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .into_inner();
    for pending in accepted {
        pending
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .into_inner();
    }
    worker.close();
}
