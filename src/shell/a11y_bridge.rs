//! The OS AccessKit bridge: the same tree `assert a11y` reads, pushed to the
//! platform assistive stack.
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
//! there. The shell refreshes the slot on a frame cadence and nudges the
//! adapter with `update_if_active`, so the OS view lags the app by at most
//! the cadence, and a screen reader that never activates costs one mutex
//! store per refresh and nothing else.
//!
//! Actions are deliberately not routed yet: a screen-reader `Click` arriving
//! as an `ActionRequest` is dropped here. Routing it into the app's action
//! spine is the pass's follow-up work, and pretending otherwise would make
//! the first pass assert a path that does not exist.

use std::sync::{Arc, Mutex};

use accesskit::{ActionRequest, TreeUpdate};
use accesskit_winit::Adapter;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

/// The latest projected tree, shared between the main thread that builds it
/// and whatever thread the platform activates from.
pub(crate) type SharedTree = Arc<Mutex<Option<TreeUpdate>>>;

struct ServeLatest(SharedTree);

impl accesskit::ActivationHandler for ServeLatest {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.0.lock().expect("a11y tree slot poisoned").clone()
    }
}

struct DropActions;

impl accesskit::ActionHandler for DropActions {
    fn do_action(&mut self, _request: ActionRequest) {}
}

struct NoDeactivation;

impl accesskit::DeactivationHandler for NoDeactivation {
    fn deactivate_accessibility(&mut self) {}
}

/// Install the bridge. Must run before the window is first shown; the adapter
/// panics otherwise, which is why the shell creates its window hidden.
pub(crate) fn install(
    event_loop: &ActiveEventLoop,
    window: &Window,
    shared: SharedTree,
) -> Adapter {
    let adapter = Adapter::with_direct_handlers(
        event_loop,
        window,
        ServeLatest(shared),
        DropActions,
        NoDeactivation,
    );
    tracing::info!("a11y_bridge: installed");
    adapter
}
