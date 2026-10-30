// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The self-drive scenario lane: the app drives ITSELF from a scenario file,
//! so headed receipts need no OS synthetic input — no focus race, no key
//! theft when a human is using the machine (the SendKeys lane's unfixable
//! failure mode). This is the vocabulary's first automation consumer (the
//! architecture plan's recorded trigger): every step lowers to an ordinary
//! [`Action`] through the same `update` spine as a keypress, so the runner
//! needs no second execution model.
//!
//! Activation: set `TURNSTONE_SCENARIO` to a scenario file path before launch;
//! captures land in `TURNSTONE_CAPTURE_DIR` (default: beside the scenario).
//! The run ends by writing `scenario.done` there — first line `RESULT ok` or
//! `RESULT fail`, then the log lines — and exiting WITHOUT saving the
//! session, so a scenario never mutates the profile it ran against and
//! reruns stay deterministic.
//!
//! Grammar (one verb per line, `#` comments):
//!
//! ```text
//! settle [frames]           # pump N frames (default 20)
//! capture <name>            # self-capture the composed frame -> <name>.png
//! open <url>                # Action::OpenAddress (use mere:// for offline)
//! import-clip <path>        # host admission of a selected Fleece package
//! export-clip <path>        # export the current clip to a new package file
//! omnibar find|actions      # Action::OmnibarOpen
//! type <text>               # Action::OmnibarChar per char
//! insert <text>             # IME commit into the focused omnibar or Knot editor
//! key enter|escape|tab|backspace|delete|up|down|left|right|home|end|ctrl+s
//! key ctrl+a|shift+right    # native editor selection through the same key seam
//! act <palette label>       # commit a palette_actions() entry by label
//! select-graph-member <url>|none # Canvas multi-selection in the owner pane
//! activate-graph-fold <index> # press/release a real native fold badge
//! record-graph-folds <name> <count> # read native/source/saved-state checkpoint
//! record-graph-scene <name> # read owning graph scene, geometry and saved facets
//! script <inline lua>        # run a Piccolo control script; its Actions lower
//!                            # through the same spine (the automation runner —
//!                            # needs the `piccolo` feature)
//! click-at <x> <y>          # pointer click at window px (content links, canvas)
//! press-at <x> <y>          # mouse down, retained until release-at
//! release-at <x> <y>        # mouse up after intervening frames/moves
//! move-at <x> <y>           # pointer hover at window px (cursor/hover receipts)
//! touch-at <id> down|move|up|cancel <x> <y> [pressure]
//!                           # one direct contact through Pointer Events
//! right-click-at <x> <y>    # right-click: opens the command palette, selecting
//!                           # the node under the pointer (the context menu)
//! click-row <substr>       # click the list-pane row whose text contains substr
//! scroll <x> <y> <dy>       # wheel at window px (page scroll / canvas pan)
//! scroll-reader <role> <dy> # wheel at the live Reader inset|workbench|tile rect
//! record-reader-appearances <name> # stable sorted Reader appearance observation
//! record-idle <name>        # bounded global-idle diagnosis after a wait
//! divider <ratio>           # set the active pane's split ratio (0.0-1.0)
//! assert pane <tag>         # a pane with that PaneContent tag is in the tree
//! assert maximized | not-maximized
//! drop-file <x> <y> <path>  # drop a file at window px (image on node = sprite;
//!                           # else it becomes a file:// node)
//! hover-file <x> <y> <path> # begin/continue the OS file-drag lifecycle
//! drag-tab <a> onto <b>     # drag workbench tab <a> onto tab <b>'s cell (stack)
//! drag-tab <a> onto <b> @ <edge>  # ...releasing on the cell's left|right|top|bottom
//!                           # edge band (split beside instead of stack)
//! drag-tab <a> out          # ...releasing OUTSIDE the workbench (on the canvas):
//!                           # the tile tears out into a lens window (branch arm)
//! assert row <substr>       # a Trail/Roster/Inspector row's text contains substr
//! assert no-row <substr>    # NO list-pane row contains substr
//! assert wb-cells ==|>=|<= <n>  # the workbench has n cells
//! assert wb-cell <substr>   # a workbench cell's tab string contains substr
//! assert wb-fraction ==|>=|<= <f>  # the workbench root split's FIRST fraction
//! assert a11y <substr>      # an a11y-projection line ("role: label") contains substr
//! assert lens-pane <substr> # a lens window's "ordinal:tag" pane contains substr
//! assert lens-surface <kind> # the FIRST lens window's live plan composites that kind
//! assert no-pane <tag>      # NO pane with that tag is in the PRIMARY tree
//! assert no-lens-pane <substr> # NO lens "ordinal:tag" pane contains substr
//! assert no-surface <kind>  # NO surface of that kind in the PRIMARY plan
//! assert reader-appearances ==|>=|<= <n> # live Reader presentations in the plan
//! assert reader-sources ==|>=|<= <n> # distinct Reader source documents
//! assert reader-role <role> # exactly one live Reader role
//! assert reader-viewport <role> # session viewport matches its live rect
//! assert reader-scroll <role> ==|>=|<= <px> # role's own retained scroll offset
//! assert content-sessions-idle # every live document session has settled
//! assert content-ready # fetches, content requests, and sessions are quiet
//! wait-row <frames> <substr> # like `assert row`, re-checked once per pumped
//!                            # frame for up to <frames> frames; passes the
//!                            # moment it matches, fails with `assert row`'s
//!                            # diagnostics if the budget runs out
//! wait-status <frames> <substr> # over the place status lines, without blocking
//!                            # native frames or cross-process event delivery
//! wait-file <frames> <path>  # like wait-row, satisfied once <path> exists
//!                            # and is non-empty, without blocking rendered frames
//!                            # (cross-process sequencing
//!                            # through an exchange directory)
//! wait-knot <frames> <substr> # like wait-row, over this session's
//!                            # `crate::observe::KnotDocumentFacts` (from
//!                            # `App::knot_documents()`), each entry read as
//!                            # "<status> <address>" (e.g. "visiting
//!                            # knot://<root>/field.knot")
//! record-place <name>        # write crate::observe::place_record(..), pretty
//!                            # printed, to <name>.json in the capture dir
//! record-source-captures <name> # write current-session capture request facts
//!                            # (owner node, URL, phase and exact stored refs)
//! wait-source-capture <frames> <ordinal> <phase> # wait for that capture phase
//! wait-saved-sources <frames> # wait for this session's source admission read
//! wait-saved-reader <frames> # wait for the focused saved source's live reader
//! record-saved-sources <name> # write source admission, entries and live readers
//! record-structured-research <name> # read only the current admitted result
//! choose-suggestion <label> # unique visible row; ordinary arrow and enter keys
//! queue-act <palette label> # retain the exact offered Action through later
//!                            # scenario steps; no execution until deliver-act
//! deliver-act               # lower the retained Action through Shell::act
//! touch <path>               # write a one-line marker file at <path> (any
//!                            # absolute path, not just the capture dir) —
//!                            # a cross-process signal a scenario can wait on
//!                            # with wait-file from another process
//! assert active-ratio ==|>=|<= <f> # the ACTIVE pane's parent-split ratio (any space)
//! assert sessions ==|>=|<= <n>  # the manifest set holds n sessions
//! assert session <substr>   # the live session's label contains substr
//! assert nodes ==|>=|<= <n> # the graph holds n nodes
//! capture-lens <name>       # self-capture the first lens window's frame
//! assert omnibar open|closed
//! assert omnibar-text <str>  # the omnibar text is exactly <str>
//! assert focused <substr>   # focused node's url/caption contains substr
//! assert surface <kind>     # a surface of that kind (canvas|content|chrome) is composited
//! assert focus <kind>       # semantic input goes to that surface kind
//! assert suggestions ==|>=|<= <n>
//! assert visible            # at least one node inside the viewport
//! assert event <substr>     # a semantic event matching <substr> was emitted
//! log <text>
//! ```
//!
//! Asserts read the observation surface ([`crate::observe`]) — the same
//! snapshot/event pair the a11y and automation lanes consume — so a green
//! scenario certifies the surface those lanes will stand on.
//!
//! Every line is expanded for `${NAME}` before parsing: each occurrence is
//! replaced with the process environment variable of that name (a missing
//! variable is a parse error, named by line). This lets one scenario file
//! reference a path chosen by its driver (an exchange directory, say)
//! without hardcoding it — see `wait-file` above and `scenarios/place_*.scn`.

