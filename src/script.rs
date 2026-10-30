// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Capability-scoped Piccolo control scripts.
//!
//! This is Turnstone's app-control lane, not a web-page JavaScript substitute.
//! A script receives a small observation snapshot and emits ordinary
//! [`crate::action::Action`] values. The caller remains responsible for
//! lowering those actions through [`crate::app::App::update`] and running the
//! returned effects through the shell's ports.

use std::cell::RefCell;
use std::rc::Rc;

use script_engine_api::{Budget, CallCx, HostData, NativeFn, PumpOutcome, ScriptEngine};
use script_engine_piccolo::{PiccoloCallCx, PiccoloEngine};
use serde_json::json;

use crate::action::Action;
use crate::app::App;
use crate::panes::{PaneKindId, kind};

const READ_APP: u8 = 1 << 0;
const DISPATCH_ACTION: u8 = 1 << 1;
const NAVIGATE: u8 = 1 << 2;
const CONTROL_PANES: u8 = 1 << 3;
const AUTHOR_CONTENT: u8 = 1 << 4;

/// The capabilities a control script may exercise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScriptCapabilities(u8);

impl ScriptCapabilities {
    pub(crate) const fn read_only() -> Self {
        Self(READ_APP)
    }

    pub(crate) const fn control() -> Self {
        Self(READ_APP | DISPATCH_ACTION | NAVIGATE | CONTROL_PANES)
    }

    /// Control plus authoring. Separate from [`control`](Self::control)
    /// because writing content is a separate grant, and the privileged
    /// omnibar lane should not acquire it by sharing a constructor.
    #[cfg(test)]
    pub(crate) const fn authoring() -> Self {
        Self(READ_APP | DISPATCH_ACTION | NAVIGATE | CONTROL_PANES | AUTHOR_CONTENT)
    }

    /// Everything but reading the app. Exists for the tests that prove a
    /// read-gated binding actually refuses.
    #[cfg(test)]
    pub(crate) const fn without_read() -> Self {
        Self(DISPATCH_ACTION | NAVIGATE | CONTROL_PANES)
    }

    const fn contains(self, capability: u8) -> bool {
        self.0 & capability != 0
    }
}

/// The read surface exposed to a control script. Keep this as data rather than
/// handing Lua an `App` reference: the script can observe the host, but it
/// cannot mutate application state except by emitting typed Actions.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AppSnapshot {
    focused_url: Option<String>,
    graph_nodes: usize,
    live_content_nodes: usize,
    focus: &'static str,
    pane_count: usize,
    isometric: bool,
    height_by_degree: bool,
}

impl AppSnapshot {
    fn from_app(app: &App) -> Self {
        Self {
            focused_url: app.graph_runtimes.focused_url().map(str::to_string),
            graph_nodes: app.graph_runtimes.graph().nodes().count(),
            live_content_nodes: app.content.live_nodes().count(),
            focus: app.focus.label(),
            pane_count: app.frisket.iter_leaves().count(),
            isometric: app.graph_runtimes.is_isometric(),
            height_by_degree: app.graph_runtimes.height_by_degree(),
        }
    }

    fn to_json(&self) -> String {
        json!({
            "focused_url": self.focused_url,
            "graph_nodes": self.graph_nodes,
            "live_content_nodes": self.live_content_nodes,
            "focus": self.focus,
            "pane_count": self.pane_count,
            "isometric": self.isometric,
            "height_by_degree": self.height_by_degree,
        })
        .to_string()
    }
}

struct ControlScriptHost {
    snapshot: AppSnapshot,
    /// What woke this run, already serialized. Built once per run rather than
    /// per call, so a body asking twice gets the same answer.
    trigger: String,
    capabilities: ScriptCapabilities,
    actions: RefCell<Vec<Action>>,
    /// A behavior's proposed scalar output for deadband comparison. It is
    /// metadata about the action batch, not permission to perform it.
    output: RefCell<Option<i64>>,
}

/// One behavior evaluation before anything is lowered into application truth.
#[derive(Debug)]
pub(crate) struct BehaviorRun {
    pub(crate) actions: Vec<Action>,
    pub(crate) output: Option<i64>,
}

type PiccoloValue = <PiccoloEngine as ScriptEngine>::Value;

