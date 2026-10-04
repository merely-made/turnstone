// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Arrangement and physics actions shared by Graphshell's browser presentations.
//!
//! The old page's form and the tree's panel read their controls into these
//! typed inputs and call the same actions, so there is one implementation of
//! each Apply. The canvas owns the state; nothing here reads a DOM. Catalogs
//! come from the canvas (`CANVAS_LAYOUT_STRATEGIES`, `CANVAS_PHYSICS_*`).

use std::collections::HashMap;

use mere::canvas::{
    CANVAS_LAYOUT_STRATEGIES, Canvas, LayoutStats, PhysicsChoice, PhysicsOverlay,
    project_canvas_strategy_with_score_for_view,
};
use mere::kernel::geometry::PortablePoint;
use mere::kernel::graph::NodeKey;

use crate::product::ProjectionClock;

/// The arrangement picker's "no arrangement" choice: physics alone.
pub const FREE_ARRANGEMENT: &str = "free";
/// What a profile picker reads when the live pair names no profile.
pub const CUSTOM_PROFILE: &str = "custom";

/// The arrangement picker's choices, `(id, label)`: the canvas's graph-wide
/// strategies, then free.
pub fn arrangement_choices() -> impl Iterator<Item = (&'static str, &'static str)> {
    CANVAS_LAYOUT_STRATEGIES
        .iter()
        .copied()
        .chain([(FREE_ARRANGEMENT, "Free (physics alone)")])
}

/// The live profile id, or [`CUSTOM_PROFILE`].
pub fn profile_id(canvas: &Canvas) -> &'static str {
    canvas.physics_profile_id().unwrap_or(CUSTOM_PROFILE)
}

/// The overlays a form has ticked, in catalog order.
pub fn ticked_overlays(ticked: impl Fn(PhysicsOverlay) -> bool) -> Vec<PhysicsOverlay> {
    PhysicsOverlay::ALL
        .into_iter()
        .filter(|overlay| ticked(*overlay))
        .collect()
}

/// Apply physics: sources, overlays and law in one rebuild. Returns the
/// status, with the law's reason when it refused the overlays.
pub fn apply_physics(canvas: &mut Canvas, choice: &PhysicsChoice) -> String {
    let refused = canvas.set_physics_choice(choice).err();
    let status = with_overlays(
        format!("Physics set to {}", canvas.physics_law().label()),
        canvas,
    );
    match refused {
        Some(refusal) => format!("{status} · {}", refusal.reason),
        None => status,
    }
}

/// Apply a named profile's law and overlays. Sources are left as they are.
pub fn apply_profile(canvas: &mut Canvas, id: &str) -> Result<String, String> {
    if id.is_empty() || id == CUSTOM_PROFILE {
        return Err("choose a profile first".to_string());
    }
    if !canvas.apply_physics_profile(id) {
        return Err(format!("unknown physics profile {id}"));
    }
    Ok(with_overlays(
        format!("Profile {id}: {}", canvas.physics_law().label()),
        canvas,
    ))
}

/// The layout's room-by-mass signature where a law was applied, so a
/// receipt can say what the law did from there: rank above the start's,
/// density CV under it (Density's qualitative bar, ruled 2026-10-03).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LawStart {
    pub mass_area_rank: f32,
    pub density_cv: f32,
}

impl LawStart {
    pub fn of(canvas: &Canvas) -> Self {
        let stats = canvas.layout_stats();
        Self {
            mass_area_rank: stats.mass_area_rank,
            density_cv: stats.density_cv,
        }
    }