/// One parsed scenario step. `TURNSTONE_SCENARIO` runs on the shared
/// `taproot::Scenario` loop; the generic verbs (act/settle/capture/log,
/// assert text/event/snap) it owns, and these app-specific steps reach the Shell
/// via `Driveable::app_step`, parsed here and run by `Shell::run_scenario_step`.
#[derive(Debug)]
pub enum Step {
    Open(String),
    ImportClip(String),
    ExportClip(String),
    Omnibar {
        command: bool,
    },
    Type(String),
    Insert(String),
    Key(EditKey),
    KeyWindow(usize, EditKey),
    Act(String),
    /// A Piccolo control script whose emitted Actions lower through the spine
    /// (the "one description, two runners" automation lane). Piccolo-gated.
    Script(String),
    /// A pointer press+release at window pixel coordinates (rung 5 slice B).
    /// Drives the same surface-routed path winit does; a click on content
    /// resolves links, a click on the canvas is a canvas gesture.
    Click(f32, f32),
    /// Click admitted document content in one focused native window. Header,
    /// graph and pane hits are refused rather than borrowing another window.
    ClickContentWindow(usize, f32, f32),
    Press(f32, f32),
    Release(f32, f32),
    /// A pointer move at window pixel coordinates. This is separately
    /// driveable so cursor and hover callbacks can be received before a click.
    Move(f32, f32),
    /// One direct touch contact. The caller's u64 id is translated to a live,
    /// collision-free Pointer Events id by the same seam winit drives.
    Touch(u64, inker::PointerPhase, f32, f32, Option<f32>),
    /// A right-click at window pixel coordinates: opens the command palette,
    /// selecting the graph node under the pointer first (the context menu).
    RightClick(f32, f32),
    /// A wheel event at window pixel coordinates: content scrolls, canvas pans.
    Scroll(f32, f32, f32, f32),
    /// A wheel event at the centre of a named, live Reader appearance. The
    /// shell resolves the rect from its current plan, so the receipt names a
    /// presentation role rather than relying on a fixed window coordinate.
    ScrollReader(String, f32),
    /// Write the live Reader appearance registry/plan intersection as a
    /// scenario receipt, sorted by stable surface id.
    RecordReaderAppearances(String),
    /// Write the named reasons that currently make Turnstone's global `wait`
    /// report busy. This is a read-only receipt, not another waiting policy.
    RecordIdle(String),
    /// Toggle the named current source member using Canvas multi-selection.
    SelectGraphMember(String),
    /// Click the one painted note member through the normal pointer route.
    ClickNoteMember,
    /// Read graph truth, native fold routes and the saved session sidecar.
    RecordGraphFolds(String, usize),
    /// Read the owning graph scene, geometry, source truth and saved facets.
    RecordGraphScene(String),
    /// Activate a real native fold badge by index through pointer input.
    ActivateGraphFold(usize),
    /// Resolve an actual painted node, then deliver two ordinary pointer clicks.
    DoubleClickGraphNode(String),
    /// Click the list-pane (Trail/Roster) row whose text contains this substring.
    /// The shell resolves the row's window position (it owns the pane rects and
    /// rows), so the receipt names a row by text, not pixels.
    ClickRow(String),
    /// Click the Roster's tab by label; the shell asks the strip where it is.
    ClickTab(String),
    /// Click the Gloss minimap's node by label/url substring.
    ClickNode(String),
    /// Click a visible link from the actual retained reader geometry.
    ClickReadingReference(String),
    /// Press at the first point, move through it, release at the second.
    Drag((f32, f32), (f32, f32)),
    /// Drag the workbench tab labelled like the first substring onto the cell
    /// of the tab labelled like the second (the stack gesture, by name). The
    /// optional edge releases on that band of the cell body (split beside).
    DragTab(String, String, Option<String>),
    /// Drag workbench tab <a> and release OUTSIDE the workbench pane (on the
    /// canvas) — the tile tear-out gesture (the trichotomy's branch arm).
    DragTabOut(String),
    /// Drop the file at window `(x, y)` — the same handler winit's
    /// `DroppedFile` drives.
    DropFile(f32, f32, String),
    /// Hover the file at window `(x, y)` — the same handler winit's
    /// `HoveredFile` drives, allowing Chromium to answer `dragover` before a
    /// later `DroppedFile` turn.
    HoverFile(f32, f32, String),
    /// The workbench has this many cells.
    AssertWbCells(CmpOp, usize),
    /// A workbench cell's tab string contains this substring.
    AssertWbCell(String),
    /// The workbench root split's FIRST fraction compares as given.
    AssertWbFraction(CmpOp, f32),
    /// An a11y-projection line contains this substring.
    AssertA11y(String),
    /// The window count (primary + lenses) compares as given.
    AssertWindows(CmpOp, usize),
    /// A lens window holds a pane whose "ordinal:tag" contains the substring.
    AssertLensPane(String),
    /// The FIRST lens window's live surface plan composites a surface of the
    /// named kind (the tiles-follow-the-pane receipt).
    AssertLensSurface(String),
    /// NO pane with the given tag is in the PRIMARY tree (the tear-out's
    /// departure half).
    AssertNoPane(String),
    /// NO surface of the named kind is in the PRIMARY plan (the one-session-
    /// one-surface rule's cross-window half).
    AssertNoSurface(String),
    AssertReaderAppearances(CmpOp, usize),
    AssertReaderSources(CmpOp, usize),
    AssertReaderRole(String),
    AssertReaderViewport(String),
    AssertReaderScroll(String, CmpOp, f32),
    /// The document-session portion of quiescence. Graph layout may still be
    /// moving, so this deliberately does not claim whole-app idle.
    AssertContentSessionsIdle,
    AssertContentReady,
    /// NO lens pane's "ordinal:tag" contains the substring (the lens close
    /// op's departure half).
    AssertNoLensPane(String),
    /// The ACTIVE pane's parent-split ratio compares as given, in whichever
    /// space holds the pane — the divider op's readback, primary or lens.
    AssertActiveRatio(CmpOp, f32),
    /// The manifest set's session count compares as given.
    AssertSessions(CmpOp, usize),
    /// The live session's label contains this substring.
    AssertSession(String),
    /// The graph's node count compares as given (a switch shows a different
    /// graph; this is the cheap witness).
    AssertNodes(CmpOp, usize),
    /// `assert energy <op> <n>` / `assert overlaps <op> <n>`: the layout's
    /// kinetic energy and its overlapping pairs. (Physics catalog — P4.)
    AssertEnergy(CmpOp, f32),
    /// `assert physics running|paused`.
    AssertPhysicsRunning(bool),
    AssertOverlaps(CmpOp, usize),
    /// Self-capture the first live lens window's composed frame.
    CaptureLens(String),
    /// The root split's ratio compares as given.
    AssertRatio(CmpOp, f32),
    Settle(u32),
    Capture(String),
    CapturesDiffer(String, String),
    ScrollPaneWindow(u64, f32, f32, f32, f32),
    AssertOmnibar(bool),
    /// Whether the last content scroll key moved the focused page (`moved`) or
    /// was an honest no-op at the end (`still`).
    AssertScrolled(bool),
    AssertText(String),
    AssertFocused(String),
    /// The focused node's browser fetch phase contains this receipt substring.
    AssertBrowserFetch(String),
    /// The prospective-node link preview contains this receipt substring.
    AssertLinkPreview(String),
    /// A named surface kind ("canvas" / "content" / "chrome" / "pane") is in the plan.
    AssertSurface(String),
    /// The focus target is a named surface kind.
    AssertFocus(String),
    /// A pane with the given `PaneContent` tag is in the frisket tree.
    AssertPane(String),
    /// Whether a pane is maximized.
    AssertMaximized(bool),
    /// A list/detail pane row's text contains this substring.
    AssertRow(String),
    /// NO list-pane row contains this substring.
    AssertNoRow(String),
    AssertTab(String),
    /// Set the active pane's divider ratio (drag the seam).
    Divider(f32),
    AssertSuggestions(CmpOp, usize),
    AssertVisible,
    /// The focused node's content lifecycle is Live (the phase-4 receipt's
    /// app-truth half; the capture is the pixel half).
    AssertContentLive,
    /// A semantic event whose description contains the substring was emitted
    /// at some point this run.
    AssertEvent(String),
    /// Like [`Step::AssertRow`], but re-checked once per pumped frame for up
    /// to this many frames: passes the moment a row matches, otherwise fails
    /// with the same diagnostics once the budget runs out.
    WaitRow(u32, String),
    /// Like [`Step::WaitRow`], over `crate::observe::snapshot(..).place_status`.
    WaitStatus(u32, String),
    /// Like [`Step::WaitRow`], satisfied once the named path exists and is
    /// non-empty — cross-process sequencing through a shared exchange dir.
    WaitFile(u32, String),
    /// Like [`Step::WaitRow`], over this session's
    /// `crate::observe::KnotDocumentFacts` entries, each formatted as
    /// "<status> <address>".
    WaitKnot(u32, String),
    /// Wait until an observed source request reaches the named phase.
    WaitSourceCapture(u32, u64, String),
    /// Wait until this session's saved-source admission load has completed.
    WaitSavedSources(u32),
    /// Wait for the current admitted research query, not only its source owner.
    WaitResearchSearch(u32),
    /// Wait for an offered capture action bound to this exact current URL.
    WaitSourceTarget(u32, String),
    /// Wait for a current saved structured result or historical replay.
    WaitStructuredResult(u32),
    /// Wait until the focused saved-source reader has content and a live engine.
    WaitSavedReader(u32),
    /// Write the place observation facts (`crate::observe::place_record`),
    /// pretty-printed, to `<name>.json` in the capture directory.
    RecordPlace(String),
    /// Write the current session's correlated source-capture facts from the
    /// same observation snapshot used by the visible Inspector rows.
    RecordSourceCaptures(String),
    /// Write current-session saved-source admissions, entries, and reader state.
    RecordSavedSources(String),
    /// Observe the admitted current structured result without evaluating it.
    RecordStructuredResearch(String),
    /// Record visible negative/scope status without disclosing an old result.
    RecordStructuredStatus(String),
    /// Record bounded process counters and nonsecret custody principal.
    RecordRuntime(String),
    /// Record the current admitted explicit comparison without source bodies.
    RecordComparison(String),
    /// Select a unique visible omnibar row through ordinary key events.
    ChooseSuggestion(String),
    /// Refuse a named action if it is in the ordinary current action catalog.
    AssertNoAction(String),
    /// Select the visible exact manifest from an earlier capture observation.
    ChooseCapturedSource {
        name: String,
        ordinal: u64,
    },
    /// Select an exactly saved result recorded in this session.
    ChooseSavedResult(String),
    /// Snapshot an exact catalog Action for a later stale-admission check.
    QueueAct(String),
    /// Deliver the previously queued catalog Action through the ordinary
    /// shell action spine after the intervening scenario steps.
    DeliverAct,
    /// Write a one-line marker file at an arbitrary path — the cross-process
    /// signal a scenario in another process waits on with `wait-file`.
    TouchFile(String),
    Log(String),
}