fn host(cx: &PiccoloCallCx<'_>) -> Result<Rc<ControlScriptHost>, String> {
    cx.host_data()
        .and_then(|data| data.downcast::<ControlScriptHost>().ok())
        .ok_or_else(|| "Turnstone control script has no host state".to_string())
}

fn require(host: &ControlScriptHost, capability: u8, name: &str) -> Result<(), String> {
    if host.capabilities.contains(capability) {
        Ok(())
    } else {
        Err(format!("control script lacks capability: {name}"))
    }
}

struct Snapshot;

impl NativeFn<PiccoloEngine> for Snapshot {
    fn call(cx: &mut PiccoloCallCx<'_>) -> Result<PiccoloValue, String> {
        let snapshot = {
            let host = host(cx)?;
            require(&host, READ_APP, "app.read")?;
            host.snapshot.to_json()
        };
        cx.make_string(&snapshot)
    }
}

/// What woke this run: the matched entries, or an empty list when a person
/// asked for it by hand. Gated on `app.read` like the snapshot, because it
/// describes the graph.
struct Trigger;

impl NativeFn<PiccoloEngine> for Trigger {
    fn call(cx: &mut PiccoloCallCx<'_>) -> Result<PiccoloValue, String> {
        let trigger = {
            let host = host(cx)?;
            require(&host, READ_APP, "app.read")?;
            host.trigger.clone()
        };
        cx.make_string(&trigger)
    }
}

/// Declare this run's scalar output for actuation deadband comparison.
/// Exactly one value is permitted so the host never has to guess which sample
/// describes the batch it is about to lower.
struct Output;

impl NativeFn<PiccoloEngine> for Output {
    fn call(cx: &mut PiccoloCallCx<'_>) -> Result<PiccoloValue, String> {
        let value = cx.arg(0);
        let output: i64 = cx
            .value_to_string(&value)?
            .parse()
            .map_err(|_| "mere.output requires a signed integer".to_string())?;
        let host = host(cx)?;
        let mut slot = host.output.borrow_mut();
        if slot.replace(output).is_some() {
            return Err("mere.output may be called only once per run".to_string());
        }
        Ok(cx.undefined())
    }
}

/// Write a note's body: the authoring lane a summarizing behavior needs.
///
/// Gated on the `Author` ring, which install never preselects, so a helper
/// that can overwrite what you wrote has been granted that on purpose.
struct Write;

impl NativeFn<PiccoloEngine> for Write {
    fn call(cx: &mut PiccoloCallCx<'_>) -> Result<PiccoloValue, String> {
        let url_value = cx.arg(0);
        let url = cx.value_to_string(&url_value)?;
        let body_value = cx.arg(1);
        let body = cx.value_to_string(&body_value)?;
        if url.trim().is_empty() {
            return Err("mere.write requires a non-empty address".to_string());
        }
        {
            let host = host(cx)?;
            require(&host, AUTHOR_CONTENT, "content.author")?;
            host.actions
                .borrow_mut()
                .push(Action::WriteNote { url, body });
        }
        Ok(cx.undefined())
    }
}

struct Dispatch;

impl NativeFn<PiccoloEngine> for Dispatch {
    fn call(cx: &mut PiccoloCallCx<'_>) -> Result<PiccoloValue, String> {
        let value = cx.arg(0);
        let command = cx.value_to_string(&value)?;
        let action =
            action_for(&command).ok_or_else(|| format!("unknown control action: {command}"))?;
        {
            let host = host(cx)?;
            require(&host, DISPATCH_ACTION, "action.dispatch")?;
            host.actions.borrow_mut().push(action);
        }
        Ok(cx.undefined())
    }
}

struct Open;

impl NativeFn<PiccoloEngine> for Open {
    fn call(cx: &mut PiccoloCallCx<'_>) -> Result<PiccoloValue, String> {
        let value = cx.arg(0);
        let url = cx.value_to_string(&value)?;
        if url.trim().is_empty() {
            return Err("mere.open requires a non-empty address".to_string());
        }
        {
            let host = host(cx)?;
            require(&host, NAVIGATE, "navigation.open")?;
            host.actions.borrow_mut().push(Action::OpenAddress(url));
        }
        Ok(cx.undefined())
    }
}

struct Summon;

