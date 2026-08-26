//! The automation surface: what a scenario drives turnstone through.
//!
//! `Automatable` grants the shared `resolve`, `click`, and `select-text` verbs,
//! so the app implements only what it alone knows: its DOMs (via a visitor,
//! since they sit behind `RefCell`), its retained text targets, its snapshot,
//! its labelled actions, and whether it is still busy. `Driveable` adds the two
//! the generic loop cannot do: a screenshot and turnstone's own verbs.

use winit::event::MouseButton;
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};

use crate::action::Action;
use crate::panes::PaneContent;

use super::Shell;

/// turnstone drives through the shared genet-probe harness: implementing this
/// small surface grants the `resolve` / `click` verbs (used by the collapsed
/// `click_pane_*` above) for free. `with_surfaces` hands the retained pane DOMs
/// to a visitor — the borrow guards live only for the callback, which is why the
/// trait takes a visitor rather than returning a `Vec` (turnstone's DOMs are behind
/// `RefCell`). Inspector/Workbench panes join by adding their `dom_ref` here when
/// they grow click verbs.
impl genet_probe::Automatable for Shell {
    fn with_surfaces<R>(&self, f: impl FnOnce(&[genet_probe::ProbeSurface<'_>]) -> R) -> R {
        let plan = self.surface_plan();
        let mut guards: Vec<(
            &'static str,
            [f32; 4],
            std::cell::Ref<'_, genet_scripted_dom::ScriptedDom>,
        )> = Vec::new();
        let mut contributed_guards: Vec<(
            &'static str,
            [f32; 4],
            std::cell::Ref<'_, genet_scripted_dom::ScriptedDom>,
            &str,
        )> = Vec::new();
        let knot_sheet = format!(
            "{} {}",
            crate::ui::CAMBIUM_SHEET,
            crate::knot_authoring::KNOT_SHEET
        );
        for surface in &plan {
            let rect = [
                surface.rect.x,
                surface.rect.y,
                surface.rect.w,
                surface.rect.h,
            ];
            match surface.kind {
                crate::surface::SurfaceKind::Content(node) => {
                    if let Some(session) = self.content_sessions.get(&node)
                        && let Some(knot) = session
                            .as_any_ref()
                            .downcast_ref::<crate::knot_authoring::KnotDocumentSession>(
                        )
                    {
                        guards.push(("knot", rect, knot.dom_ref()));
                    }
                }
                crate::surface::SurfaceKind::Pane(id) => match self.pane_content(id) {
                    Some(PaneContent::Roster) => {
                        if let Some(g) = self.renderers.roster.get(&id) {
                            guards.push(("roster", rect, g.dom_ref()));
                        }
                    }
                    Some(PaneContent::Trail) => {
                        if let Some(pane) = self.renderers.trail.get(&id) {
                            guards.push(("trail", rect, pane.dom_ref()));
                        }
                    }
                    Some(PaneContent::Inspector) => {
                        if let Some(pane) = self.renderers.inspector.get(&id) {
                            guards.push(("inspector", rect, pane.dom_ref()));
                        }
                    }
                    Some(PaneContent::Gloss(_)) => {
                        if let Some(pane) = self.renderers.gloss.get(&id) {
                            guards.push(("gloss", rect, pane.dom_ref()));
                        }
                    }
                    Some(PaneContent::Apparatus) => {
                        if let Some(pane) = self.renderers.apparatus.get(&id) {
                            guards.push(("apparatus", rect, pane.dom_ref()));
                        }
                    }
                    Some(PaneContent::Steward) => {
                        if let Some(pane) = self.renderers.steward.get(&id) {
                            guards.push(("steward", rect, pane.dom_ref()));
                        }
                    }
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::TRANSCRIPT =>
                    {
                        if let Some(pane) = self.renderers.transcript.get(&id) {
                            guards.push(("transcript", rect, pane.dom_ref()));
                        }
                    }
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::SETTINGS =>
                    {
                        if let Some(pane) = self.renderers.settings.get(&id) {
                            guards.push(("settings", rect, pane.dom_ref()));
                        }
                    }
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::PUBLISHING =>
                    {
                        if let Some(pane) = self.renderers.publish.get(&id) {
                            guards.push(("publishing", rect, pane.dom_ref()));
                        }
                    }
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::SHARED_KNOT =>
                    {
                        if let Some(pane) = self.renderers.shared_knot.get(&id) {
                            guards.push(("shared-knot", rect, pane.dom_ref()));
                        }
                    }
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::DEVICE_RECEIPTS =>
                    {
                        if let Some(pane) = self.renderers.device_receipts.get(&id) {
                            guards.push(("device-receipts", rect, pane.dom_ref()));
                        }
                    }
                    Some(PaneContent::Registered(kind))
                        if kind.as_str() == crate::panes::kind::FROZEN_PROJECTION =>
                    {
                        if let Some(pane) = self.renderers.frozen_projection.get(&id) {
                            guards.push(("frozen-projection", rect, pane.dom_ref()));
                        }
                    }
                    Some(PaneContent::Registered(_)) => {
                        if let Some(pane) = self.renderers.contributed.get(id) {
                            contributed_guards.push((
                                "contributed",
                                rect,
                                pane.dom_ref(),
                                pane.stylesheet(),
                            ));
                        }
                    }
                    Some(PaneContent::Overmap(_)) => {
                        if let Some(pane) = self.renderers.overmap.get(&id) {
                            guards.push(("overmap", rect, pane.dom_ref()));
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        let mut surfaces: Vec<genet_probe::ProbeSurface> = guards
            .iter()
            .map(|(name, rect, r)| genet_probe::ProbeSurface {
                name,
                dom: r,
                rect: *rect,
                sheet: if *name == "knot" {
                    &knot_sheet
                } else {
                    crate::ui::CAMBIUM_SHEET
                },
            })
            .collect();
        surfaces.extend(contributed_guards.iter().map(|(name, rect, dom, sheet)| {
            genet_probe::ProbeSurface {
                name,
                dom,
                rect: *rect,
                sheet,
            }
        }));
        f(&surfaces)
    }

    fn text_target(&self, text: &str) -> Result<Option<genet_probe::TextTarget>, String> {
        let plan = self.surface_plan();
        let mut matches = plan.iter().filter_map(|surface| {
            let crate::surface::SurfaceKind::Content(node) = surface.kind else {
                return None;
            };
            self.content_sessions
                .get(&node)
                .and_then(|session| session.text_target(text))
                .map(|target| genet_probe::TextTarget {
                    anchor: (
                        surface.rect.x + target.anchor[0],
                        surface.rect.y + target.anchor[1],
                    ),
                    focus: (
                        surface.rect.x + target.focus[0],
                        surface.rect.y + target.focus[1],
                    ),
                })
        });
        let first = matches.next();
        if matches.next().is_some() {
            return Err("more than one live content surface matched".into());
        }
        Ok(first)
    }

    fn snapshot(&self) -> genet_probe::ProbeSnapshot {
        let snap = crate::observe::snapshot(&self.app);
        let kept = snap.focused.as_ref().is_some_and(|node| node.kept);
        let mut out = genet_probe::ProbeSnapshot::default()
            .with_field("focus", snap.focus)
            .with_field("node-count", snap.node_count.to_string())
            .with_field("roster-tab", snap.roster_tab)
            // The panes and surfaces as joined tags, so a generic scenario can
            // `assert snap panes ~ roster` without an app-specific verb. This is
            // minimal-shared-and-grow: the app adds the fields its scenarios name.
            .with_field("panes", snap.panes.join(","))
            .with_field("surfaces", snap.surfaces.join(","))
            .with_field("floats", snap.floating_panes.join(","))
            .with_field("lens-floats", snap.lens_floating_panes.join(","))
            // What the app will DO right now, by label — the automation half of
            // a coherent snapshot. `assert snap actions ~ Fit view` asks whether
            // a verb is on offer before spending a step on it.
            .with_field("actions", snap.available_actions.join(","))
            .with_field("kept", kept.to_string());
        if let Some(find) = snap.document_find {
            out = out
                .with_field("document-find-query", find.query)
                .with_field("document-find-count", find.count.to_string())
                .with_field(
                    "document-find-current",
                    find.current
                        .map_or_else(String::new, |index| (index + 1).to_string()),
                )
                .with_field("document-find-status", find.status);
        }
        if let Some(decision) = snap.user_agent_decision {
            out = out
                .with_field("decision-kind", decision.kind)
                .with_field("decision-node", decision.node.to_string())
                .with_field("decision-request", decision.request.to_string())
                .with_field("decision-prompt", decision.prompt)
                .with_field("decision-queued", decision.queued.to_string())
                .with_field("decision-submitting", decision.submitting.to_string())
                .with_field(
                    "decision-auth-field",
                    decision.authentication_field.unwrap_or_default(),
                )
                .with_field(
                    "decision-process-memory",
                    decision.remember_for_process.to_string(),
                );
        }
        // Fold the url in with the caption, so `assert snap focused ~ example.com`
        // can name the navigated address, not only the display caption.
        out.focused = snap.focused.map(|n| format!("{}  {}", n.caption, n.url));
        out
    }

    fn drain_events(&mut self) -> Vec<String> {
        self.observed_events.drain(..).collect()
    }

    fn act(&mut self, label: &str) -> bool {
        // Resolve against THE catalog the palette offers, in its order, so a
        // scenario acts on exactly what a person would see and pick. An exact
        // label wins anywhere before any prefix is considered; ties inside each
        // pass go to the earlier row, which is the contextual one. (This used to
        // resolve static-first while the palette showed dynamic-first — the two
        // could have disagreed about a shadowed label.) The prefix pass keeps
        // `act Switch to session` working without spelling out the whole row.
        let rows = self.app.available_actions();
        let action = rows
            .iter()
            .find(|(l, _)| l == label)
            .or_else(|| rows.iter().find(|(l, _)| l.starts_with(label)))
            .map(|(_, action)| action.clone());
        match action {
            Some(action) => {
                Shell::act(self, action);
                true
            }
            None => false,
        }
    }

    /// Turnstone's quiescence report, driving the `wait` verb. Busy while any of
    /// the three kinds of work a scenario must not race is outstanding:
    ///
    /// - a page or favicon FETCH is in flight (the port has not answered),
    /// - a content spawn is `Requested` (the effect is out, no session yet),
    /// - a live session is not `settled()` (script work or layout pending).
    ///
    /// Deliberately conservative in both directions. It reports `Some` always
    /// (turnstone DOES report quiescence, even when the honest answer is "idle"),
    /// and it counts a spawn as busy from the effect rather than from the
    /// session, so the gap between them cannot read as quiet.
    fn busy(&mut self) -> Option<bool> {
        if self.pending_fetches.any_in_flight() {
            return Some(true);
        }
        if self.app.content.any_requested() {
            return Some(true);
        }
        Some(self.content_sessions.values_mut().any(|s| !s.settled()))
    }

    fn press(&mut self, x: f32, y: f32) {
        self.deliver_press(x, y, MouseButton::Left);
    }

    fn moved(&mut self, x: f32, y: f32) {
        self.deliver_move(x, y);
    }

    fn release(&mut self, x: f32, y: f32) {
        self.deliver_release(x, y, MouseButton::Left);
    }
}

/// The `Driveable` half: the two things the shared genet-probe scenario loop
/// cannot do itself. `capture` queues a screenshot the next render fulfills (into
/// the active shared run's dir); `app_step` is left at its default (unknown verb
/// fails loudly) — turnstone's ~30 app-specific verbs are the coordinated
/// follow-on, homed here when the harness fully retires `scenario.rs`. Until
/// then the shared loop drives turnstone through its generic verbs, proving the
/// two grammars are one loop.
impl genet_probe::Driveable for Shell {
    fn capture(&mut self, name: &str) -> bool {
        self.pending_capture = Some(self.shared_out_dir.join(format!("{name}.png")));
        self.request_redraw();
        true
    }

    /// turnstone's app-specific verbs, reached when the shared grammar passes a
    /// line through. The whole vocabulary now: parse the line with turnstone's own
    /// parser and run it against the Shell via `run_scenario_step`. An unknown
    /// verb fails loudly (parse returns Err), never a silent skip.
    fn app_step(&mut self, line: &str) -> Result<(), String> {
        let step = crate::scenario::parse(line)?
            .into_iter()
            .next()
            .ok_or_else(|| format!("app_step: empty line '{line}'"))?;
        self.run_scenario_step(&step)
    }
}

impl Shell {
    /// Execute one turnstone scenario step against the Shell — the app-specific
    /// verbs the shared genet-probe loop hands to `Driveable::app_step`. This is
    /// turnstone's former `scenario.rs` `tick()` (asserts) and `scenario_pump`'s
    /// `Tick` execution (interactions), unified into one pass: an assert reads
    /// the observation snapshot and returns `Err` on mismatch; an interaction
    /// drives the Shell directly. The generic verbs (act/settle/capture/log,
    /// assert event/text/snap) never arrive — the shared loop owns them; their
    /// arms below are defensive.
    fn run_scenario_step(&mut self, step: &crate::scenario::Step) -> Result<(), String> {
        use crate::action::CaretMove;
        use crate::scenario::{CmpOp, EditKey, Step};

        fn cmp_usize(op: &CmpOp, a: usize, b: usize) -> bool {
            match op {
                CmpOp::Eq => a == b,
                CmpOp::Ge => a >= b,
                CmpOp::Le => a <= b,
            }
        }
        fn cmp_f32(op: &CmpOp, a: f32, b: f32) -> bool {
            match op {
                CmpOp::Eq => (a - b).abs() < 1e-3,
                CmpOp::Ge => a >= b,
                CmpOp::Le => a <= b,
            }
        }

        match step {
            // ---- interactions: drive the Shell (the former Tick execution) ----
            Step::Open(url) => self.act(Action::OpenAddress(url.clone())),
            Step::Omnibar { command } => self.act(Action::OmnibarOpen { command: *command }),
            Step::Type(text) => {
                if self.app.user_agent_decision.is_open() {
                    if self.app.user_agent_decision.accepts_text() {
                        self.act(Action::InsertAuthentication(text.clone()));
                    } else {
                        return Err("type: the active decision has no text field".into());
                    }
                } else if self.app.document_find.open {
                    self.act(Action::InsertDocumentFind(text.clone()));
                } else {
                    for c in text.chars() {
                        self.act(Action::OmnibarChar(c));
                    }
                }
            }
            Step::Insert(text) => {
                if self.app.user_agent_decision.is_open() {
                    if self.app.user_agent_decision.accepts_text() {
                        self.act(Action::InsertAuthentication(text.clone()));
                    } else {
                        return Err("insert: the active decision has no text field".into());
                    }
                } else if self.app.document_find.open {
                    self.act(Action::InsertDocumentFind(text.clone()));
                } else if self.app.omnibar.open {
                    self.act(Action::OmnibarInsert(text.clone()));
                } else if self.deliver_contributed_ime(&winit::event::Ime::Commit(text.clone())) {
                    self.request_redraw();
                } else if self.deliver_knot_ime(&winit::event::Ime::Commit(text.clone())) {
                    self.request_redraw();
                } else {
                    return Err("insert: no focused text editor".into());
                }
            }
            Step::Key(key) => {
                // Route through the SAME key seam winit uses, so `key` drives
                // whatever holds focus (the omnibar, a focused page, the
                // canvas) exactly as a real press would — one description, two
                // runners, for keys as well as pointers. The former direct
                // EditKey->omnibar-Action map only ever reached the omnibar.
                let (winit_key, ctrl) = match key {
                    EditKey::Enter => (WinitKey::Named(WinitNamedKey::Enter), false),
                    EditKey::Escape => (WinitKey::Named(WinitNamedKey::Escape), false),
                    EditKey::Tab => (WinitKey::Named(WinitNamedKey::Tab), false),
                    EditKey::Backspace => (WinitKey::Named(WinitNamedKey::Backspace), false),
                    EditKey::Delete => (WinitKey::Named(WinitNamedKey::Delete), false),
                    EditKey::Up => (WinitKey::Named(WinitNamedKey::ArrowUp), false),
                    EditKey::Down => (WinitKey::Named(WinitNamedKey::ArrowDown), false),
                    EditKey::Left => (WinitKey::Named(WinitNamedKey::ArrowLeft), false),
                    EditKey::Right => (WinitKey::Named(WinitNamedKey::ArrowRight), false),
                    EditKey::Home => (WinitKey::Named(WinitNamedKey::Home), false),
                    EditKey::End => (WinitKey::Named(WinitNamedKey::End), false),
                    EditKey::PageDown => (WinitKey::Named(WinitNamedKey::PageDown), false),
                    EditKey::PageUp => (WinitKey::Named(WinitNamedKey::PageUp), false),
                    EditKey::Space => (WinitKey::Named(WinitNamedKey::Space), false),
                    EditKey::Save => (WinitKey::Character("s".into()), true),
                    EditKey::Find => (WinitKey::Character("f".into()), true),
                };
                let previous_ctrl = self.ctrl;
                self.ctrl |= ctrl;
                self.on_key(&winit_key);
                self.ctrl = previous_ctrl;
            }
            Step::Script(source) => self.run_scenario_script(source),
            Step::Click(x, y) => {
                self.deliver_press(*x, *y, MouseButton::Left);
                self.deliver_release(*x, *y, MouseButton::Left);
            }
            Step::Press(x, y) => self.deliver_press(*x, *y, MouseButton::Left),
            Step::Release(x, y) => self.deliver_release(*x, *y, MouseButton::Left),
            Step::Move(x, y) => self.deliver_move(*x, *y),
            Step::Touch(id, phase, x, y, pressure) => {
                self.deliver_touch_contact(*id, *phase, *x, *y, *pressure, None)
            }
            Step::RightClick(x, y) => {
                self.deliver_press(*x, *y, MouseButton::Right);
                self.deliver_release(*x, *y, MouseButton::Right);
            }
            Step::ClickRow(substr) => self.click_pane_row(substr),
            Step::ClickTab(label) => self.click_pane_tab(label),
            Step::ClickNode(substr) => self.click_pane_node(substr),
            Step::Drag(from, to) => {
                self.deliver_press(from.0, from.1, MouseButton::Left);
                let mid = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
                self.deliver_move(mid.0, mid.1);
                self.deliver_move(to.0, to.1);
                self.deliver_release(to.0, to.1, MouseButton::Left);
            }
            Step::DragTab(from, onto, edge) => {
                self.drag_workbench_tab(from, onto, edge.as_deref());
            }
            Step::DragTabOut(from) => self.drag_workbench_tab_out(from),
            Step::HoverFile(x, y, path) => self.hover_file(*x, *y, std::path::Path::new(path)),
            Step::DropFile(x, y, path) => self.drop_file(*x, *y, std::path::Path::new(path)),
            Step::Scroll(x, y, dx, dy) => self.deliver_wheel(*x, *y, *dx, *dy),
            Step::Divider(ratio) => self.act(Action::SetActivePaneDivider(*ratio)),

            // ---- asserts: read the snapshot, Err on mismatch (former tick) ----
            Step::AssertOmnibar(open) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.omnibar.open != *open {
                    let state = if *open { "open" } else { "closed" };
                    return Err(format!("assert omnibar {state}: it is not"));
                }
            }
            Step::AssertScrolled(want_moved) => match self.content_scroll_moved {
                Some(moved) if moved == *want_moved => {}
                Some(_) => {
                    let (want, got) = if *want_moved {
                        ("moved", "did not")
                    } else {
                        ("still", "moved")
                    };
                    return Err(format!("assert scrolled {want}: the focused page {got}"));
                }
                None => {
                    return Err(
                        "assert scrolled: no content scroll key has been delivered".to_string()
                    );
                }
            },
            Step::AssertText(want) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.omnibar.text != *want {
                    return Err(format!(
                        "assert omnibar-text '{want}': the omnibar holds '{}'",
                        snap.omnibar.text
                    ));
                }
            }
            Step::AssertFocused(substr) => {
                let needle = substr.to_lowercase();
                let snap = crate::observe::snapshot(&self.app);
                let hay = snap
                    .focused
                    .map(|f| format!("{} {}", f.url, f.caption).to_lowercase())
                    .unwrap_or_default();
                if !hay.contains(&needle) {
                    return Err(format!("assert focused '{substr}': focused is '{hay}'"));
                }
            }
            Step::AssertBrowserFetch(substr) => {
                let actual = crate::observe::snapshot(&self.app)
                    .browser_fetch
                    .unwrap_or_else(|| "none".to_string());
                if !actual.contains(substr) {
                    return Err(format!(
                        "assert fetch '{substr}': focused browser fetch is '{actual}'"
                    ));
                }
            }
            Step::AssertLinkPreview(substr) => {
                let actual = crate::observe::snapshot(&self.app)
                    .link_preview
                    .unwrap_or_else(|| "none".to_string());
                if !actual.contains(substr) {
                    return Err(format!(
                        "assert link-preview '{substr}': preview is '{actual}'"
                    ));
                }
            }
            Step::AssertSurface(kind) => {
                let snap = crate::observe::snapshot(&self.app);
                if !snap.surfaces.iter().any(|s| s == kind) {
                    return Err(format!(
                        "assert surface '{kind}': the plan is {:?}",
                        snap.surfaces
                    ));
                }
            }
            Step::AssertFocus(kind) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.focus != *kind {
                    return Err(format!("assert focus '{kind}': focus is '{}'", snap.focus));
                }
            }
            Step::AssertPane(tag) => {
                let snap = crate::observe::snapshot(&self.app);
                if !snap.panes.iter().any(|p| p == tag) {
                    return Err(format!(
                        "assert pane '{tag}': the tree holds {:?}",
                        snap.panes
                    ));
                }
            }
            Step::AssertMaximized(want) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.maximized != *want {
                    let state = if *want { "maximized" } else { "not maximized" };
                    return Err(format!("assert {state}: it is not"));
                }
            }
            Step::AssertNoRow(substr) => {
                let snap = crate::observe::snapshot(&self.app);
                let hit = snap
                    .trail_rows
                    .iter()
                    .chain(snap.roster_rows.iter())
                    .chain(snap.inspector_rows.iter())
                    .find(|r| r.contains(substr));
                if let Some(row) = hit {
                    return Err(format!(
                        "assert no-row '{substr}': a row still has it: '{row}'"
                    ));
                }
            }
            Step::AssertRow(substr) => {
                let snap = crate::observe::snapshot(&self.app);
                let hit = snap
                    .trail_rows
                    .iter()
                    .chain(snap.roster_rows.iter())
                    .chain(snap.inspector_rows.iter())
                    .any(|r| r.contains(substr));
                if !hit {
                    return Err(format!(
                        "assert row '{substr}': trail {:?} roster {:?} inspector {:?}",
                        snap.trail_rows, snap.roster_rows, snap.inspector_rows
                    ));
                }
            }
            Step::AssertTab(want) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.roster_tab != want {
                    return Err(format!(
                        "assert tab '{want}': the Roster is on '{}'",
                        snap.roster_tab
                    ));
                }
            }
            Step::AssertRatio(op, want) => {
                let snap = crate::observe::snapshot(&self.app);
                let ok = snap.split_ratio.is_some_and(|r| cmp_f32(op, r, *want));
                if !ok {
                    return Err(format!(
                        "assert ratio {op:?} {want}: the root split is {:?}",
                        snap.split_ratio
                    ));
                }
            }
            Step::AssertActiveRatio(op, want) => {
                let snap = crate::observe::snapshot(&self.app);
                let ok = snap.active_ratio.is_some_and(|r| cmp_f32(op, r, *want));
                if !ok {
                    return Err(format!(
                        "assert active-ratio {op:?} {want}: the active pane's split is {:?}",
                        snap.active_ratio
                    ));
                }
            }
            Step::AssertSuggestions(op, n) => {
                let snap = crate::observe::snapshot(&self.app);
                let len = snap.omnibar.suggestions.len();
                if !cmp_usize(op, len, *n) {
                    return Err(format!(
                        "assert suggestions: have {len} ({:?}), wanted {op:?} {n}",
                        snap.omnibar.suggestions
                    ));
                }
            }
            Step::AssertVisible => {
                if !crate::observe::snapshot(&self.app).graph_visible {
                    return Err("assert visible: every node is off-screen".to_string());
                }
            }
            Step::AssertContentLive => {
                let snap = crate::observe::snapshot(&self.app);
                let focused = snap.focused.as_ref().map(|f| f.member);
                let state = focused
                    .and_then(|id| snap.content.iter().find(|(n, _)| *n == id))
                    .map(|(_, s)| s.clone());
                if state.as_deref() != Some("live") {
                    return Err(format!(
                        "assert content-live: focused node is {}",
                        state.unwrap_or_else(|| "without content state".to_string())
                    ));
                }
            }
            Step::AssertWbCells(op, n) => {
                let snap = crate::observe::snapshot(&self.app);
                if !cmp_usize(op, snap.workbench_cells.len(), *n) {
                    return Err(format!(
                        "assert wb-cells: have {} ({:?}), wanted {op:?} {n}",
                        snap.workbench_cells.len(),
                        snap.workbench_cells
                    ));
                }
            }
            Step::AssertWbCell(substr) => {
                let snap = crate::observe::snapshot(&self.app);
                if !snap.workbench_cells.iter().any(|c| c.contains(substr)) {
                    return Err(format!(
                        "assert wb-cell '{substr}': the cells are {:?}",
                        snap.workbench_cells
                    ));
                }
            }
            Step::AssertWbFraction(op, want) => {
                let snap = crate::observe::snapshot(&self.app);
                let ok = snap
                    .workbench_fractions
                    .first()
                    .is_some_and(|f| cmp_f32(op, *f, *want));
                if !ok {
                    return Err(format!(
                        "assert wb-fraction {op:?} {want}: the root fractions are {:?}",
                        snap.workbench_fractions
                    ));
                }
            }
            Step::AssertWindows(op, n) => {
                let snap = crate::observe::snapshot(&self.app);
                if !cmp_usize(op, snap.windows, *n) {
                    return Err(format!("assert windows {op:?} {n}: have {}", snap.windows));
                }
            }
            Step::AssertSessions(op, n) => {
                let snap = crate::observe::snapshot(&self.app);
                if !cmp_usize(op, snap.session_count, *n) {
                    return Err(format!(
                        "assert sessions {op:?} {n}: have {}",
                        snap.session_count
                    ));
                }
            }
            Step::AssertSession(substr) => {
                let snap = crate::observe::snapshot(&self.app);
                if !snap.session.to_lowercase().contains(&substr.to_lowercase()) {
                    return Err(format!(
                        "assert session '{substr}': the live session is '{}'",
                        snap.session
                    ));
                }
            }
            Step::AssertNodes(op, n) => {
                let snap = crate::observe::snapshot(&self.app);
                if !cmp_usize(op, snap.node_count, *n) {
                    return Err(format!("assert nodes {op:?} {n}: have {}", snap.node_count));
                }
            }
            Step::AssertA11y(substr) => {
                let (tree, _, _) = self.projected_a11y_tree();
                let lines = crate::a11y::tree_lines(&tree);
                if !lines.iter().any(|l| l.contains(substr)) {
                    return Err(format!(
                        "assert a11y '{substr}': {} lines, none match (first 12: {:?})",
                        lines.len(),
                        lines.iter().take(12).collect::<Vec<_>>()
                    ));
                }
            }

            // ---- generic verbs the shared loop owns; never reached, defensive ----
            Step::Act(label) => {
                if !genet_probe::Automatable::act(self, label) {
                    return Err(format!("act: no palette action labelled '{label}'"));
                }
            }
            Step::Settle(_) | Step::Log(_) => {}
            Step::Capture(name) => {
                self.pending_capture = Some(self.shared_out_dir.join(format!("{name}.png")));
            }
            Step::CaptureLens(name) => {
                // A lens capture lands on the LENS's own redraw, so it cannot
                // report success here. What it can do is refuse the impossible
                // case: with no lens open, the pending path would simply never
                // be consumed and the receipt would pass having written
                // nothing. That silent hole is exactly how a `capture-lens`
                // after a session switch (which closes the outgoing session's
                // windows) looked green while producing no pixels.
                if self.lens_windows.is_empty() {
                    return Err(format!(
                        "capture-lens '{name}': no lens window is open to capture"
                    ));
                }
                self.pending_lens_capture = Some(self.shared_out_dir.join(format!("{name}.png")));
                // The lens presents on its own redraw; nudge every window so
                // the pending capture lands this pump.
                self.request_redraw();
            }
            Step::AssertLensPane(substr) => {
                let snap = crate::observe::snapshot(&self.app);
                if !snap.lens_panes.iter().any(|p| p.contains(substr)) {
                    return Err(format!(
                        "assert lens-pane '{substr}': the lens spaces hold {:?}",
                        snap.lens_panes
                    ));
                }
            }
            Step::AssertNoPane(tag) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.panes.iter().any(|p| p == tag) {
                    return Err(format!(
                        "assert no-pane '{tag}': the primary tree still holds {:?}",
                        snap.panes
                    ));
                }
            }
            Step::AssertLensSurface(kind) => {
                // The first lens window's LIVE plan — the same one its render
                // and input use — so a green assert certifies what that window
                // actually composites.
                let Some((lw, lh, ordinal)) = self
                    .lens_windows
                    .values()
                    .next()
                    .map(|l| (l.width, l.height, l.ordinal))
                else {
                    return Err(format!("assert lens-surface '{kind}': no lens window"));
                };
                let plan = self.lens_plan(ordinal, lw, lh);
                if !plan.iter().any(|s| s.kind.label() == kind) {
                    let kinds: Vec<_> = plan.iter().map(|s| s.kind.label()).collect();
                    return Err(format!(
                        "assert lens-surface '{kind}': the lens plan is {kinds:?}"
                    ));
                }
            }
            Step::AssertNoLensPane(substr) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.lens_panes.iter().any(|p| p.contains(substr)) {
                    return Err(format!(
                        "assert no-lens-pane '{substr}': the lens spaces hold {:?}",
                        snap.lens_panes
                    ));
                }
            }
            Step::AssertNoSurface(kind) => {
                let snap = crate::observe::snapshot(&self.app);
                if snap.surfaces.iter().any(|s| s == kind) {
                    return Err(format!(
                        "assert no-surface '{kind}': the primary plan is {:?}",
                        snap.surfaces
                    ));
                }
            }
            Step::AssertEvent(_) => {}
        }
        self.request_redraw();
        Ok(())
    }
}
