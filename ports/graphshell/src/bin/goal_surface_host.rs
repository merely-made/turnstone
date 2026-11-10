// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Headed native proof of the goal surface on the shared Cambium host. It
//! opens the surface over the synthetic retained fixture (page text in the
//! real `PageTextStore`, links in a real graph), enters a goal, runs it, and
//! captures the presented frames of the results and the traversal
//! explanation. With `--receipt DIRECTORY` it is finite: it writes stage PNGs,
//! per-stage pixel evidence and `qualification.json`, then closes.
//!
//! Input is state-level (the goal field's buffer is filled and the Find
//! button's DOM click is dispatched); no physical pointer or keyboard is
//! qualified.
//!
//! With feature `goal-surface-model`, the surface offers the opt-in model
//! toggle; `GRAPHSHELL_GOAL_MODEL_DIR` names the local model folder. The
//! receipt flag `--model` runs the model proof: a paraphrase goal that shares
//! no term with any page, run first with the toggle off, then with the model.

use cambium::{AnyView, GenetAppRunner, GenetCtx, GenetElement, PointerClick, lens};
use cambium_genet_winit_host::{HostOptions, Init, ScrollAlign, inert_hooks, run};
use genet_scripted_dom::NodeId;
use graphshell::goal_surface::{GoalSurface, Start};
use graphshell_host::goal_surface::{self, fixture};
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const GOAL: &str = "keep a starter alive while travelling";