impl NativeFn<PiccoloEngine> for Summon {
    fn call(cx: &mut PiccoloCallCx<'_>) -> Result<PiccoloValue, String> {
        let value = cx.arg(0);
        let pane = cx.value_to_string(&value)?;
        let kind = pane_kind(&pane).ok_or_else(|| format!("unknown pane kind: {pane}"))?;
        {
            let host = host(cx)?;
            require(&host, CONTROL_PANES, "panes.control")?;
            host.actions.borrow_mut().push(Action::SummonPane(kind));
        }
        Ok(cx.undefined())
    }
}

fn action_for(command: &str) -> Option<Action> {
    match command.trim().to_ascii_lowercase().as_str() {
        "save_session" => Some(Action::SaveSession),
        "reseed_layout" => Some(Action::ReseedLayout),
        "fit_view" => Some(Action::FitView),
        // The analytic-layout lane (projection-engine proof 1): the runner
        // reaches the same strategy switch the palette does.
        "layout_spiral" => Some(Action::SetLayoutStrategy(Some("phyllotaxis.default"))),
        "layout_force" => Some(Action::SetLayoutStrategy(None)),
        "toggle_isometric" => Some(Action::ToggleIsometric),
        "toggle_height_by_degree" => Some(Action::ToggleHeightByDegree),
        "toggle_size_by_recency" => Some(Action::ToggleSizeByRecency),
        "toggle_physics" => Some(Action::TogglePhysics),
        "toggle_live_content" => Some(Action::ToggleNodeContent),
        "close_pane" => Some(Action::CloseActivePane),
        "maximize_pane" => Some(Action::ToggleMaximizePane),
        // The navigation lane (r3-owed row): the automation runner reaches the
        // same Back/Forward/Reload the keyboard chords and palette do.
        "back" => Some(Action::NavBack),
        "forward" => Some(Action::NavForward),
        "reload" => Some(Action::Reload),
        // The workbench lane (rung 5 slice E).
        "open_in_workbench" => Some(Action::OpenInWorkbench),
        "close_workbench_tile" => Some(Action::CloseWorkbenchTile),
        // The window lane (rung 7): a lens over the same state.
        "new_window" => Some(Action::NewWindow),
        _ => None,
    }
}

fn pane_kind(pane: &str) -> Option<PaneKindId> {
    match pane.trim().to_ascii_lowercase().as_str() {
        "roster" => Some(PaneKindId::new(kind::ROSTER)),
        "trail" => Some(PaneKindId::new(kind::TRAIL)),
        "gloss" => Some(PaneKindId::new(kind::GLOSS)),
        "inspector" => Some(PaneKindId::new(kind::INSPECTOR)),
        "comms" => Some(PaneKindId::new(kind::COMMS)),
        "overmap" => Some(PaneKindId::new(kind::OVERMAP)),
        "workbench" => Some(PaneKindId::new(kind::WORKBENCH)),
        "settings" => Some(PaneKindId::new(kind::SETTINGS)),
        "arrange" => Some(PaneKindId::new(kind::ARRANGE)),
        "publishing" => Some(PaneKindId::new(kind::PUBLISHING)),
        "shared-knot" => Some(PaneKindId::new(kind::SHARED_KNOT)),
        _ => None,
    }
}

/// Derive a run's capabilities from a participant's structural caps (participant
/// gate B2): each script capability class maps to the same servitor
/// capability [`crate::ring`] gates emissions by, and the bit is set only when
/// the provider covers it. A participant granted no rings evaluates read-less and
/// dispatch-less, and the denial surfaces in the run.
///
/// One authority, two lane faces: this is the piccolo face, `emit_allowed` is
/// the wasm one. They must ask the same questions or the "one grant" doctrine
/// is a claim rather than a property.
pub(crate) fn capabilities_from_grant(
    authority: &impl servitor::AuthorityProvider,
    subject: servitor::Subject,
) -> ScriptCapabilities {
    use crate::ring::Ring;
    use servitor::Mode;
    let mut bits = 0u8;
    let covers = |cap: &servitor::Cap, mode| authority.covers(subject, cap, mode);
    if covers(&crate::denizen::read_cap(), Mode::Read) {
        bits |= READ_APP;
    }
    for (ring, bit) in [
        (Ring::Dispatch, DISPATCH_ACTION),
        (Ring::Navigate, NAVIGATE),
        (Ring::Panes, CONTROL_PANES),
        (Ring::Author, AUTHOR_CONTENT),
    ] {
        if ring.cap().is_some_and(|cap| covers(&cap, Mode::Write)) {
            bits |= bit;
        }
    }
    ScriptCapabilities(bits)
}

