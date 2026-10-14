// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Runnable Graphshell composition proof, with the same owner fixture as its
//! CPU controls. These are public, unsealed in-memory owners; no live peer,
//! persistent community, document editor or activity owner is installed.

#[path = "../../examples/support/moot_resources_fixture.rs"]
mod fixture;

use cambium::{AnyView, GenetAppRunner, GenetCtx, GenetElement, PointerClick, el, lens};
use cambium_genet_winit_host::{HostOptions, Init, ScrollAlign, inert_hooks, run};
use fixture::{BODY, DESTINATION, Fixture};
use genet_scripted_dom::NodeId;
use graphshell::moot_resources::{Mount, Update};
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use notochord::RevocationLedger;
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

struct App {
    alice: Mount,
    bob: Mount,
}
type View = Box<dyn AnyView<App, (), GenetCtx, GenetElement>>;
type Logic = fn(&App) -> View;
type Runner = GenetAppRunner<App, Logic, View>;
fn view(_: &App) -> View {
    Box::new(el(
        "main",
        (
            el("h1", "Friends' notebooks"),
            el(
                "p",
                "Local owner composition proof · two independent admitted readings · public in-memory content",
            ),
            el(
                "div",
                (
                    el(
                        "section",
                        (
                            el("h2", "Alice's reading"),
                            lens(
                                |mount: &mut Mount| graphshell::moot_resources::view(mount),
                                |app: &mut App| &mut app.alice,
                            ),
                        ),
                    )
                    .attr("id", "alice-reading")
                    .attr("class", "reading"),
                    el(
                        "section",
                        (
                            el("h2", "Bob's reading"),
                            lens(
                                |mount: &mut Mount| graphshell::moot_resources::view(mount),
                                |app: &mut App| &mut app.bob,
                            ),
                        ),
                    )
                    .attr("id", "bob-reading")
                    .attr("class", "reading"),
                ),
            )
            .attr("class", "readings"),
        ),
    ))
}
fn find(dom: &genet_scripted_dom::ScriptedDom, node: NodeId, id: &str) -> Option<NodeId> {
    if dom.attribute(node, &Namespace::from(""), &LocalName::from("id")) == Some(id) {
        return Some(node);
    }
    dom.dom_children(node)
        .find_map(|child| find(dom, child, id))
}
fn click(runner: &mut Runner, id: &str) {
    let node = find(&runner.dom().borrow(), runner.root(), id).expect("mounted control");
    runner.dispatch_click(node, PointerClick::at((1., 1.)));
}