    /// Snapshot fields: the start's values and whether `now` beats them.
    pub fn fields(&self, now: &LayoutStats) -> [(&'static str, String); 4] {
        [
            (
                "law-start-mass-area-rank",
                format!("{:.2}", self.mass_area_rank),
            ),
            ("law-start-density-cv", format!("{:.3}", self.density_cv)),
            (
                "layout-rank-rose",
                (now.mass_area_rank > self.mass_area_rank).to_string(),
            ),
            (
                "layout-cv-fell",
                (now.density_cv < self.density_cv).to_string(),
            ),
        ]
    }
}

/// A receipt line for `log-layout <label>`: the law, the layout's signature
/// now, and where the law started.
pub fn layout_line(label: &str, canvas: &Canvas, start: Option<&LawStart>) -> String {
    let stats = canvas.layout_stats();
    let start = start.map_or_else(
        || "start unrecorded".to_string(),
        |start| {
            format!(
                "start rank {:.3} cv {:.3}",
                start.mass_area_rank, start.density_cv
            )
        },
    );
    format!(
        "layout {label}: law {} nodes {} rank {:.3} cv {:.3} overlaps {} spread {:.0}; {start}",
        canvas.physics_law().id(),
        canvas.graph().node_count(),
        stats.mass_area_rank,
        stats.density_cv,
        stats.overlaps,
        stats.spread,
    )
}

fn with_overlays(status: String, canvas: &Canvas) -> String {
    let overlays = canvas.physics_overlays();
    if overlays.is_empty() {
        return status;
    }
    let labels: Vec<_> = overlays.iter().map(|overlay| overlay.label()).collect();
    format!("{status} with {}", labels.join(", "))
}

/// An applied arrangement: its id, the status, and a transition when the
/// layout animates to its slots.
pub struct ArrangementApplied {
    pub layout_id: String,
    pub status: String,
    pub transition: Option<ArrangementTransition>,
}

/// Apply arrangement `layout_id` at `viewport`. Free drops the analytic
/// layout and lets the law alone place the graph; any other id projects its
/// strategy and animates there when positions change. Selection survives.
pub fn apply_arrangement(
    canvas: &mut Canvas,
    layout_id: &str,
    viewport: (u32, u32),
) -> Result<ArrangementApplied, String> {
    if !arrangement_choices().any(|(id, _)| id == layout_id) {
        return Err(format!("unknown arrangement {layout_id}"));
    }
    let selected = canvas.selected_members();
    if layout_id == FREE_ARRANGEMENT {
        // Reverting resumes a paused sim (the canvas's own rule).
        canvas.set_projection_score(None);
        canvas.set_layout_strategy(None);
        canvas.set_selected_members(&selected);
        return Ok(ArrangementApplied {
            layout_id: layout_id.to_string(),
            status: "Arrangement set to free: physics alone".to_string(),
            transition: None,
        });
    }
    let previous_score = canvas.projection_score().cloned();
    let extents = canvas.strategy_extents();
    let projection = project_canvas_strategy_with_score_for_view(
        layout_id,
        canvas.graph(),
        canvas.focused_key(),
        viewport.0,
        viewport.1,
        None,
        Some(&extents),
        true,
        canvas.camera().zoom,
        previous_score.as_ref(),
    );
    let transition = ArrangementTransition::between(canvas, &projection.positions)?;
    canvas.set_layout_strategy(Some(layout_id.to_string()));
    canvas.set_projection_score(projection.score);
    if transition.is_none() {
        canvas.apply_strategy_positions(&projection.positions);
    }
    canvas.note_strategy_computed(layout_id, viewport.0, viewport.1, canvas.focused_key());
    canvas.set_selected_members(&selected);
    if transition.is_none() {
        canvas.fit_to_content();
    }
    Ok(ArrangementApplied {
        layout_id: layout_id.to_string(),
        status: if transition.is_some() {
            format!("Arrangement changing to {layout_id}")
        } else {
            format!("Arrangement set to {layout_id}")
        },
        transition,
    })
}

/// Advance a running arrangement transition at host time `host_ms`. When it
/// lands, the final slots apply, the transition clears, and the landed
/// status comes back.
pub fn advance_arrangement(
    canvas: &mut Canvas,
    transition: &mut Option<ArrangementTransition>,
    layout_id: &str,
    host_ms: f64,
) -> Option<String> {
    let (positions, complete) = transition.as_mut()?.advance(host_ms);
    if complete {
        canvas.apply_strategy_positions(&positions);
        *transition = None;
        Some(format!("Arrangement set to {layout_id}"))
    } else {
        canvas.preview_strategy_positions(&positions);
        None
    }
}

/// One playback of an analytic arrangement change. Scenotime owns the
/// schedule; the host owns the frame clock it is advanced with.
pub struct ArrangementTransition {
    schedule: scenotime::TransitionSchedule,
    clock: ProjectionClock,
    node_of: HashMap<sceno::InstanceId, NodeKey>,
    start_positions: Vec<(NodeKey, PortablePoint)>,
    final_positions: Vec<(NodeKey, PortablePoint)>,
}

impl ArrangementTransition {
    /// A transition from the canvas's current geometry to `final_positions`,
    /// or `None` when nothing would move.
    pub fn between(
        canvas: &Canvas,
        final_positions: &[(NodeKey, PortablePoint)],
    ) -> Result<Option<Self>, String> {
        let final_of = final_positions.iter().copied().collect::<HashMap<_, _>>();
        let old_geometry = canvas.cartography_geometry();
        let old_of = old_geometry.iter().collect::<HashMap<_, _>>();
        let extents = canvas.strategy_extents();
        let mut nodes = canvas.graph().nodes().collect::<Vec<_>>();
        nodes.sort_by_key(|(_, node)| node.id);

        let mut scene = sceno::Scene::new();
        scene.generation = canvas.graph().revision();
        let mut node_of = HashMap::new();
        let mut start_positions = Vec::with_capacity(nodes.len());
        let mut operations = Vec::new();
        for (key, node) in nodes {
            let Some(target) = final_of.get(&key).copied() else {
                continue;
            };
            let current = old_of
                .get(&node.id)
                .map(|(x, y)| PortablePoint::new(*x, *y))
                .unwrap_or(target);
            let source = scene.intern_source(sceno::SourceRef::new(
                mere::canvas::MERE_GRAPH_ADAPTER,
                node.id.to_string(),
            ));
            let (width, height) = extents.get(&key).copied().unwrap_or((36.0, 36.0));
            let item = sceno::ProjectedItem {
                source,
                space: sceno::Scene::WORLD,
                transform: sceno::Transform2::translation(current.x, current.y),
                footprint: sceno::Footprint::Rect {
                    size: sceno::Size2::new(width, height),
                },
                representation: canvas
                    .projection_representation(key)
                    .cloned()
                    .unwrap_or(sceno::Representation::Card),
                layer: 0,
                visible: true,
                hit: None,
                channels: Vec::new(),
            };
            let instance = sceno::InstanceId(scene.items.len() as u32);
            node_of.insert(instance, key);
            start_positions.push((key, current));
            scene.items.push(item.clone());
            if current != target {
                let mut target_item = item;
                target_item.transform = sceno::Transform2::translation(target.x, target.y);
                operations.push(scenotime::SceneOp::UpdateItem {
                    index: instance,
                    value: target_item,
                });
            }
        }

        if operations.is_empty() {
            return Ok(None);
        }
        let before = scenotime::SceneSnapshot::from_dense(
            scenotime::SceneEpoch(1),
            scenotime::Revision(1),
            scene,
        )
        .map_err(|error| format!("could not build transition start: {error:?}"))?;
        let diff = scenotime::SceneDiff {
            epoch: before.epoch,
            base: before.revision,
            revision: scenotime::Revision(before.revision.0 + 1),
            operations,
        };
        let schedule = scenotime::TransitionSchedule::from_diff(
            &before,
            &diff,
            &scenotime::TransitionSpec::default(),
        )
        .map_err(|error| format!("could not schedule arrangement transition: {error:?}"))?;
        Ok(Some(Self {
            schedule,
            clock: ProjectionClock::default(),
            node_of,
            start_positions,
            final_positions: final_positions.to_vec(),
        }))
    }