struct App {
    surface: GoalSurface,
}
type View = Box<dyn AnyView<App, (), GenetCtx, GenetElement>>;
type Logic = fn(&App) -> View;
type Runner = GenetAppRunner<App, Logic, View>;
fn view(_: &App) -> View {
    Box::new(cambium::el(
        "main",
        lens(
            |surface: &mut GoalSurface| goal_surface::view(surface),
            |app: &mut App| &mut app.surface,
        ),
    ))
}
fn find(dom: &genet_scripted_dom::ScriptedDom, node: NodeId, id: &str) -> Option<NodeId> {
    if dom.attribute(node, &Namespace::from(""), &LocalName::from("id")) == Some(id) {
        return Some(node);
    }
    dom.dom_children(node).find_map(|child| find(dom, child, id))
}
fn click(runner: &mut Runner, id: &str) {
    let node = find(&runner.dom().borrow(), runner.root(), id).expect("mounted control");
    runner.dispatch_click(node, PointerClick::at((1., 1.)));
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (receipt, size) = match args.as_slice() {
        [] => (None, (1280., 920.)),
        [flag, path] if flag == "--receipt" => (Some(PathBuf::from(path)), (1280., 920.)),
        [flag, path, size_flag, dimensions] | [flag, path, size_flag, dimensions, _]
            if flag == "--receipt" && size_flag == "--size" =>
        {
            let (w, h) = dimensions.split_once('x').expect("WIDTHxHEIGHT");
            (
                Some(PathBuf::from(path)),
                (w.parse().expect("width"), h.parse().expect("height")),
            )
        },
        _ => panic!("Usage: goal-surface-host [--receipt DIRECTORY [--size WIDTHxHEIGHT] [--model]]"),
    };
    let with_model = args.last().is_some_and(|a| a == "--model");
    #[cfg(not(feature = "goal-surface-model"))]
    assert!(!with_model, "--model needs the goal-surface-model feature");
    if let Some(path) = &receipt {
        std::fs::create_dir_all(path).expect("receipt directory");
    }
    let pages = Rc::new(pollster::block_on(fixture::pages()));
    #[allow(unused_mut)]
    let mut state = App {
        surface: GoalSurface::new(pages.clone()),
    };
    #[cfg(feature = "goal-surface-model")]
    {
        state.surface.model.dir = goal_surface::model_path::dir_from_env();
    }
    let max_frames: u32 = if with_model { 4000 } else { 500 };
    let deadline = Duration::from_secs(if with_model { 180 } else { 35 });
    let finite = receipt.is_some();
    let started = Instant::now();
    let mut hooks = inert_hooks::<App, Logic, View>();
    hooks.after_wake = Box::new(move |ctx| {
        #[cfg(feature = "goal-surface-model")]
        ctx.runner.update(|app| {
            app.surface.poll_model();
        });
        if finite && started.elapsed() >= deadline {
            eprintln!("[goal-proof] finite startup/presentation deadline reached");
            *ctx.close = true;
        }
    });
    hooks.frame = Box::new(move |_| finite);
    if let Some(out) = receipt {
        let done = Rc::new(Cell::new(false));
        let captured = Rc::new(RefCell::new(Vec::new()));
        let pending = Rc::new(RefCell::new(
            None::<(usize, PathBuf, cambium_rootstock::CaptureFrame)>,
        ));
        let (mut stage, mut frames, mut entered, mut armed) = (0_usize, 0_u32, 0_u32, false);
        #[allow(unused_mut)]
        let mut off_matched: Option<usize> = None;
        let start_index = pages
            .sources()
            .iter()
            .position(|(source, _)| *source == goal_surface::source_id(fixture::OWNER, chartulary::resource_id(fixture::INDEX)))
            .expect("fixture index page");
        hooks.after_frame = Box::new(move |ctx| {
            let completed = pending.borrow_mut().as_mut().and_then(|(_, _, job)| job.poll());
            if let Some(result) = completed {
                let (stage, destination, _) = pending.borrow_mut().take().unwrap();
                let (presentation, frame) = result.expect("native readback");
                let colors: std::collections::HashSet<_> = frame
                    .rgba
                    .chunks_exact(4)
                    .map(|p| [p[0], p[1], p[2], p[3]])
                    .collect();
                image::save_buffer(&destination, &frame.rgba, frame.width, frame.height, image::ColorType::Rgba8)
                    .expect("PNG");
                let evidence = serde_json::json!({"stage":stage,"path":destination.file_name().unwrap().to_string_lossy(),"width":frame.width,"height":frame.height,"unique_colors":colors.len(),"presentation":format!("{presentation:?}")});
                eprintln!("[goal-proof] capture {evidence}");
                assert!(colors.len() >= 8, "uniform or blank capture");
                captured.borrow_mut().push(evidence);
                done.set(true);
            }
            frames += 1;
            assert!(frames < max_frames, "bounded native receipt did not finish");
            if done.replace(false) {
                armed = false;
                stage += 1;
                entered = frames;
                let target = match (with_model, stage) {
                    (false, 1) => {
                        ctx.runner.update(|app| {
                            app.surface.goal.insert_str(GOAL);
                            app.surface.start = Start::Source(start_index);
                        });
                        click(ctx.runner, "goal-run");
                        "goal-input"
                    },
                    #[cfg(feature = "goal-surface-model")]
                    (true, 1) => {
                        ctx.runner.update(|app| {
                            app.surface.goal.insert_str(fixture::GOAL_PARAPHRASE);
                            app.surface.start = Start::Source(start_index);
                        });
                        click(ctx.runner, "goal-run");
                        "goal-input"
                    },
                    #[cfg(feature = "goal-surface-model")]
                    (true, 2) => {
                        click(ctx.runner, "goal-model-toggle");
                        "goal-input"
                    },
                    #[cfg(feature = "goal-surface-model")]
                    (true, 3) => {
                        click(ctx.runner, "goal-run");
                        "goal-input"
                    },
                    (false, 2) | (true, 4) => "goal-steps",
                    _ => {
                        let app = ctx.runner.state();
                        let run = app.surface.run.as_ref().expect("a run");
                        assert!(with_model || run.matches().count() >= 2, "cited results");
                        let mut result = serde_json::json!({
                            "status": "passed",
                            "frames": frames,
                            "captures": *captured.borrow(),
                            "goal": if with_model { fixture::GOAL_PARAPHRASE } else { GOAL },
                            "start": fixture::INDEX,
                            "retained_pages": app.surface.pages().sources().len(),
                            "run": {
                                "stop": format!("{:?}", run.stop),
                                "matched": run.matched,
                                "steps": run.steps.len(),
                                "cited": run.matches().map(|s| s.address.clone()).collect::<Vec<_>>(),
                            },
                            "traversal_receipt": app.surface.receipt,
                            "source_profile": "synthetic fixture: page text in PageTextStore (two versions of one page), links in a real Mere graph",
                            "provider": "lexical (eidetic_search::goal::GoalAssessor under fleece::retained_traversal); no model",
                            "input": "goal buffer filled and Find clicked through Genet DOM callbacks by the finite host harness; no physical pointer/keyboard qualification",
                            "network": false,
                            "graph_writes": false,
                        });
                        #[cfg(feature = "goal-surface-model")]
                        if with_model {
                            use goal_surface::model_path::ModelState;
                            let ModelState::Ready(loaded) = &app.surface.model.state else { panic!("model not ready") };
                            result["provider"] = "eidetic_search::goal_model::ModelGoalAssessor (S1 plus all-MiniLM-L6-v2 via ESP/Burn CPU) under fleece::retained_traversal::run_retained_traversal_with".into();
                            result["model"] = serde_json::json!({
                                "dir": app.surface.model.dir,
                                "artifacts_sha256": loaded.digests,
                                "match_similarity_milli": loaded.threshold_milli,
                                "threshold_note": loaded.threshold_note,
                                "load_ms": loaded.load_ms,
                                "rule": format!("{:?}", app.surface.model.rule),
                                "lexical_matched": run.model.as_ref().map(|m| m.lexical_matched),
                                "unavailable_steps": run.model.as_ref().map(|m| m.unavailable_steps),
                                "off_run_matched": off_matched,
                            });
                        }
                        std::fs::write(out.join("qualification.json"), serde_json::to_vec_pretty(&result).unwrap()).unwrap();
                        println!("RESULT {result}");
                        *ctx.close = true;
                        return;
                    },
                };
                let node = find(&ctx.runner.dom().borrow(), ctx.runner.root(), target).expect("scroll target");
                ctx.scroll_into_view(node, ScrollAlign::Start);
            }
            if armed || ctx.presentation.is_none() || frames < entered + 3 {
                return;
            }
            let ready = match (with_model, stage) {
                (_, 0) | (false, 2) | (true, 4) => true,
                (false, 1) => ctx.runner.state().surface.run.is_some(),
                #[cfg(feature = "goal-surface-model")]
                (true, 1) => {
                    let ready = ctx.runner.state().surface.run.is_some();
                    if ready && off_matched.is_none() {
                        off_matched = ctx.runner.state().surface.run.as_ref().map(|r| r.matched);
                    }
                    ready
                },
                #[cfg(feature = "goal-surface-model")]
                (true, 2) => matches!(ctx.runner.state().surface.model.state, goal_surface::model_path::ModelState::Ready(_)),
                #[cfg(feature = "goal-surface-model")]
                (true, 3) => ctx.runner.state().surface.run.as_ref().is_some_and(|r| r.model.is_some()),
                _ => false,
            };
            if !ready {
                return;
            }
            armed = true;
            let destination = out.join(format!("stage-{stage}.png"));
            let sink = pending.clone();
            *ctx.capture = Some(Box::new(move |capture| {
                *sink.borrow_mut() = Some((stage, destination, capture.expect("native presentation capture")));
            }));
        });
    }
    let sheet = format!(
        "main{{font-family:sans-serif;font-size:16px;padding:16px;min-height:100vh;box-sizing:border-box;background:#f6f7f8;color:#20252b}}h1{{font-size:24px;margin:0 0 6px}}h2{{font-size:20px}}h3{{font-size:17px;margin:2px 0}}button{{color:#20252b;background:#edf1f5;border:1px solid #6d7681;padding:4px 8px}} {}",
        goal_surface::CSS
    );
    let running = Arc::new(AtomicBool::new(true));
    let thread_running = running.clone();
    let timer = Rc::new(RefCell::new(None));
    let timer_slot = timer.clone();
    let result = run(
        HostOptions {
            title: "Graphshell · Goal surface".into(),
            initial_logical_size: size,
            ..Default::default()
        },
        move |_, _, wake| {
            #[cfg(feature = "goal-surface-model")]
            {
                state.surface.model.waker = Some(wake.callback());
            }
            if finite {
                let callback = wake.callback();
                *timer_slot.borrow_mut() = Some(std::thread::spawn(move || {
                    while thread_running.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(250));
                        if thread_running.load(Ordering::Relaxed) {
                            callback();
                        }
                    }
                }));
            }
            Init {
                state,
                logic: view as Logic,
                sheet,
                fonts: vec![],
                images: vec![],
            }
        },
        hooks,
    );
    running.store(false, Ordering::Relaxed);
    if let Some(thread) = timer.borrow_mut().take() {
        thread.join().expect("proof wake timer");
    }
    result.expect("native Graphshell host");
}
