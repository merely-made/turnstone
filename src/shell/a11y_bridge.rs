// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The OS AccessKit bridge: app projection and retained foreign subtrees,
//! composed on the UI thread and pushed to the platform assistive stack.
//!
//! Turnstone has carried a complete in-process accessibility projection since
//! the a11y module landed — `project_app` stitches chrome, panes, and the
//! frozen projection into one `UxTree`, and the scenario lane asserts against
//! it. What was missing was the last foot: nothing handed that tree to the OS,
//! so a screen reader saw a bare window while the harness saw everything. The
//! projection grammar plan's manual screen-reader pass fails its own preflight
//! ("a11y_bridge: installed") without this file.
//!
//! The adapter's activation handler runs on whatever thread the platform
//! calls in from, so it takes the latest tree out of a shared slot rather
//! than reaching into `App`, which lives on the main thread and must stay
//! there. The shell retains a safe host-only activation snapshot in the slot;
//! activation wakes the UI thread to replay nested subtrees in parent order.
//! Foreign semantic changes publish on their own wake through
//! `update_if_active`, independently of pixel acquisition. App-only changes
//! retain their frame cadence, with immediate publication on focus/layout
//! changes. Composed receipt snapshots do not prove OS or screen-reader use.
//!
//! Actions are routed, not dropped, as of the day after the first pass: the
//! platform hands an `ActionRequest` to whatever thread it likes, so the
//! handler queues it and wakes the event loop, and the shell drains the
//! queue on the main thread, resolves each target through the route table
//! built beside the projection, and lowers it through the same update spine
//! a keypress uses. A request whose node has no route lands in the event
//! stream as `interaction-missed a11y-action`, because a miss a receipt
//! cannot see is the failure this whole lane exists to prevent.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use accesskit::{ActionRequest, TreeUpdate};
use accesskit_winit::Adapter;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

/// The latest projected tree, shared between the main thread that builds it
/// and whatever thread the platform activates from.
pub(crate) type SharedTree = Arc<Mutex<Option<TreeUpdate>>>;

struct ServeLatest {
    tree: SharedTree,
    replay: Arc<AtomicBool>,
    wake: winit::event_loop::EventLoopProxy<()>,
}

impl accesskit::ActivationHandler for ServeLatest {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.replay.store(true, Ordering::Release);
        let _ = self.wake.send_event(());
        self.tree.lock().expect("a11y tree slot poisoned").clone()
    }
}

/// Queued assistive actions, drained by the shell on the main thread.
pub(crate) type ActionQueue = Arc<Mutex<Vec<ActionRequest>>>;

struct QueueAndWake {
    queue: ActionQueue,
    wake: winit::event_loop::EventLoopProxy<()>,
}

impl accesskit::ActionHandler for QueueAndWake {
    fn do_action(&mut self, request: ActionRequest) {
        self.queue
            .lock()
            .expect("a11y action queue poisoned")
            .push(request);
        let _ = self.wake.send_event(());
    }
}

struct ReplayAfterDeactivation(Arc<AtomicBool>);

impl accesskit::DeactivationHandler for ReplayAfterDeactivation {
    fn deactivate_accessibility(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

/// Install the bridge. Must run before the window is first shown; the adapter
/// panics otherwise, which is why the shell creates its window hidden.
pub(crate) fn install(
    event_loop: &ActiveEventLoop,
    window: &Window,
    shared: SharedTree,
    queue: ActionQueue,
    wake: winit::event_loop::EventLoopProxy<()>,
    replay: Arc<AtomicBool>,
) -> Adapter {
    let adapter = Adapter::with_direct_handlers(
        event_loop,
        window,
        ServeLatest {
            tree: shared,
            replay: replay.clone(),
            wake: wake.clone(),
        },
        QueueAndWake { queue, wake },
        ReplayAfterDeactivation(replay),
    );
    tracing::info!("a11y_bridge: installed");
    adapter
}