    /// The positions at host time `host_ms`, and whether the transition is done.
    pub fn advance(&mut self, host_ms: f64) -> (Vec<(NodeKey, PortablePoint)>, bool) {
        let frame = self.schedule.sample_at(self.clock.observe(host_ms));
        if frame.complete {
            return (self.final_positions.clone(), true);
        }
        let mut positions = self
            .start_positions
            .iter()
            .copied()
            .collect::<HashMap<_, _>>();
        for sample in frame.items {
            let Some(key) = self.node_of.get(&sample.instance).copied() else {
                continue;
            };
            positions.insert(
                key,
                PortablePoint::new(
                    sample.value.transform.translate.x,
                    sample.value.transform.translate.y,
                ),
            );
        }
        let positions = self
            .start_positions
            .iter()
            .filter_map(|(key, _)| positions.get(key).copied().map(|position| (*key, position)))
            .collect();
        (positions, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mere::canvas::{
        CANVAS_PHYSICS_PROFILES, PhysicsDepthSource, PhysicsKindSource, PhysicsLaw,
        PhysicsMassSource,
    };

    /// Prints the reference fixture's relations as node-index pairs, for a
    /// probe elsewhere that needs the same topology. Diagnostic only.
    #[test]
    #[ignore = "diagnostic: prints the fixture topology"]
    fn print_fixture_topology() {
        let persona = crate::mere_host::SelectedPersonaRef {
            persona: crate::mere_host::FIXTURE_PERSONA_ADDRESS.to_string(),
            profile: "profile:graphshell-tree".to_string(),
        };
        let app = crate::app::GraphshellApp::fixture(muniment::MemoryBackend::new(), persona)
            .expect("fixture");
        let graph = app.host.graph();
        let mut keys: Vec<_> = graph.nodes().map(|(key, _)| key).collect();
        keys.sort_by_key(|key| key.index());
        let index = |key| keys.iter().position(|k| *k == key).unwrap();
        let pairs: Vec<String> = graph
            .relations()
            .map(|r| format!("({}, {})", index(r.from), index(r.to)))
            .collect();
        println!(
            "FIXTURE nodes {} relations [{}]",
            keys.len(),
            pairs.join(", ")
        );
    }

    fn canvas() -> Canvas {
        let mut canvas = Canvas::with_sample_graph();
        canvas.resize(800, 600);
        canvas
    }

    #[test]
    fn physics_apply_sets_sources_overlays_and_law_together() {
        let mut canvas = canvas();
        let choice = PhysicsChoice {
            law: PhysicsLaw::Kinds,
            overlays: ticked_overlays(|overlay| {
                matches!(
                    overlay,
                    PhysicsOverlay::Skeleton | PhysicsOverlay::DegreeRepulsion
                )
            }),
            kind: PhysicsKindSource::Degree,
            mass: PhysicsMassSource::PageRank,
            depth: PhysicsDepthSource::Focus,
        };
        assert_eq!(
            choice.overlays,
            [PhysicsOverlay::DegreeRepulsion, PhysicsOverlay::Skeleton],
            "ticked overlays come in catalog order"
        );
        let status = apply_physics(&mut canvas, &choice);
        assert_eq!(canvas.physics_choice(), choice);
        assert_eq!(status, "Physics set to Kinds with Hub room, Skeleton");
        assert_eq!(profile_id(&canvas), CUSTOM_PROFILE);
        let status = apply_physics(
            &mut canvas,
            &PhysicsChoice {
                law: PhysicsLaw::Springs,
                ..choice
            },
        );
        assert_eq!(status, "Physics set to Springs with Hub room, Skeleton");
    }

    #[test]
    fn a_profile_synchronizes_law_and_overlays_and_keeps_sources() {
        let mut canvas = canvas();
        apply_physics(
            &mut canvas,
            &PhysicsChoice {
                kind: PhysicsKindSource::Component,
                ..PhysicsChoice::default()
            },
        );
        for profile in CANVAS_PHYSICS_PROFILES {
            apply_profile(&mut canvas, profile.id).unwrap();
            let live = canvas.physics_choice();
            assert_eq!(live.law, profile.law, "{}", profile.id);
            assert_eq!(live.overlays, profile.overlays, "{}", profile.id);
            assert_eq!(live.kind, PhysicsKindSource::Component, "sources stay");
            assert_eq!(profile_id(&canvas), profile.id, "names itself back");
        }
        assert_eq!(
            apply_profile(&mut canvas, "liquid").unwrap(),
            "Profile liquid: Springs with Centre"
        );
        assert!(apply_profile(&mut canvas, CUSTOM_PROFILE).is_err());
        assert!(apply_profile(&mut canvas, "").is_err());
        assert!(apply_profile(&mut canvas, "plasma").is_err());
        assert_eq!(
            profile_id(&canvas),
            "liquid",
            "a refused profile changes nothing"
        );
    }

    #[test]
    fn a_custom_pair_names_no_profile_until_it_matches_again() {
        let mut canvas = canvas();
        apply_profile(&mut canvas, "void").unwrap();
        let mut choice = canvas.physics_choice();
        choice.overlays = vec![PhysicsOverlay::Skeleton];
        apply_physics(&mut canvas, &choice);
        assert_eq!(profile_id(&canvas), CUSTOM_PROFILE);
        choice.overlays.clear();
        apply_physics(&mut canvas, &choice);
        assert_eq!(profile_id(&canvas), "void");
    }

    #[test]
    fn free_drops_the_strategy_and_an_analytic_arrangement_restores_it() {
        let mut canvas = canvas();
        assert!(apply_arrangement(&mut canvas, "spiral", (800, 600)).is_err());
        let applied = apply_arrangement(&mut canvas, "grid.default", (800, 600)).unwrap();
        assert_eq!(applied.layout_id, "grid.default");
        assert!(canvas.physics_paused(), "an analytic arrangement pauses");
        if let Some(transition) = applied.transition {
            let mut transition = Some(transition);
            let mut landed = None;
            for frame in 0..600 {
                landed = advance_arrangement(
                    &mut canvas,
                    &mut transition,
                    "grid.default",
                    frame as f64 * 16.0,
                );
                if landed.is_some() {
                    break;
                }
            }
            assert_eq!(landed.as_deref(), Some("Arrangement set to grid.default"));
            assert!(transition.is_none());
        }
        let applied = apply_arrangement(&mut canvas, FREE_ARRANGEMENT, (800, 600)).unwrap();
        assert!(applied.transition.is_none());
        assert_eq!(applied.status, "Arrangement set to free: physics alone");
        assert!(!canvas.physics_paused(), "free resumes the law");
        assert!(
            arrangement_choices().any(|(id, _)| id == FREE_ARRANGEMENT),
            "free is offered"
        );
        assert_eq!(
            arrangement_choices().count(),
            CANVAS_LAYOUT_STRATEGIES.len() + 1
        );
    }

    /// The tree page's canvas at the receipts' 1400 by 900 window.
    const TREE_CANVAS: (u32, u32) = (982, 627);

    /// The P2 fixture as the tree page opens it (`web_tree.rs`,
    /// `web_graphs::prepared_canvas`): the reference host's graph on the boot
    /// Spiral, fitted to the canvas.
    fn p2_fixture_canvas() -> Canvas {
        use crate::app::GraphshellApp;
        use crate::mere_host::{FIXTURE_PERSONA_ADDRESS, SelectedPersonaRef};
        const SPIRAL: &str = "phyllotaxis.default";
        let persona = SelectedPersonaRef {
            persona: FIXTURE_PERSONA_ADDRESS.to_string(),
            profile: "profile:graphshell-tree".to_string(),
        };
        let app = GraphshellApp::fixture(muniment::MemoryBackend::new(), persona).unwrap();
        let (width, height) = TREE_CANVAS;
        let mut canvas = Canvas::with_graph(app.host.graph().clone());
        canvas.resize(width, height);
        canvas.set_layout_strategy(Some(SPIRAL.to_string()));
        let positions = mere::canvas::project_canvas_strategy(
            SPIRAL,
            canvas.graph(),
            None,
            width,
            height,
            None,
            None,
            true,
        );
        canvas.apply_strategy_positions(&positions);
        canvas.fit_to_content();
        canvas
    }

    /// F10 (dynamics grammar plan, G1): Kinds reached as its receipt reaches
    /// it (Play, Free, Kinds by site), whose Play is the play control's
    /// continuous run, then 1 800 frames at 60 Hz, one step each. Its kinetic
    /// energy stays above the P2 floor of 1 at 6 s and at 30 s, so Kinds
    /// never rests on the P2 fixture.
    #[test]
    fn kinds_never_rests_on_the_p2_fixture() {
        use std::time::Duration;
        let (width, height) = TREE_CANVAS;
        let mut canvas = p2_fixture_canvas();
        canvas.set_physics_paused(false);
        apply_arrangement(&mut canvas, FREE_ARRANGEMENT, TREE_CANVAS).unwrap();
        apply_physics(
            &mut canvas,
            &PhysicsChoice {
                law: PhysicsLaw::Kinds,
                kind: PhysicsKindSource::Site,
                ..PhysicsChoice::default()
            },
        );
        let mut steps = 0;
        let mut readings = Vec::new();
        for frame in 0..=1800u64 {
            canvas.frame_at(
                width,
                height,
                Duration::from_micros(frame * 1_000_000 / 60),
                Default::default(),
            );
            steps += canvas
                .elapsed_step_report()
                .map_or(0, |report| report.steps);
            if [60, 360, 1800].contains(&frame) {
                readings.push((frame, steps, canvas.physics_energy()));
            }
        }
        println!("kinds on the P2 fixture (frame, steps, energy): {readings:?}");
        for &(frame, steps, energy) in &readings {
            assert_eq!(
                u64::from(steps),
                frame - 1,
                "a step every frame after the first"
            );
            assert!(energy >= 1.0, "kinds at frame {frame}: energy {energy}");
        }
    }

    /// The law-start fields say "rose" and "fell" only when the layout beats
    /// the start; the same stats read false on both (the control).
    #[test]
    fn law_start_reads_rose_and_fell_only_past_the_start() {
        let start = LawStart {
            mass_area_rank: 0.2,
            density_cv: 0.4,
        };
        let field = |now: &LayoutStats, name: &str| {
            start
                .fields(now)
                .into_iter()
                .find(|(field, _)| *field == name)
                .map(|(_, value)| value)
                .unwrap()
        };
        let better = LayoutStats {
            mass_area_rank: 0.7,
            density_cv: 0.2,
            ..LayoutStats::default()
        };
        let same = LayoutStats {
            mass_area_rank: 0.2,
            density_cv: 0.4,
            ..LayoutStats::default()
        };
        assert_eq!(field(&better, "layout-rank-rose"), "true");
        assert_eq!(field(&better, "layout-cv-fell"), "true");
        assert_eq!(field(&same, "layout-rank-rose"), "false");
        assert_eq!(field(&same, "layout-cv-fell"), "false");
        assert_eq!(field(&same, "law-start-mass-area-rank"), "0.20");
        assert_eq!(field(&same, "law-start-density-cv"), "0.400");
    }
}
