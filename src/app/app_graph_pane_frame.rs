// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    /// One frame of a graph pane whose surface is `width` x `height` physical
    /// pixels: the scene is at logical size, for rasterization at
    /// [`pane_layout_scale`](Self::pane_layout_scale).
    pub fn graph_pane_frame(
        &mut self,
        pane: PaneId,
        width: u32,
        height: u32,
    ) -> Option<(netrender::Scene, bool)> {
        let scale = self.pane_layout_scale(pane);
        let (width, height) = (
            (width as f32 / scale).round().max(1.0) as u32,
            (height as f32 / scale).round().max(1.0) as u32,
        );
        let theme = self.map_theme();
        let graph = self.graph_for_pane(pane)?;
        let mut transition = self.graph_runtimes.get_mut(graph)?.scene_transition.take();
        let restored =
            std::mem::take(&mut self.graph_runtimes.get_mut(graph)?.scene_restore_pending);
        let fit = self.graph_views.take_fit(pane);
        static CLOCK: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        let host_ms = CLOCK
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_secs_f64()
            * 1000.0;
        let result = self.with_graph_pane(pane, |canvas| {
            if let Some(theme) = &theme {
                canvas.apply_theme(theme);
            }
            if (!restored || !Self::prime_restored_layout(canvas, width, height))
                && Self::drive_layout_strategy_for(canvas, width, height)
            {
                // A resized/recomputed scene supersedes any older return.
                transition = None;
                canvas.take_stop_return();
            }
            let id = canvas.layout_strategy().unwrap_or("free").to_owned();
            mere::canvas::physics_controls::advance_arrangement(
                canvas,
                &mut transition,
                &id,
                host_ms,
            );
            if fit {
                canvas.resize(width, height);
                canvas.fit_visible();
            }
            let (scene, redraw) = canvas.frame(width, height);
            (
                scene,
                redraw || transition.is_some() || canvas.has_stop_return(),
            )
        });
        self.graph_runtimes.get_mut(graph)?.scene_transition = transition;
        result
    }

    pub fn graph_pane_focused_member(&self, pane: PaneId) -> Option<uuid::Uuid> {
        self.graph_views.focused_member(pane).or_else(|| {
            self.graph_for_pane(pane)
                .and_then(|graph| self.graph_runtimes.canvas(graph))
                .and_then(mere::canvas::Canvas::focused_member)
        })
    }

    pub fn focused_graph_pane(&self) -> Option<PaneId> {
        match self.focus {
            FocusTarget::Graph(pane) => Some(pane),
            FocusTarget::Chrome | FocusTarget::Content { .. } | FocusTarget::Pane(_) => None,
        }
    }

    /// The Forme owner for a Workbench action. A focused Workbench pane wins;
    /// otherwise the focused graph pane supplies its identity forme.
    pub fn workbench_owner_pane(&self) -> Option<PaneId> {
        self.active_pane
            .filter(|pane| self.pane_content(*pane) == Some(&PaneContent::Workbench))
            .or_else(|| self.focused_graph_pane())
    }

    pub fn graph_pane_node_at_screen(
        &mut self,
        pane: PaneId,
        x: f32,
        y: f32,
    ) -> Option<uuid::Uuid> {
        let (x, y) = self.logical(pane, x, y);
        self.with_graph_pane(pane, |canvas| canvas.node_at_screen(x, y))
            .flatten()
    }

    pub fn graph_pane_select_member(&mut self, pane: PaneId, member: uuid::Uuid) -> bool {
        self.with_graph_pane(pane, |canvas| canvas.select_member(member))
            .unwrap_or(false)
    }

    pub fn graph_pane_cursor_moved(&mut self, pane: PaneId, x: f32, y: f32) -> bool {
        let (x, y) = self.logical(pane, x, y);
        self.reading
            .scope
            .activation
            .entry(pane)
            .or_default()
            .moved((x, y));
        self.with_graph_pane(pane, |canvas| canvas.cursor_moved(x, y))
            .unwrap_or(false)
    }

    pub fn graph_pane_wheel(&mut self, pane: PaneId, dx: f32, dy: f32) -> bool {
        let (dx, dy) = self.logical(pane, dx, dy);
        self.with_graph_pane(pane, |canvas| canvas.wheel(dx, dy))
            .unwrap_or(false)
    }

    pub fn graph_pane_pointer_down(
        &mut self,
        pane: PaneId,
        button: mere::canvas::PointerButton,
        x: f32,
        y: f32,
    ) -> bool {
        let (x, y) = self.logical(pane, x, y);
        let member = self
            .with_graph_pane(pane, |canvas| canvas.node_at_screen(x, y))
            .flatten()
            .and_then(|member| {
                self.graph_for_pane(pane)
                    .and_then(|graph| self.graph_runtimes.get(graph))
                    .map(|runtime| (runtime.instance, member))
            });
        self.reading
            .scope
            .activation
            .entry(pane)
            .or_default()
            .press(member, (x, y), button != mere::canvas::PointerButton::Left);
        if let Some(graph) = self.graph_for_pane(pane) {
            self.graph_runtimes.get_mut(graph).unwrap().scene_transition = None;
        }
        self.with_graph_pane(pane, |canvas| {
            canvas.take_stop_return();
            canvas.pointer_down(button, x, y)
        })
        .unwrap_or(false)
    }

    /// The right click the pane's canvas did not use, if any, for the host's
    /// context menu (Scenograph editor plan SE26).
    pub fn graph_pane_take_context_request(
        &mut self,
        pane: PaneId,
    ) -> Option<mere::canvas::ContextRequest> {
        self.with_graph_pane(pane, |canvas| canvas.take_context_request())
            .flatten()
    }

    pub fn graph_pane_pointer_up(
        &mut self,
        pane: PaneId,
        button: mere::canvas::PointerButton,
        x: f32,
        y: f32,
    ) -> bool {
        let (x, y) = self.logical(pane, x, y);
        let handled = self
            .with_graph_pane(pane, |canvas| canvas.pointer_up(button, x, y))
            .unwrap_or(false);
        let member = self
            .with_graph_pane(pane, |canvas| canvas.node_at_screen(x, y))
            .flatten()
            .and_then(|member| {
                self.graph_for_pane(pane)
                    .and_then(|graph| self.graph_runtimes.get(graph))
                    .map(|runtime| (runtime.instance, member))
            });
        static CLOCK: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        let now = CLOCK
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_secs_f64()
            * 1000.0;
        if button == mere::canvas::PointerButton::Left {
            if let Some(member) = self
                .reading
                .scope
                .activation
                .entry(pane)
                .or_default()
                .release(member, (x, y), now)
            {
                self.reading.scope.read_request = self
                    .graph_for_pane(pane)
                    .map(|graph| (graph, member.0, member.1));
            }
        } else {
            self.reading
                .scope
                .activation
                .entry(pane)
                .or_default()
                .cancel();
        }
        handled
    }

    /// Record that a place refused something, in the one event stream a
    /// scenario reads. The shell calls this for a refused visit, which never
    /// reaches a surface and so cannot report itself.
    pub fn note_place_refusal(&mut self, reason: String) {
        self.events.push(AppEvent::PlaceRefused(reason));
    }

    pub(crate) fn note_retained_clip_refusal(&mut self, reason: String) {
        self.events.push(AppEvent::AffordanceUnavailable {
            what: "retained-clip",
            target: reason,
        });
    }

    pub(crate) fn note_clip_package_exported(
        &mut self,
        clip: eidetic::ManifestId,
        path: &std::path::Path,
    ) {
        self.events.push(AppEvent::ClipPackageExported {
            clip: crate::retained_clip::address(clip),
            path: path.display().to_string(),
        });
    }

    /// What this session has open as Knot documents, for observation.
    pub fn knot_documents(&self) -> &[crate::observe::KnotDocumentFacts] {
        &self.knot_documents
    }

    /// Replace the mirror. The shell calls this after every update drain with
    /// the facts its own live surfaces report, plus whatever was refused.
    pub fn set_knot_documents(&mut self, facts: Vec<crate::observe::KnotDocumentFacts>) {
        self.knot_documents = facts;
    }

    /// Drain the semantic events emitted since the last call (the shell
    /// hands them to the scenario's log, diagnostics, or drops them).
    pub fn take_events(&mut self) -> Vec<AppEvent> {
        // The behavior drain reads this queue without consuming it (the shell
        // owns the drain), so its read cursor is an index into the vec and has
        // to go back to zero when the vec does. The ORDINAL must not: a watch
        // cursor is monotonic, so if event numbering restarted at every shell
        // drain, every later event would look older than what the watch had
        // already seen and nothing would ever wake again. `events_base` is the
        // running total that keeps the two in step.
        self.events_base += self.events.len() as u64;
        self.events_seen = 0;
        std::mem::take(&mut self.events)
    }

    pub fn physics_refusal(&self) -> Option<&mere::canvas::OverlayRefusal> {
        self.physics_refusal.as_ref().and_then(|(graph, refusal)| {
            (*graph == self.graph_runtimes.active_graph()).then_some(refusal)
        })
    }

    pub(super) fn record_physics_result(
        &mut self,
        result: Result<(), mere::canvas::OverlayRefusal>,
    ) {
        self.physics_refusal = match result {
            Ok(()) => None,
            Err(refusal) => {
                self.events.push(AppEvent::PhysicsRefused(refusal.clone()));
                Some((self.graph_runtimes.active_graph(), refusal))
            },
        };
    }

    /// The events the behavior drain has not considered yet.
    pub(crate) fn unseen_events(&self) -> &[AppEvent] {
        let seen = self.events_seen.min(self.events.len());
        &self.events[seen..]
    }

    /// Mark everything currently queued as considered.
    pub(crate) fn mark_events_seen(&mut self) {
        self.events_seen = self.events.len();
    }

    /// How many events are queued, for attributing the ones a body produces.
    pub(crate) fn events_len(&self) -> usize {
        self.events.len()
    }

    /// The ordinal of the queue's first entry: events already taken by the
    /// shell. Added to an index to get a number that only ever grows.
    pub(crate) fn events_base(&self) -> u64 {
        self.events_base
    }

    /// Seed a new lens window's pane space: a lone Orrery leaf with a freshly
    /// minted pane id (globally unique across every window's tree, so surface
    /// keys and the active-pane anchor never collide). Returns its ordinal.
    pub(super) fn seed_lens_space(&mut self) -> usize {
        let pane_id = PaneId(self.next_pane_id);
        self.next_pane_id += 1;
        let ordinal = self.lenses.len();
        self.lenses.push(Some(FrisketLayout {
            id: crate::panes::FrisketId::new(format!("lens-{ordinal}")),
            label: format!("lens {ordinal}"),
            root: PaneNode::Leaf {
                pane_id,
                content: PaneContent::Orrery,
                graph_id: self.graph_runtimes.active_graph(),
            },
        }));
        self.lens_blueprints.push(None);
        ordinal
    }

    /// A legacy station authority still awaiting supported Workspace import.
    pub(crate) fn blueprint_space(&self, space: SpaceRef) -> Option<&SpaceBlueprint> {
        match space {
            SpaceRef::Primary => self.primary_blueprint.as_ref(),
            SpaceRef::Lens(ordinal) => self.lens_blueprints.get(ordinal).and_then(Option::as_ref),
        }
    }

    pub(super) fn take_blueprint_space(&mut self, space: SpaceRef) -> Option<SpaceBlueprint> {
        match space {
            SpaceRef::Primary => self.primary_blueprint.take(),
            SpaceRef::Lens(ordinal) => self.lens_blueprints.get_mut(ordinal).and_then(Option::take),
        }
    }

    /// A pointer press promotes the clicked float above its siblings. Tiled
    /// panes deliberately do not instantiate a blueprint by being clicked.
    pub(crate) fn raise_floating_pane(&mut self, pane: PaneId) -> bool {
        let Some(space) = self.space_of(pane) else {
            return false;
        };
        if self.ensure_shared_space(space).is_err() {
            return false;
        }
        self.space_mut(space)
            .and_then(|layout| layout.workspace_mut())
            .is_some_and(|frame| {
                frame
                    .apply(&workbench::FloatEvent::Raise(workbench::TileId(pane.0)).into())
                    .is_ok_and(|outcome| outcome.changed())
            })
    }

    /// Choose the newest live lens that is not `exclude`, spawning a lens when
    /// none qualifies. The ordinal is returned separately so A5 can prepare
    /// the destination's blueprint before a legacy leaf is moved there.
    pub(super) fn target_lens(&mut self, exclude: Option<SpaceRef>) -> (usize, Vec<Effect>) {
        let mut effects = Vec::new();
        let target = self
            .lenses
            .iter()
            .enumerate()
            .rev()
            .find(|(i, s)| s.is_some() && exclude != Some(SpaceRef::Lens(*i)))
            .map(|(i, _)| i);
        let ordinal = match target {
            Some(ordinal) => ordinal,
            None => {
                let ordinal = self.seed_lens_space();
                self.events.push(AppEvent::WindowOpened);
                effects.push(Effect::OpenWindow { ordinal });
                ordinal
            },
        };
        (ordinal, effects)
    }

    /// Land a legacy leaf in one chosen lens, beside that lens's final leaf.
    pub(super) fn land_leaf_at_lens(&mut self, leaf: PaneNode, ordinal: usize) {
        if let Some(Some(lens)) = self.lenses.get_mut(ordinal) {
            let anchor_path = lens
                .iter_leaves()
                .last()
                .map(|(id, _, _)| id)
                .and_then(|id| crate::pane::path_of(lens, id))
                .unwrap_or_default();
            lens.summon_leaf(&anchor_path, InsertSide::Right, leaf);
        }
    }

    /// Land `leaf` in the newest live lens that is not `exclude` (a tear-out
    /// must LEAVE its source window), spawning a lens when none qualifies.
    /// Returns the effects (an `OpenWindow` when a lens spawned).
    pub(super) fn land_leaf_in_lens(
        &mut self,
        leaf: PaneNode,
        exclude: Option<SpaceRef>,
    ) -> Vec<Effect> {
        let (ordinal, effects) = self.target_lens(exclude);
        self.land_leaf_at_lens(leaf, ordinal);
        effects
    }

    /// The space holding `pane`: the primary tree, else the live lens whose
    /// tree carries it. Pane ids are minted from one counter, so the answer is
    /// unique — this is how a pane-anchored op (close, divider, summon-beside,
    /// tear-out) finds which window's tree to mutate.
    pub fn space_of(&self, pane: PaneId) -> Option<SpaceRef> {
        if self.frisket.iter_leaves().any(|(id, _, _)| id == pane) {
            return Some(SpaceRef::Primary);
        }
        self.lenses.iter().enumerate().find_map(|(i, s)| {
            s.as_ref()
                .filter(|space| space.iter_leaves().any(|(id, _, _)| id == pane))
                .map(|_| SpaceRef::Lens(i))
        })
    }

    /// The layout a [`SpaceRef`] names, when it is live.
    pub fn space(&self, space: SpaceRef) -> Option<&FrisketLayout> {
        match space {
            SpaceRef::Primary => Some(&self.frisket),
            SpaceRef::Lens(i) => self.lenses.get(i).and_then(Option::as_ref),
        }
    }

    /// Mutable [`Self::space`].
    pub(crate) fn space_mut(&mut self, space: SpaceRef) -> Option<&mut FrisketLayout> {
        match space {
            SpaceRef::Primary => Some(&mut self.frisket),
            SpaceRef::Lens(i) => self.lenses.get_mut(i).and_then(Option::as_mut),
        }
    }

    /// Note a semantic event from outside `update` — the shell's own divergence
    /// (an interaction that missed, an affordance not yet wired) joins the same
    /// drained stream the update path feeds, so automation reads one channel.
    pub fn note(&mut self, event: AppEvent) {
        self.events.push(event);
    }
}