/// Run a control script with full control capabilities against `app`,
/// returning the Actions it emitted (the shell lowers them through the same
/// `App::update` spine a keypress takes — the automation runner of the "one
/// description, two runners" pair). A script error surfaces as `Err`, so a
/// scenario `script` step fails loudly rather than silently emitting nothing.
pub fn run_control(app: &App, source: &str, max_steps: u64) -> Result<Vec<Action>, String> {
    run_inner(
        app,
        source,
        ScriptCapabilities::control(),
        max_steps,
        &crate::behaviors::TriggerContext::default(),
    )
    .map(|run| run.actions)
}

/// Run one capability-scoped Lua control script and return the Actions it
/// emitted. The host applies them later through the normal Action spine.
pub(crate) fn run(
    app: &App,
    source: &str,
    capabilities: ScriptCapabilities,
    max_steps: u64,
    trigger: &crate::behaviors::TriggerContext,
) -> Result<Vec<Action>, String> {
    run_inner(app, source, capabilities, max_steps, trigger).map(|run| run.actions)
}

/// Run a resident behavior and keep the scalar output beside its actions so
/// the actuation boundary can apply a declared deadband.
pub(crate) fn run_behavior(
    app: &App,
    source: &str,
    capabilities: ScriptCapabilities,
    max_steps: u64,
    trigger: &crate::behaviors::TriggerContext,
) -> Result<BehaviorRun, String> {
    run_inner(app, source, capabilities, max_steps, trigger)
}

fn run_inner(
    app: &App,
    source: &str,
    capabilities: ScriptCapabilities,
    max_steps: u64,
    trigger: &crate::behaviors::TriggerContext,
) -> Result<BehaviorRun, String> {
    if max_steps == 0 {
        return Err("control script requires a positive step budget".to_string());
    }

    let host = Rc::new(ControlScriptHost {
        snapshot: AppSnapshot::from_app(app),
        trigger: trigger.to_json(),
        capabilities,
        actions: RefCell::new(Vec::new()),
        output: RefCell::new(None),
    });
    let host_data: HostData = host.clone();
    let mut engine = PiccoloEngine::new().map_err(|err| format!("Piccolo init: {err:?}"))?;
    engine.set_host_data(host_data);
    engine
        .set_function::<Snapshot>("__mere_snapshot", 0)
        .map_err(|err| format!("install mere.snapshot: {err:?}"))?;
    engine
        .set_function::<Dispatch>("__mere_dispatch", 1)
        .map_err(|err| format!("install mere.dispatch: {err:?}"))?;
    engine
        .set_function::<Write>("__mere_write", 2)
        .map_err(|err| format!("install mere.write: {err:?}"))?;
    engine
        .set_function::<Trigger>("__mere_trigger", 0)
        .map_err(|err| format!("install mere.trigger: {err:?}"))?;
    engine
        .set_function::<Output>("__mere_output", 1)
        .map_err(|err| format!("install mere.output: {err:?}"))?;
    engine
        .set_function::<Open>("__mere_open", 1)
        .map_err(|err| format!("install mere.open: {err:?}"))?;
    engine
        .set_function::<Summon>("__mere_summon", 1)
        .map_err(|err| format!("install mere.summon: {err:?}"))?;
    engine
        .eval(
            "mere = { snapshot = __mere_snapshot, dispatch = __mere_dispatch, \
             trigger = __mere_trigger, output = __mere_output, open = __mere_open, \
             write = __mere_write, summon = __mere_summon }",
        )
        .map_err(|err| format!("install Turnstone control API: {err:?}"))?;
    engine
        .eval_bounded(source, Budget::Steps(max_steps))
        .map_err(|err| format!("control script failed: {err:?}"))?;
    if matches!(engine.pump(Budget::Steps(max_steps)), PumpOutcome::Pending) {
        return Err("control script microtask budget exhausted".to_string());
    }

    Ok(BehaviorRun {
        actions: host.actions.take(),
        output: host.output.take(),
    })
}

#[cfg(test)]
#[path = "script/tests.rs"]
mod tests;