/// Extract a saved result identity without accepting a stale or foreign observation.
pub(crate) fn observed_saved_result(
    value: &serde_json::Value,
    session: &str,
) -> Result<String, String> {
    if value["schema"] != "turnstone.structured-observation/v1"
        || value["session"].as_str() != Some(session)
        || value["historical"] != false
        || value["busy"] != false
    {
        return Err("saved result observation is not current for this session".into());
    }
    let saved = value["saved_manifest"]
        .as_str()
        .ok_or("saved result missing")?;
    if saved.is_empty() || value["content_manifest"].as_str() != Some(saved) {
        return Err("saved result identity mismatch".into());
    }
    Ok(saved.to_string())
}

/// Extract one saved capture identity from a current-session observation.
pub(crate) fn observed_capture_manifest(
    value: &serde_json::Value,
    session: &str,
    ordinal: u64,
) -> Result<String, String> {
    let values = value
        .as_array()
        .ok_or("capture observation is not an array")?;
    let matches: Vec<_> = values
        .iter()
        .filter(|item| {
            item["session"].as_str() == Some(session)
                && item["ordinal"].as_u64() == Some(ordinal)
                && item["phase"] == "saved"
        })
        .collect();
    let [item] = matches.as_slice() else {
        return Err("capture observation absent or ambiguous".into());
    };
    let manifest = item["stored"]["annotation_manifest"]
        .as_str()
        .ok_or("capture manifest missing")?;
    if manifest.is_empty() {
        return Err("empty capture manifest".into());
    }
    Ok(manifest.to_string())
}

