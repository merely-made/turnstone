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

use cambium::{AnyView, GenetAppRunner, GenetCtx, GenetElement, PointerClick, lens};
use cambium_genet_winit_host::{HostOptions, Init, ScrollAlign, inert_hooks, run};
use genet_scripted_dom::NodeId;
use graphshell::goal_surface::{self, GoalSurface, Start, fixture};
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
        [flag, path, size_flag, dimensions] if flag == "--receipt" && size_flag == "--size" => {
            let (w, h) = dimensions.split_once('x').expect("WIDTHxHEIGHT");
            (
                Some(PathBuf::from(path)),
                (w.parse().expect("width"), h.parse().expect("height")),
            )
        },
        _ => panic!("Usage: goal-surface-host [--receipt DIRECTORY [--size WIDTHxHEIGHT]]"),
    };
    if let Some(path) = &receipt {
        std::fs::create_dir_all(path).expect("receipt directory");
    }
    let pages = Rc::new(pollster::block_on(fixture::pages()));
    let state = App {
        surface: GoalSurface::new(pages.clone()),
    };
    let finite = receipt.is_some();
    let started = Instant::now();
    let mut hooks = inert_hooks::<App, Logic, View>();
    hooks.after_wake = Box::new(move |ctx| {
        if finite && started.elapsed() >= Duration::from_secs(35) {
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
            assert!(frames < 500, "bounded native receipt did not finish");
            if done.replace(false) {
                armed = false;
                stage += 1;
                entered = frames;
                let target = match stage {
                    1 => {
                        ctx.runner.update(|app| {
                            app.surface.goal.insert_str(GOAL);
                            app.surface.start = Start::Source(start_index);
                        });
                        click(ctx.runner, "goal-run");
                        "goal-input"
                    },
                    2 => "goal-steps",
                    _ => {
                        let app = ctx.runner.state();
                        let run = app.surface.run.as_ref().expect("a run");
                        assert!(run.matches().count() >= 2, "cited results");
                        let result = serde_json::json!({
                            "status": "passed",
                            "frames": frames,
                            "captures": *captured.borrow(),
                            "goal": GOAL,
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
            let ready = match stage {
                0 => true,
                1 => ctx.runner.state().surface.run.is_some(),
                2 => true,
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
