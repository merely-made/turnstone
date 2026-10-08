// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Tasks a resident must end before it releases what they borrow.
//!
//! A door's connections and a lane's watchers are spawned, so dropping the
//! future that started them does not stop them, and whatever they hold (a
//! catalog, a store handle) outlives the run loop. A host scopes such work
//! with [`ResidentTasks::scope`]; every [`spawn_tracked`] inside it joins the
//! set, and [`ResidentTasks::cancel_and_join`] aborts the set and waits until
//! each task's future is dropped. Outside a scope `spawn_tracked` is
//! `tokio::spawn`, so untracked callers are unchanged.

use std::future::Future;
use std::sync::{Arc, Mutex};

use tokio::task::JoinSet;

tokio::task_local! {
    static CURRENT: ResidentTasks;
}

/// The set a host cancels and joins before its ordered shutdown.
#[derive(Clone, Default)]
pub struct ResidentTasks(Arc<Mutex<JoinSet<()>>>);

impl ResidentTasks {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run `future` so that every [`spawn_tracked`] under it joins this set.
    pub async fn scope<F: Future>(&self, future: F) -> F::Output {
        CURRENT.scope(self.clone(), future).await
    }

    /// Spawn into this set; the task keeps the set as its scope, so what it
    /// spawns joins too.
    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let mut set = self.0.lock().expect("task set is never poisoned");
        // Reap the finished, so a long-lived door does not accumulate them.
        while set.try_join_next().is_some() {}
        set.spawn(CURRENT.scope(self.clone(), future));
    }

    /// Tasks not yet joined.
    pub fn len(&self) -> usize {
        self.0.lock().expect("task set is never poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Abort every task and wait until each has dropped its future. Repeats
    /// until the set stays empty, in case a task spawned while being aborted.
    pub async fn cancel_and_join(&self) -> usize {
        let mut ended = 0;
        loop {
            let mut set = std::mem::take(&mut *self.0.lock().expect("task set is never poisoned"));
            if set.is_empty() {
                return ended;
            }
            set.abort_all();
            while set.join_next().await.is_some() {
                ended += 1;
            }
        }
    }
}

/// Spawn into the current [`ResidentTasks`] scope, or detached outside one.
pub fn spawn_tracked<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    match CURRENT.try_with(Clone::clone) {
        Ok(tasks) => tasks.spawn(future),
        Err(_) => {
            tokio::spawn(future);
        },
    }
}

/// [`spawn_tracked`] for a task whose result is awaited: the handle answers
/// as `tokio::spawn`'s does, and the task still joins the scope's set.
pub fn spawn_tracked_with_handle<F, T>(future: F) -> tokio::sync::oneshot::Receiver<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let (send, receive) = tokio::sync::oneshot::channel();
    spawn_tracked(async move {
        let _ = send.send(future.await);
    });
    receive
}
