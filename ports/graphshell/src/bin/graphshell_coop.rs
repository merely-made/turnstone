// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Native Graphshell co-op surface. All remote actions travel through the
//! ordinary admitted projection carrier; the UI owns no graph or signing key.
use cambium::{
    AnyView, GenetCtx, GenetElement, PointerClick, TextInput, button, el, lens, text_field_typed,
};
use cambium_genet_winit_host::{HostOptions, Init, ScrollAlign, inert_hooks, run};
use graphshell::coop::{
    self, Contact, Data, Edit, Invitation, Operation, Owner,
    network::{self, Guest},
};
use muniment::RedbBackend;
use pandect::SessionId;
use personae::IdentityProvider;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

enum Command {
    Edit(Edit),
    Invite {
        contact: PathBuf,
        output: PathBuf,
        write: bool,
    },
    Revoke,
    Reconnect,
    Refresh,
    Stop,
}
#[derive(Clone, PartialEq, Serialize)]
struct Update {
    data: Option<Data>,
    status: String,
    root: String,
}
struct App {
    tx: mpsc::Sender<Command>,
    rx: mpsc::Receiver<Update>,
    update: Update,
    inputs: Vec<TextInput>,
    owner: bool,
}
impl App {
    fn send(&mut self, c: Command) {
        if self.tx.send(c).is_ok() {
            self.update.status = "Working…".into()
        } else {
            self.update.status = "Session worker stopped".into()
        }
    }
    fn add(&mut self) {
        if let Some(data) = &self.update.data {
            self.send(Command::Edit(data.edit(Operation::Add {
                id: Uuid::new_v4(),
                address: self.inputs[0].text().trim().into(),
                title: self.inputs[1].text().trim().into(),
            })))
        }
    }
    fn rename(&mut self) {
        let id = Uuid::parse_str(self.inputs[2].text().trim());
        match (id, self.update.data.as_ref()) {
            (Ok(id), Some(data)) => self.send(Command::Edit(data.edit(Operation::Rename {
                id,
                title: self.inputs[1].text().trim().into(),
            }))),
            _ => self.update.status = "Choose an object by its id".into(),
        }
    }
    fn invite(&mut self, write: bool) {
        self.send(Command::Invite {
            contact: self.inputs[3].text().into(),
            output: self.inputs[4].text().into(),
            write,
        })
    }
}
type View = Box<dyn AnyView<App, (), GenetCtx, GenetElement>>;
type Logic = fn(&App) -> View;
fn field(i: usize, label: &'static str) -> View {
    Box::new(el(
        "label",
        (
            label,
            lens(
                move |input: &mut TextInput| {
                    text_field_typed(input)
                        .attr("id", format!("field-{i}"))
                        .attr("aria-label", label)
                },
                move |app: &mut App| &mut app.inputs[i],
            ),
        ),
    ))
}
fn view(app: &App) -> View {
    let mut body: Vec<View> = vec![
        Box::new(el("h1", "Graphshell · Co-op")),
        Box::new(el(
            "p",
            if app.owner {
                "You hold this graph. Your guest receives access to this graph only."
            } else {
                "You are visiting one graph. The holder saves every accepted edit."
            },
        )),
        Box::new(el("p", format!("Your identity: {}", app.update.root))),
        Box::new(
            el("p", app.update.status.clone())
                .attr("id", "status")
                .attr("role", "status"),
        ),
    ];
    if let Some(data) = &app.update.data {
        body.push(Box::new(
            el(
                "p",
                format!(
                    "Graph {} · revision {} · guest {} · {}",
                    data.graph.as_uuid(),
                    data.revision.0,
                    data.guest.as_deref().unwrap_or("none"),
                    data.access
                ),
            )
            .attr("id", "scope"),
        ));
        let mut cards: Vec<View> = vec![];
        for node in &data.nodes {
            let id = node.id;
            cards.push(Box::new(
                el(
                    "article",
                    (
                        el("h2", node.title.clone()),
                        el("p", node.address.clone()),
                        button("Select object", move |app: &mut App, _| {
                            app.inputs[2] = TextInput::new(id.to_string());
                        })
                        .attr("id", format!("select-{id}")),
                    ),
                )
                .attr("class", "node"),
            ));
        }
        body.push(Box::new(
            el("section", cards)
                .attr("class", "graph")
                .attr("aria-label", "Shared graph"),
        ));
        let selected = data
            .nodes
            .iter()
            .find(|node| node.id.to_string() == app.inputs[2].text());
        body.push(Box::new(el(
            "p",
            format!(
                "Selected object: {}",
                selected.map_or("none", |n| n.title.as_str())
            ),
        )));
        body.extend([field(0, "Address"), field(1, "Title")]);
        body.push(Box::new(el(
            "div",
            (
                button("Add address", |app: &mut App, _| app.add()).attr("id", "add"),
                button("Save title", |app: &mut App, _| app.rename()).attr("id", "rename"),
                button("Refresh graph", |app: &mut App, _| {
                    app.send(Command::Refresh)
                })
                .attr("id", "refresh"),
            ),
        )));
        let authors = data
            .authors
            .iter()
            .rev()
            .take(3)
            .map(|a| {
                let who = if a.id == format!("personae:{}", data.owner) {
                    "Owner"
                } else if data
                    .guest
                    .as_ref()
                    .is_some_and(|g| a.id == format!("personae:{g}"))
                {
                    "Guest"
                } else {
                    "Saved author"
                };
                format!("{who}: {}", a.id.trim_start_matches("personae:"))
            })
            .collect::<Vec<_>>()
            .join("\n");
        body.push(Box::new(
            el("pre", format!("Saved activity\n{authors}")).attr("id", "authors"),
        ));
    }
    if app.owner {
        body.push(Box::new(el("h2", "Guest access")));
        body.extend([
            field(3, "Guest contact file"),
            field(4, "Invitation output file"),
        ]);
        body.push(Box::new(el(
            "div",
            (
                button("Invite editor", |app: &mut App, _| app.invite(true))
                    .attr("id", "invite-editor"),
                button("Invite reader", |app: &mut App, _| app.invite(false))
                    .attr("id", "invite-reader"),
                button("End guest access", |app: &mut App, _| {
                    app.send(Command::Revoke)
                })
                .attr("id", "revoke"),
            ),
        )));
    } else {
        body.push(Box::new(
            button("Reconnect to holder", |app: &mut App, _| {
                app.send(Command::Reconnect)
            })
            .attr("id", "reconnect"),
        ));
    }
    Box::new(el("main", body))
}
#[derive(Clone)]
struct Args {
    mode: String,
    store: PathBuf,
    graph: Option<SessionId>,
    handoff: PathBuf,
    invitation: PathBuf,
    contact: PathBuf,
    scenario: Option<PathBuf>,
}
fn args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let mode = it.next().unwrap_or_default();
    let mut a = Args {
        mode,
        store: PathBuf::new(),
        graph: None,
        handoff: PathBuf::new(),
        invitation: PathBuf::new(),
        contact: PathBuf::new(),
        scenario: None,
    };
    while let Some(k) = it.next() {
        let value = it.next().ok_or("option needs a value")?;
        match k.as_str() {
            "--store" => a.store = value.into(),
            "--graph" => {
                a.graph = Some(SessionId::from_uuid(
                    Uuid::parse_str(&value).map_err(|e| e.to_string())?,
                ))
            },
            "--handoff" => a.handoff = value.into(),
            "--invitation" => a.invitation = value.into(),
            "--contact" => a.contact = value.into(),
            "--scenario" => a.scenario = Some(value.into()),
            _ => return Err(format!("unknown option {k}")),
        }
    }
    if !["owner", "guest", "contact", "new-graph", "list"].contains(&a.mode.as_str()) {
        return Err("usage: graphshell-coop owner --store DB --graph UUID --handoff DIR | guest --invitation FILE --contact FILE --handoff DIR | contact --handoff DIR | new-graph|list --store DB".into());
    }
    Ok(a)
}
fn worker(a: Args, rx: mpsc::Receiver<Command>, tx: mpsc::Sender<Update>) -> Result<(), String> {
    let identity = network::identity()?;
    let root = coop::hex(&identity.master_public_key().to_bytes());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&a.handoff).map_err(|e| e.to_string())?;
    network::write_json(
        &a.handoff.join("identity.json"),
        &Contact::create(&*identity)?,
    )?;
    let owner = if a.mode == "owner" {
        let backend = RedbBackend::open(&a.store).map_err(|e| e.to_string())?;
        let product = Arc::new(Mutex::new(runtime.block_on(Owner::open(
            backend,
            a.graph.ok_or("choose --graph explicitly")?,
            identity.master_public_key().to_bytes(),
        ))?));
        let transport = runtime.block_on(network::bind(&*identity))?;
        let card = runtime.block_on(network::card(
            &*identity,
            product.lock().unwrap().graph_id(),
            &transport,
        ))?;
        network::write_json(&a.handoff.join("contact.json"), &card)?;
        let mut host = network::host(product.clone())?;
        let ledger = host.revocations().clone();
        let server = runtime.spawn(async move {
            loop {
                match host.accept_one(&*transport, coop::now_ms).await {
                    Ok(Ok(served)) => {
                        tokio::spawn(async move {
                            let _ = served.finished().await;
                        });
                    },
                    Ok(Err(reason)) => eprintln!("co-op admission refused: {reason:?}"),
                    Err(error) => {
                        eprintln!("co-op listener stopped: {error}");
                        break;
                    },
                }
            }
        });
        Some((product, ledger, server))
    } else {
        None
    };
    let mut guest: Option<Guest> = None;
    let mut status = "Ready".to_string();
    let mut connect = true;
    let mut last: Option<Update> = None;
    let mut force = true;
    loop {
        if identity.is_locked() {
            return Err("identity locked; co-op stopped".into());
        }
        if owner.is_none() && connect {
            connect = false;
            let result = network::read_json::<Invitation>(&a.invitation).and_then(|invitation| {
                network::read_json(&a.contact)
                    .and_then(|card| Guest::connect(&runtime, &*identity, invitation, card))
            });
            match result {
                Ok(value) => {
                    guest = Some(value);
                    status = "Connected to invited graph".into();
                },
                Err(reason) => {
                    guest = None;
                    status = format!("Unavailable: {reason}");
                },
            }
        }
        let data = if let Some((product, _, _)) = &owner {
            Some(product.lock().unwrap().owner_data())
        } else {
            guest.as_ref().and_then(|g| g.data.clone())
        };
        let update = Update {
            data,
            status: status.clone(),
            root: root.clone(),
        };
        if force || last.as_ref() != Some(&update) {
            if tx.send(update.clone()).is_err() {
                break;
            }
            last = Some(update);
        }
        let command = match rx.recv_timeout(Duration::from_millis(350)) {
            Ok(c) => Some(c),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(_) => break,
        };
        force = command.is_some();
        let reading = matches!(&command, None | Some(Command::Refresh));
        let outcome = match command {
            Some(Command::Stop) => break,
            Some(Command::Reconnect) => {
                guest = None;
                connect = true;
                Ok("Reconnecting…")
            },
            Some(Command::Edit(edit)) => {
                if let Some((product, _, _)) = &owner {
                    runtime
                        .block_on(product.lock().unwrap().edit_owner(edit))
                        .map(|_| "Saved")
                } else {
                    guest
                        .as_mut()
                        .ok_or_else(|| "holder is unavailable".to_string())
                        .and_then(|g| g.edit(edit))
                        .map(|_| "Saved by holder")
                }
            },
            Some(Command::Invite {
                contact,
                output,
                write,
            }) => {
                if let Some((product, _, _)) = &owner {
                    network::read_json::<Contact>(&contact)
                        .and_then(|contact| {
                            runtime.block_on(product.lock().unwrap().invite(
                                &*identity,
                                contact,
                                write,
                                coop::now_ms() + 24 * 60 * 60 * 1000,
                            ))
                        })
                        .and_then(|invitation| network::write_json(&output, &invitation))
                        .map(|_| "Invitation saved")
                } else {
                    Err("only the owner can invite".into())
                }
            },
            Some(Command::Revoke) => {
                if let Some((product, ledger, _)) = &owner {
                    let mut product = product.lock().unwrap();
                    runtime.block_on(product.revoke(&*identity)).and_then(|()| {
                        ledger.write().map_err(|_| "ledger lock")?.fold(
                            product
                                .revocation()
                                .unwrap()
                                .check()
                                .map_err(|e| format!("{e:?}"))?,
                        );
                        Ok("Guest access ended")
                    })
                } else {
                    Err("only the owner can end access".into())
                }
            },
            Some(Command::Refresh) => guest
                .as_mut()
                .map_or(Ok("Ready"), |g| g.refresh().map(|_| "Live")),
            None => guest
                .as_mut()
                .map_or(Ok("Ready"), |g| g.poll().map(|_| "Live")),
        };
        match outcome {
            Ok(message) => {
                if command_is_user_message(message) {
                    status = message.into()
                }
            },
            Err(reason) => {
                status = if reading {
                    format!("Unavailable: {reason}")
                } else {
                    format!("Refused: {reason}")
                }
            },
        }
    }
    if let Some((_, _, server)) = owner {
        server.abort();
    }
    Ok(())
}
fn command_is_user_message(message: &str) -> bool {
    message != "Live" && message != "Ready"
}