/// Find one row by exact displayed label, falling back to a unique substring.
/// The caller must still navigate to the row through the ordinary input seam.
pub(crate) fn suggestion_target(labels: &[String], label: &str) -> Result<usize, String> {
    let exact: Vec<_> = labels
        .iter()
        .enumerate()
        .filter(|(_, value)| value.as_str() == label || value.strip_prefix("› ") == Some(label))
        .map(|(index, _)| index)
        .collect();
    let matches = if exact.is_empty() {
        labels
            .iter()
            .enumerate()
            .filter(|(_, value)| value.contains(label))
            .map(|(index, _)| index)
            .collect::<Vec<_>>()
    } else {
        exact
    };
    match matches.as_slice() {
        [target] => Ok(*target),
        _ => Err(format!("suggestion is absent or ambiguous: {label}")),
    }
}

#[derive(Debug)]
pub enum EditKey {
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    ShiftRight,
    ShiftLeft,
    ShiftEnter,
    SelectAll,
    Home,
    End,
    PageDown,
    PageUp,
    Space,
    Save,
    Find,
    /// Ctrl+D: keep or drop the highlighted command in the `>` lane.
    KeepCommand,
    FrameFocus,
    FrameClose,
    BackTab,
}

#[derive(Debug)]
pub enum CmpOp {
    Eq,
    Ge,
    Le,
}

#[cfg(test)]
#[path = "scenario/tests.rs"]
mod tests;

#[path = "scenario/parse_xy.rs"]
mod parse_xy;
use parse_xy::parse_xy;
#[path = "scenario/expand_env.rs"]
mod expand_env;
pub(crate) use expand_env::expand_env;
#[path = "scenario/parse.rs"]
mod parse;
pub use parse::parse;

/// Observe the ordinary offered capture action; never synthesize or invoke it.
pub(crate) fn source_target_ready(actions: &[(String, crate::action::Action)], url: &str) -> bool {
    actions.iter().filter(|(label, action)| label == "Save source and keep"
        && matches!(action, crate::action::Action::CaptureSourceDocument { target } if target.url == url))
        .count() == 1
}

/// Diagnose the ordinary command target without exposing a different target's URL.
pub(crate) fn source_target_wait_diagnostic(app: &crate::app::App, url: &str) -> String {
    match app.browser_command_target() {
        None => "no browser command target".into(),
        Some((_, current)) if current != url => "browser command target differs".into(),
        Some((node, _)) => match app.source_capture_target(node) {
            Err(reason) => format!("source admission refused: {reason}"),
            Ok(_) => "admitted source has no unique capture action".into(),
        },
    }
}