fn main() {
    eprintln!("[moot-proof] main entered");
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
        _ => panic!("Usage: moot-resource-host [--receipt DIRECTORY [--size WIDTHxHEIGHT]]"),
    };
    if let Some(path) = &receipt {
        std::fs::create_dir_all(path).expect("receipt directory");
    }
    let f = Rc::new(pollster::block_on(Fixture::new()));
    eprintln!("[moot-proof] signed memory fixture ready");
    let state = App {
        alice: pollster::block_on(f.mount("alice", &f.alice)),
        bob: pollster::block_on(f.mount("bob", &f.bob)),
    };
    eprintln!("[moot-proof] both admitted mounts ready");
    let bob_clock = Rc::new(Cell::new(20_u64));
    let mut hooks = inert_hooks::<App, Logic, View>();
    let pump = f.clone();
    let clock = bob_clock.clone();
    let finite = receipt.is_some();
    let deadline_started = Instant::now();
    // Proof-only bounded wake/deadline control. A failed drawable remains a
    // failure; only an actual fresh presentation advances the stage driver.
    hooks.after_wake = Box::new(move |ctx| {
        if finite && deadline_started.elapsed() >= Duration::from_secs(35) {
            eprintln!("[moot-proof] finite startup/presentation deadline reached");
            *ctx.close = true;
        }
    });
    let mut frame_hooks = 0_u32;
    hooks.frame = Box::new(move |ctx| {
        frame_hooks += 1;
        if finite && frame_hooks <= 5 {
            eprintln!("[moot-proof] frame-hook begin count={frame_hooks}");
        }
        let mut changed = false;
        ctx.runner.update_deferred(|app| {
            for (mount, identity, at_ms) in [
                (&mut app.alice, &pump.alice, 20),
                (&mut app.bob, &pump.bob, clock.get()),
            ] {
                let was_closed = mount.closed_reason().is_some();
                match pollster::block_on(pump.poll(
                    mount,
                    identity,
                    &RevocationLedger::default(),
                    at_ms,
                )) {
                    Ok(Some(Update::Refreshed | Update::Dispatched { .. })) => changed = true,
                    Err(_) if !was_closed => changed = true,
                    _ => {},
                }
            }
        });
        if changed {
            ctx.runner.update(|_| {});
        }
        if finite && frame_hooks <= 5 {
            eprintln!("[moot-proof] frame-hook end count={frame_hooks} changed={changed}");
        }
        finite || changed
    });
    if let Some(out) = receipt {
        let done = Rc::new(Cell::new(false));
        let captured = Rc::new(RefCell::new(Vec::new()));
        let pending = Rc::new(RefCell::new(
            None::<(usize, std::path::PathBuf, cambium_rootstock::CaptureFrame)>,
        ));
        let mut stage = 0_usize;
        let mut frames = 0_u32;
        let mut entered = 0_u32;
        let mut armed = false;
        let fixture = f.clone();
        hooks.after_frame = Box::new(move |ctx| {
            let completed = pending
                .borrow_mut()
                .as_mut()
                .and_then(|(_, _, job)| job.poll());
            if let Some(result) = completed {
                let (stage, destination, _) = pending.borrow_mut().take().unwrap();
                let (presentation, frame) = result.expect("native readback");
                let colors: std::collections::HashSet<_> = frame
                    .rgba
                    .chunks_exact(4)
                    .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
                    .collect();
                image::save_buffer(
                    &destination,
                    &frame.rgba,
                    frame.width,
                    frame.height,
                    image::ColorType::Rgba8,
                )
                .expect("PNG");
                let pixel_report = serde_json::json!({"stage":stage,"width":frame.width,"height":frame.height,"unique_colors":colors.len(),"first_pixel":frame.rgba.get(..4),"presentation":format!("{presentation:?}")});
                std::fs::write(
                    destination.with_extension("json"),
                    serde_json::to_vec_pretty(&pixel_report).unwrap(),
                )
                .expect("pixel evidence");
                eprintln!("[moot-proof] capture {pixel_report}");
                assert!(colors.len() >= 8, "uniform or blank capture");
                captured.borrow_mut().push(serde_json::json!({"stage":stage,"path":destination,"width":frame.width,"height":frame.height,"unique_colors":colors.len(),"presentation":format!("{presentation:?}")}));
                done.set(true);
            }
            frames += 1;
            if frames <= 5 {
                eprintln!(
                    "[moot-proof] after-frame count={frames} stage={stage} presentation={:?} profile={:?}",
                    ctx.presentation, ctx.frame_profile
                );
            }
            assert!(frames < 500, "bounded native receipt did not finish");
            if done.replace(false) {
                armed = false;
                stage += 1;
                entered = frames;
                match stage {
                    1 => {
                        let reference = ctx
                            .runner
                            .state()
                            .alice
                            .reading()
                            .pane()
                            .snapshot()
                            .resources[0]
                            .reference;
                        let hex: String = reference
                            .contribution
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect();
                        click(ctx.runner, &format!("moot-alice-inspect-{hex}"));
                        click(ctx.runner, "moot-alice-open");
                    },
                    2 => click(ctx.runner, "moot-bob-fork-collection"),
                    3 => {
                        let reference =
                            ctx.runner.state().bob.reading().pane().snapshot().resources[0]
                                .reference;
                        let hex: String = reference
                            .contribution
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect();
                        click(ctx.runner, &format!("moot-bob-inspect-{hex}"));
                        click(ctx.runner, "moot-bob-open");
                        bob_clock.set(1_001);
                    },
                    _ => {
                        let app = ctx.runner.state();
                        assert_eq!(app.alice.opened().unwrap().bytes, BODY);
                        assert!(app.bob.closed_reason().is_some());
                        assert!(app.bob.opened().is_none());
                        assert!(
                            pollster::block_on(
                                fixture.moot.authorized_collection(
                                    gemot::moot::CollectionId(DESTINATION),
                                    25
                                )
                            )
                            .unwrap()
                            .unwrap()
                            .fork
                            .is_some()
                        );
                        let result = serde_json::json!({
                            "status":"passed", "frames":frames, "captures":*captured.borrow(),
                            "source_profile":"actual Graphshell mount; signed memory owners; verified sans-I/O session",
                            "checks":["independent readings", "local owner open", "signed reference fork and refreshed offer", "queued open refused on session expiry", "Alice's opened bytes preserved"],
                            "network":false, "persistent_restart":false, "private_content":false, "activity_owner":false,
                            "input":"real Genet DOM callbacks invoked by finite host harness; no physical pointer/keyboard qualification",
                        });
                        std::fs::write(
                            out.join("qualification.json"),
                            serde_json::to_vec_pretty(&result).unwrap(),
                        )
                        .unwrap();
                        println!("RESULT {}", result);
                        *ctx.close = true;
                        return;
                    },
                }
                let target = if stage == 1 {
                    "alice-reading"
                } else {
                    "bob-reading"
                };
                let node = find(&ctx.runner.dom().borrow(), ctx.runner.root(), target).unwrap();
                ctx.scroll_into_view(node, ScrollAlign::Start);
            }
            if armed || ctx.presentation.is_none() || frames < entered + 3 {
                return;
            }
            let ready = match stage {
                0 => true,
                1 => ctx.runner.state().alice.opened().is_some(),
                2 => {
                    ctx.runner
                        .state()
                        .bob
                        .reading()
                        .pane()
                        .snapshot()
                        .collection
                        .as_ref()
                        .unwrap()
                        .fork
                        != Some(moot::resource_coop::Offer::Offered)
                },
                3 => ctx.runner.state().bob.closed_reason().is_some(),
                _ => false,
            };
            if !ready {
                return;
            }
            armed = true;
            let destination = out.join(format!("stage-{stage}.png"));
            let sink = pending.clone();
            *ctx.capture = Some(Box::new(move |capture| {
                *sink.borrow_mut() = Some((
                    stage,
                    destination,
                    capture.expect("native presentation capture"),
                ));
            }));
        });
    }
    // Genet mounts this composition's main directly under the document. The
    // host owns its palette and typography, including an opaque surface for
    // the native frame's black clear; no synthetic body selector can supply it.
    let sheet = format!(
        "main{{font-family:sans-serif;font-size:16px;padding:16px;min-height:100vh;box-sizing:border-box;background:#f6f7f8;color:#20252b}}h1{{font-size:24px}}h2{{font-size:20px}}h3{{font-size:18px}}.readings{{display:flex;flex-wrap:wrap;gap:20px}}.reading{{flex:1;min-width:300px;padding:12px;border:1px solid #999;background:#fff}}button{{color:#20252b;background:#edf1f5;border:1px solid #6d7681;padding:4px 8px}}pre{{white-space:pre-wrap;overflow-wrap:anywhere}} {}",
        moot::resource_view::CSS
    );
    let running = Arc::new(AtomicBool::new(true));
    let thread_running = running.clone();
    let timer = Rc::new(RefCell::new(None));
    let timer_slot = timer.clone();
    let result = run(
        HostOptions {
            title: "Graphshell · Moot resources".into(),
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