#[derive(Clone, Deserialize)]
struct Step {
    #[serde(default)]
    wait_file: Option<PathBuf>,
    #[serde(default)]
    set: Vec<String>,
    #[serde(default)]
    click: Option<String>,
    #[serde(default)]
    nodes: Option<usize>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    access: Option<String>,
    capture: PathBuf,
}
fn find(
    dom: &genet_scripted_dom::ScriptedDom,
    node: genet_scripted_dom::NodeId,
    id: &str,
) -> Option<genet_scripted_dom::NodeId> {
    use layout_dom_api::{LayoutDom, LocalName, Namespace};
    if dom.attribute(node, &Namespace::from(""), &LocalName::from("id")) == Some(id) {
        return Some(node);
    }
    dom.dom_children(node)
        .find_map(|child| find(dom, child, id))
}
fn main() {
    if let Err(error) = start() {
        eprintln!("Graphshell co-op: {error}");
        std::process::exit(1)
    }
}
fn start() -> Result<(), String> {
    let a = args()?;
    if a.mode == "contact" {
        let identity = network::identity()?;
        network::write_json(
            &a.handoff.join("identity.json"),
            &Contact::create(&*identity)?,
        )?;
        return Ok(());
    }
    if a.mode == "new-graph" || a.mode == "list" {
        let identity = network::identity()?;
        let backend = RedbBackend::open(&a.store).map_err(|e| e.to_string())?;
        let sessions = pandect::MereSessions::new(backend);
        if a.mode == "list" {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &pollster::block_on(sessions.list()).map_err(|e| e.to_string())?
                )
                .unwrap()
            );
        } else {
            let mut graph = sessions.begin(
                mere::kernel::graph::Author::person(format!(
                    "personae:{}",
                    coop::hex(&identity.master_public_key().to_bytes())
                ))
                .via("graphshell"),
                None,
            );
            pollster::block_on(graph.flush(std::time::SystemTime::now()))
                .map_err(|e| e.to_string())?;
            println!("{}", graph.id().as_uuid());
        }
        return Ok(());
    }
    let steps: Vec<Step> = match &a.scenario {
        Some(p) => network::read_json(p)?,
        None => vec![],
    };
    let finite = !steps.is_empty();
    let (tx, commands) = mpsc::channel();
    let (updates, rx) = mpsc::channel();
    let mode = a.mode.clone();
    let worker_args = a.clone();
    let errors = updates.clone();
    std::thread::spawn(move || {
        if let Err(reason) = worker(worker_args, commands, updates) {
            let _ = errors.send(Update {
                data: None,
                status: format!("Unavailable: {reason}"),
                root: String::new(),
            });
        }
    });
    let state = App {
        tx: tx.clone(),
        rx,
        owner: mode == "owner",
        update: Update {
            data: None,
            status: "Opening identity…".into(),
            root: String::new(),
        },
        inputs: vec![
            TextInput::new(""),
            TextInput::new(""),
            TextInput::new(""),
            TextInput::new(a.handoff.join("guest.json").display().to_string()),
            TextInput::new(a.handoff.join("invitation.json").display().to_string()),
        ],
    };
    let mut hooks = inert_hooks::<App, Logic, View>();
    let progress = a.handoff.join("progress.json");
    let started = Instant::now();
    hooks.frame = Box::new(move |ctx| {
        let mut changed = false;
        ctx.runner.update_deferred(|app| {
            while let Ok(update) = app.rx.try_recv() {
                app.update = update;
                changed = true;
            }
        });
        if changed {
            ctx.runner.update(|_| {});
            if finite {
                network::write_json(&progress, &ctx.runner.state().update)
                    .expect("scenario progress");
            }
        }
        if finite && started.elapsed() > Duration::from_secs(180) {
            eprintln!("co-op scenario deadline");
            *ctx.close = true;
        }
        changed || finite
    });
    let captured = Rc::new(RefCell::new(
        None::<(PathBuf, cambium_rootstock::CaptureFrame)>,
    ));
    let pending = captured.clone();
    let mut index = 0;
    let mut armed = false;
    let mut prepared = false;
    let mut overview = false;
    let mut failed = false;
    let mut frames = 0;
    let output = a.handoff.clone();
    hooks.after_frame = Box::new(move |ctx| {
        frames += 1;
        let result = pending
            .borrow_mut()
            .as_mut()
            .and_then(|(_, job)| job.poll());
        if let Some(result) = result {
            let (path, _) = pending.borrow_mut().take().unwrap();
            let (presentation, frame) = result.expect("native capture");
            image::save_buffer(
                &path,
                &frame.rgba,
                frame.width,
                frame.height,
                image::ColorType::Rgba8,
            )
            .expect("capture PNG");
            let colors: std::collections::HashSet<_> =
                frame.rgba.as_chunks::<4>().0.iter().copied().collect();
            assert!(colors.len() > 8, "blank capture");
            network::write_json(&path.with_extension("json"),&serde_json::json!({"pid":std::process::id(),"frames":frames,"presentation":format!("{presentation:?}"),"colors":colors.len(),"state":ctx.runner.state().update})).unwrap();
            if failed {
                eprintln!("co-op headed refusal: {}", ctx.runner.state().update.status);
                *ctx.close = true;
                return;
            }
            index += 1;
            armed = false;
            prepared = false;
            overview = false;
            if index == steps.len() {
                network::write_json(&output.join("complete.json"),&serde_json::json!({"result":"ok","pid":std::process::id(),"frames":frames,"captures":index,"state":ctx.runner.state().update})).unwrap();
                *ctx.close = true;
            }
            return;
        }
        if !finite || index >= steps.len() || pending.borrow().is_some() {
            return;
        }
        if ctx.presentation.is_none() {
            return;
        }
        let step = &steps[index];
        if step.wait_file.as_ref().is_some_and(|p| !p.exists()) {
            return;
        }
        if !armed {
            if ctx.runner.state().update.root.is_empty() {
                return;
            }
            if !prepared {
                ctx.runner.update(|app| {
                    for (i, text) in step.set.iter().enumerate() {
                        if i < app.inputs.len() {
                            app.inputs[i] = TextInput::new(text.clone());
                        }
                    }
                });
                if let Some(id) = &step.click {
                    let node = find(&ctx.runner.dom().borrow(), ctx.runner.root(), id);
                    let Some(node) = node else {
                        return;
                    };
                    ctx.scroll_into_view(node, ScrollAlign::Nearest);
                }
                prepared = true;
                return;
            }
            if let Some(id) = &step.click {
                let node = find(&ctx.runner.dom().borrow(), ctx.runner.root(), id);
                let Some(node) = node else {
                    return;
                };
                ctx.runner.dispatch_click(node, PointerClick::at((1., 1.)));
            }
            armed = true;
            return;
        }
        let app = ctx.runner.state();
        if app.update.status == "Working…" {
            return;
        }
        if (app.update.status.starts_with("Refused:")
            || app.update.status.starts_with("Unavailable:"))
            && !step
                .status
                .as_ref()
                .is_some_and(|expected| app.update.status.contains(expected))
        {
            failed = true;
            let path = output.join("failure.png");
            let sink = pending.clone();
            *ctx.capture = Some(Box::new(move |capture| {
                *sink.borrow_mut() = Some((path, capture.expect("failure presentation")))
            }));
            return;
        }
        if step
            .nodes
            .is_some_and(|n| app.update.data.as_ref().is_none_or(|d| d.nodes.len() != n))
            || step
                .status
                .as_ref()
                .is_some_and(|s| !app.update.status.contains(s))
            || step
                .access
                .as_ref()
                .is_some_and(|s| app.update.data.as_ref().is_none_or(|d| d.access != *s))
        {
            return;
        }
        if !overview {
            if let Some(node) = find(&ctx.runner.dom().borrow(), ctx.runner.root(), "status") {
                ctx.scroll_into_view(node, ScrollAlign::Start);
            }
            overview = true;
            return;
        }
        let path = step.capture.clone();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let sink = pending.clone();
        *ctx.capture = Some(Box::new(move |capture| {
            *sink.borrow_mut() = Some((path, capture.expect("presented frame")))
        }));
    });
    let running = Arc::new(AtomicBool::new(true));
    let timer_running = running.clone();
    let timer = Rc::new(RefCell::new(None));
    let slot = timer.clone();
    let result = run(
        HostOptions {
            title: format!("Graphshell · Co-op · {mode}"),
            initial_logical_size: (1100., 850.),
            ..Default::default()
        },
        move |_, _, wake| {
            let callback = wake.callback();
            *slot.borrow_mut() = Some(std::thread::spawn(move || {
                while timer_running.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(100));
                    callback();
                }
            }));
            Init{state,logic:view as Logic,sheet:"main{font-family:sans-serif;font-size:15px;color:#192c35;background:#f4f7f6;padding:24px;min-height:100vh}h1{font-size:26px}h2{font-size:19px}p,pre{overflow-wrap:anywhere;white-space:pre-wrap}.graph{display:flex;flex-wrap:wrap;gap:12px}.node{padding:12px;background:#fff;border:1px solid #b5c7c5;min-width:200px;max-width:420px}label{display:flex;gap:12px;margin:8px 0}button{color:#12302c;background:#deebe7;border:1px solid #789b91;padding:7px 12px;margin:4px}[role=textbox]{color:#192c35;background:#fff;border:1px solid #789b91;min-width:440px;min-height:1.2em;padding:6px}[role=textbox]:focus{outline:2px solid #2e7768}#status{padding:10px;background:#e4eeeb}small{font-size:12px}pre{font-size:12px}".into(),fonts:vec![],images:vec![]}
        },
        hooks,
    );
    running.store(false, Ordering::Relaxed);
    let _ = tx.send(Command::Stop);
    if let Some(timer) = timer.borrow_mut().take() {
        let _ = timer.join();
    }
    result.map_err(|e| e.to_string())?;
    if finite && !a.handoff.join("complete.json").exists() {
        return Err("headed acceptance did not complete".into());
    }
    Ok(())
}
