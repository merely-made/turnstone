// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Native Graphshell mere viewer over the admitted resident route. This host
//! owns presentation only; its worker never opens persona files.
use cambium::{AnyView, GenetCtx, GenetElement, el};
use cambium_genet_winit_host::{AppCtx, HostOptions, Init, inert_hooks, run};
use chirograph::PortableCardV1;
mod connection;
mod coop;
use connection::Connection;
#[cfg(test)]
mod app_tests;
use mere_view::{
    HostAction, MereView, MereViewModel, MereViewRequest, MereViewState, StatusNote, ViewStatus,
};
use mesquite::{LaneConfig, Product};
use sprigging::ColorF;
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::mpsc};
use taproot::ProbeSnapshot;
const LEAF: u64 = 0x6d65_7265;
const CSS: &str = ".coop-panel{height:245px;overflow:auto;padding:8px;border-top:1px solid #6d7783}.coop-panel h2,.coop-panel p{margin:3px;overflow-wrap:anywhere}.coop-panel label{display:inline-flex;gap:4px;margin:3px}.coop-panel input{width:220px} :root{background:#16191d;color:#e8e6e1;font-family:sans-serif;font-size:14px} main{display:flex;flex-direction:column;height:100%} h1{font-size:18px;margin:8px} button{padding:4px;margin:2px;background:#22262c;color:#e8e6e1;border:1px solid #6d7783} .mere-view-session,.mere-view-archive{padding:6px;border-bottom:1px solid #353a42} .mere-view-archive-steps{gap:3px} .mere-view-session-state{font-size:12px;color:#abb4bd} .mere-view-session[data-attached=true]{color:#8cb4de} button[aria-pressed=true]{border-color:#8cb4de} .mere-record{height:180px;overflow:auto;padding:8px;border-top:1px solid #6d7783} .mere-record p{margin:3px;overflow-wrap:anywhere} .mere-view-notice{color:#f0b35a}";
struct Reading {
    model: MereViewModel,
    cards: HashMap<String, PortableCardV1>,
    graph_epoch: u64,
    saved_focus: Option<String>,
    read_only: bool,
    selected: Option<String>,
    view_saved: bool,
}
struct PendingActivation {
    epoch: u64,
    session: Option<pandect::SessionId>,
    key: String,
}
struct App {
    coop: Option<coop::Panel>,
    model: MereViewModel,
    view: MereViewState,
    size: (u32, u32),
    selected: Option<String>,
    cards: HashMap<String, PortableCardV1>,
    sender: mpsc::Sender<MereViewRequest>,
    receiver: mpsc::Receiver<Result<Reading, String>>,
    pending: bool,
    queued_activation: Option<PendingActivation>,
    events: Vec<String>,
    graph_epoch: u64,
    saved_focus: Option<String>,
    view_saved: bool,
    read_only: bool,
}
type Child = Box<dyn AnyView<App, (), GenetCtx, GenetElement>>;
type Logic = fn(&App) -> Child;
impl App {
    fn request(&mut self, request: MereViewRequest) {
        if self.coop.as_ref().is_some_and(|panel| panel.guest) {
            match request {
                MereViewRequest::Activate(key) if self.cards.contains_key(&key) => {
                    self.selected = Some(key);
                },
                MereViewRequest::Host(_) => {
                    self.coop.as_mut().unwrap().send(coop::Command::Reconnect)
                },
                MereViewRequest::Layout(layout) => {
                    self.model.layout = layout;
                    self.relayout();
                },
                _ => {},
            }
            return;
        }
        if let MereViewRequest::Activate(key) = &request {
            if self.cards.contains_key(key) {
                self.selected = Some(key.clone());
                self.view_saved = false;
                self.events.push(format!("selected {key}"));
                if self.pending {
                    self.queued_activation = Some(PendingActivation {
                        epoch: self.graph_epoch,
                        session: self.attached_session(),
                        key: key.clone(),
                    });
                    return;
                }
            }
        }
        if self.pending {
            return;
        }
        self.pending = true;
        if self.sender.send(request).is_err() {
            self.pending = false;
            self.model.notice = Some("The resident worker stopped.".into());
        }
    }
    fn drain(&mut self) {
        if let Some(panel) = self.coop.as_mut() {
            if panel.drain() && panel.guest {
                if let Some(data) = &panel.update.data {
                    let (model, cards) = coop::guest_reading(data);
                    self.model = model;
                    self.cards = cards;
                    self.graph_epoch = data.epoch.0;
                    self.read_only = data.access != "editor";
                    if self
                        .selected
                        .as_ref()
                        .is_some_and(|key| !self.cards.contains_key(key))
                    {
                        self.selected = None;
                    }
                } else {
                    self.model = MereViewModel {
                        status: ViewStatus::Unavailable(StatusNote {
                            message: panel.update.status.clone(),
                            action: None,
                        }),
                        ..Default::default()
                    };
                    self.cards.clear();
                    self.selected = None;
                    self.graph_epoch = 0;
                }
                self.pending = false;
                self.relayout();
            }
        }
        while let Ok(answer) = self.receiver.try_recv() {
            self.pending = false;
            match answer {
                Ok(reading) => {
                    self.view_saved = reading.view_saved;
                    self.read_only = reading.read_only;
                    self.saved_focus = reading.saved_focus;
                    self.selected = reading.selected;
                    self.model = reading.model;
                    self.cards = reading.cards;
                    self.graph_epoch = reading.graph_epoch;
                    self.events.push("resident refreshed".into());
                    let attached = self.attached_session();
                    if let Some(panel) = self.coop.as_mut() {
                        if let Some(data) = &panel.update.data {
                            if Some(data.graph) == attached && panel.update.status != "Working…" {
                                panel.send(coop::Command::Read(data.graph));
                            } else if Some(data.graph) != attached {
                                panel.update.data = None;
                                panel.update.status =
                                    "Choose this graph explicitly before sharing it.".into();
                            }
                        }
                    }
                },
                Err(reason) => {
                    self.queued_activation = None;
                    self.saved_focus = None;
                    self.view_saved = false;
                    self.read_only = false;
                    self.graph_epoch = 0;
                    self.model.layout.clear();
                    self.model.layouts.clear();
                    self.model.actions.clear();
                    self.model.notice = None;
                    self.cards.clear();
                    self.selected = None;
                    self.model.graph = Default::default();
                    self.model.sessions.clear();
                    self.model.can_mint = false;
                    self.model.archive = None;
                    self.model.status = ViewStatus::Unavailable(StatusNote {
                        message: reason,
                        action: Some(HostAction::new("refresh", "Retry")),
                    });
                    self.events.push("resident unavailable".into());
                },
            }
            // Keep a click responsive while the worker finishes its current
            // operation, then let the shared controller handle the latest one.
            // A different session epoch cannot inherit that pending selection.
            if let Some(activation) = self.queued_activation.take() {
                if activation.epoch == self.graph_epoch
                    && activation.session == self.attached_session()
                    && self.cards.contains_key(&activation.key)
                {
                    self.request(MereViewRequest::Activate(activation.key));
                }
            }
            self.relayout();
        }
    }
    fn attached_session(&self) -> Option<pandect::SessionId> {
        self.model
            .sessions
            .iter()
            .find(|entry| entry.attached)
            .map(|entry| entry.id)
    }
    fn relayout(&mut self) {
        self.view.lay_out(
            &self.model,
            self.size.0,
            self.size
                .1
                .saturating_sub(if self.coop.is_some() { 470 } else { 220 }),
        );
    }
    fn view_of(&self) -> MereView<'_> {
        MereView {
            model: &self.model,
            state: &self.view,
            leaf_key: LEAF,
            width: self.size.0,
            height: self
                .size
                .1
                .saturating_sub(if self.coop.is_some() { 470 } else { 220 }),
        }
    }
}
fn root(app: &App) -> Child {
    let record = app.selected.as_ref().and_then(|key| app.cards.get(key));
    let title = record
        .map(|card| card.title.clone())
        .unwrap_or_else(|| "Select a record to read its details.".into());
    let values: Vec<Child> = record
        .into_iter()
        .flat_map(|card| card.values.iter())
        .map(|value| {
            Box::new(el::<_, App, ()>(
                "p",
                format!("{}: {}", value.label, value.value),
            )) as Child
        })
        .collect();
    Box::new(el(
        "main",
        (
            el("h1", "Graphshell"),
            mere_view::mere_view(
                app.view_of(),
                |app: &mut App, event| app.view.apply(event),
                |app: &mut App, request| app.request(request),
            ),
            coop::view(app),
            el("section", (el("h2", title), values))
                .attr("class", "mere-record")
                .attr("aria-label", "Selected record"),
            el(
                "p",
                if app.view_saved {
                    "View saved"
                } else if app.selected.is_some() {
                    "View not saved"
                } else {
                    ""
                },
            )
            .attr("role", "status"),
        ),
    ))
}
struct LaneProduct {
    sheet: String,
    awaiting: std::collections::BTreeMap<String, String>,
}
impl Product for LaneProduct {
    type State = App;
    type Logic = Logic;
    type View = Child;
    const KIND: &'static str = "graphshell-mere";
    const SURFACE: &'static str = "app";
    const LOG_PREFIX: &'static str = "graphshell-mere";
    fn sheet(&self) -> &str {
        &self.sheet
    }
    fn snapshot(&self, ctx: &AppCtx<'_, App, Logic, Child>, _: usize, _dt: f32) -> ProbeSnapshot {
        let app = ctx.runner.state();
        ProbeSnapshot::default()
            .with_field("pid", std::process::id().to_string())
            .with_field(
                "coop",
                app.coop
                    .as_ref()
                    .map(|p| serde_json::to_string(&p.update).unwrap())
                    .unwrap_or_default(),
            )
            .with_field(
                "coop-nodes",
                app.coop
                    .as_ref()
                    .and_then(|p| p.update.data.as_ref())
                    .map(|d| d.nodes.len().to_string())
                    .unwrap_or_default(),
            )
            .with_field(
                "coop-access",
                app.coop
                    .as_ref()
                    .and_then(|p| p.update.data.as_ref())
                    .map(|d| d.access.clone())
                    .unwrap_or_default(),
            )
            .with_field(
                "coop-graph",
                app.coop
                    .as_ref()
                    .and_then(|p| p.update.data.as_ref())
                    .map(|d| d.graph.as_uuid().to_string())
                    .unwrap_or_default(),
            )
            .with_field(
                "coop-available",
                app.coop
                    .as_ref()
                    .is_some_and(|p| p.update.data.is_some())
                    .to_string(),
            )
            .with_field(
                "coop-status",
                app.coop
                    .as_ref()
                    .map(|p| p.update.status.clone())
                    .unwrap_or_default(),
            )
            .with_field("status", app.model.status.token())
            .with_field("sessions", app.model.sessions.len().to_string())
            .with_field("nodes", app.model.graph.nodes.len().to_string())
            .with_field("relations", app.model.graph.relations.len().to_string())
            .with_field(
                "graph-labels",
                app.model
                    .graph
                    .nodes
                    .iter()
                    .map(|node| node.label.as_str())
                    .collect::<Vec<_>>()
                    .join("; "),
            )
            .with_field(
                "archives",
                app.model
                    .archive
                    .as_ref()
                    .map_or(0, |archive| archive.entries.len())
                    .to_string(),
            )
            .with_field("epoch", app.graph_epoch.to_string())
            .with_field("layout", app.model.layout.clone())
            .with_field("read-only", app.read_only.to_string())
            .with_field(
                "attached",
                app.model
                    .sessions
                    .iter()
                    .find(|entry| entry.attached)
                    .map(|entry| entry.id.as_uuid().to_string())
                    .unwrap_or_default(),
            )
            .with_field("selected", app.selected.clone().unwrap_or_default())
            .with_field("saved-focus", app.saved_focus.clone().unwrap_or_default())
            .with_field("view-saved", app.view_saved.to_string())
            .with_field(
                "record",
                app.selected
                    .as_ref()
                    .and_then(|key| app.cards.get(key))
                    .map(|card| card.title.clone())
                    .unwrap_or_default(),
            )
            .with_field("notice", app.model.notice.clone().unwrap_or_default())
            .with_field(
                "record-data",
                app.selected
                    .as_ref()
                    .and_then(|key| app.cards.get(key))
                    .map(|card| {
                        card.values
                            .iter()
                            .map(|field| format!("{}: {}", field.label, field.value))
                            .collect::<Vec<_>>()
                            .join("; ")
                    })
                    .unwrap_or_default(),
            )
    }
    fn drain_events(&mut self, ctx: &mut AppCtx<'_, App, Logic, Child>) -> Vec<String> {
        let mut result = Vec::new();
        ctx.runner
            .update(|app| result = std::mem::take(&mut app.events));
        result
    }
    fn act(&mut self, ctx: &mut AppCtx<'_, App, Logic, Child>, label: &str) -> bool {
        if label == "refresh" {
            ctx.runner
                .update(|app| app.request(MereViewRequest::Host("refresh".into())));
            true
        } else {
            false
        }
    }
    fn busy_mut(&mut self, ctx: &mut AppCtx<'_, App, Logic, Child>, _: bool) -> Option<bool> {
        let snapshot = self.snapshot(ctx, 0, 1.0);
        let waiting = self.awaiting.iter().any(|(field, value)| {
            if field == "file" {
                return !std::path::Path::new(value).exists();
            }
            snapshot.fields.get(field).is_none_or(|actual| {
                if let Some(part) = value.strip_prefix('~') {
                    !actual.contains(part)
                } else {
                    actual != value
                }
            })
        });
        Some(
            ctx.runner.state().pending
                || waiting
                || ctx
                    .runner
                    .state()
                    .coop
                    .as_ref()
                    .is_some_and(|p| p.update.status == "Working…"),
        )
    }
    fn app_step(
        &mut self,
        ctx: &mut AppCtx<'_, App, Logic, Child>,
        _: mesquite::Checkpoints<'_>,
        line: &str,
    ) -> Result<(), String> {
        if let Some(json) = line.strip_prefix("await ") {
            self.awaiting = serde_json::from_str(json).map_err(|e| e.to_string())?;
            return Ok(());
        }
        if let Some(rest) = line.strip_prefix("coop-input ") {
            let (index, text) = rest.split_once(' ').ok_or("coop-input INDEX TEXT")?;
            let index: usize = index.parse().map_err(|_| "input index")?;
            if index >= 4 {
                return Err("input index".into());
            }
            ctx.runner.update(|app| {
                app.coop.as_mut().unwrap().inputs[index] = cambium::TextInput::new(text)
            });
            return Ok(());
        }
        Err(format!("unknown step: {line}"))
    }
}
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some("--contact") {
        assert!(args.len() == 2, "graphshell_mere --contact FILE");
        let identity = graphshell::coop::network::identity().expect("unlock identity");
        graphshell::coop::network::write_json(
            std::path::Path::new(&args[1]),
            &graphshell::coop::Contact::create(&*identity).expect("contact attestation"),
        )
        .expect("save public contact");
        return;
    }
    let visit = if args.first().map(String::as_str) == Some("--invitation") {
        assert!(
            args.len() == 4 && args[2] == "--contact",
            "graphshell_mere --invitation FILE --contact FILE"
        );
        Some((
            std::path::PathBuf::from(&args[1]),
            std::path::PathBuf::from(&args[3]),
        ))
    } else {
        None
    };
    let domain = if visit.is_some() {
        "Invited graph".to_string()
    } else {
        args.first().cloned().unwrap_or_else(|| "notes".into())
    };
    assert!(
        visit.is_some()
            || domain
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "Expected a domain identifier"
    );
    let lane = LaneConfig::from_env("GRAPHSHELL_MERE").map(|config| {
        let product = LaneProduct {
            awaiting: Default::default(),
            sheet: format!(
                "{}\n{}\n{}",
                cambium::GRAPH_CANVAS_SWATCH_CSS,
                mere_view::MERE_VIEW_CSS,
                CSS
            ),
        };
        let lane = if std::env::var_os("GRAPHSHELL_MERE_JSON_RECEIPT").is_some() {
            let scenario = taproot::Scenario::parse(
                &std::fs::read_to_string(&config.scenario).expect("scenario file"),
            )
            .expect("scenario");
            mesquite::Lane::new(
                product,
                Some(scenario),
                config.receipt_path(),
                None,
                Rc::new(std::cell::Cell::new(0)),
            )
        } else {
            mesquite::Lane::from_config(config, product, cambium_genet_winit_host::read_file)
                .expect("scenario")
        };
        lane
            // A headed acceptance lane may need time to establish real foreground
            // visibility. Keep the same finite allowance as Cleromancy; ordinary
            // windows have no lane and the default scenario deadline stays 10s.
            .with_presentation_timeout(std::time::Duration::from_secs(
                std::env::var("GRAPHSHELL_MERE_PRESENTATION_TIMEOUT_SECS")
                    .ok()
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(10)
                    .clamp(10, 60),
            ))
    });
    let lane = Rc::new(RefCell::new(lane));
    let mut hooks = inert_hooks::<App, Logic, Child>();
    hooks.after_wake = Box::new(|ctx| ctx.runner.update(|app| app.drain()));
    hooks.frame = Box::new(|ctx| {
        let size = (
            ctx.logical_size.0.round() as u32,
            ctx.logical_size.1.round() as u32,
        );
        if size != ctx.runner.state().size {
            ctx.runner.update(|app| {
                app.size = size;
                app.relayout();
            });
        }
        ctx.leaves.insert(
            LEAF,
            Box::new(ctx.runner.state().view_of().paint_leaf(|_| ColorF {
                r: 0.3,
                g: 0.55,
                b: 0.75,
                a: 1.0,
            })),
        );
        false
    });
    hooks.after_frame = Box::new(move |ctx| {
        if let Some(lane) = lane.borrow_mut().as_mut() {
            lane.after_frame(ctx);
        }
    });
    run(
        HostOptions {
            title: format!("Graphshell · {domain}"),
            initial_logical_size: (1100., 800.),
            size_env: Some((
                "GRAPHSHELL_MERE_WIDTH".into(),
                "GRAPHSHELL_MERE_HEIGHT".into(),
            )),
            ..Default::default()
        },
        move |_, _, wake| {
            let (sender, receiver) = if visit.is_some() {
                let (sender, _) = mpsc::channel();
                let (_, receiver) = mpsc::channel();
                (sender, receiver)
            } else {
                spawn_worker(domain.clone(), wake.clone())
            };
            let panel = coop::Panel::new(domain, visit, wake.clone());
            let app = App {
                coop: Some(panel),
                model: MereViewModel {
                    title: "Connecting to resident".into(),
                    status: ViewStatus::Building(StatusNote {
                        message: "Reading the shared mere.".into(),
                        action: None,
                    }),
                    ..Default::default()
                },
                view: MereViewState::default(),
                size: (1100, 800),
                selected: None,
                cards: HashMap::new(),
                sender,
                receiver,
                pending: true,
                queued_activation: None,
                events: Vec::new(),
                graph_epoch: 0,
                saved_focus: None,
                view_saved: false,
                read_only: false,
            };
            Init {
                state: app,
                logic: root as Logic,
                sheet: format!(
                    "{}\n{}\n{}",
                    cambium::GRAPH_CANVAS_SWATCH_CSS,
                    mere_view::MERE_VIEW_CSS,
                    CSS
                ),
                fonts: Vec::new(),
                images: Vec::new(),
            }
        },
        hooks,
    )
    .expect("event loop");
}

fn spawn_worker(
    domain: String,
    wake: cambium_rootstock::HostWake,
) -> (
    mpsc::Sender<MereViewRequest>,
    mpsc::Receiver<Result<Reading, String>>,
) {
    let (sender, requests) = mpsc::channel();
    let (answers, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("resident worker runtime");
        runtime.block_on(async move {
            let deadline = std::time::Duration::from_secs(5);
            let mut connection = tokio::time::timeout(deadline, Connection::open(&domain))
                .await
                .unwrap_or_else(|_| Err("The resident connection timed out.".into()));
            let first = match &mut connection {
                Ok(connection) => tokio::time::timeout(deadline, connection.read())
                    .await
                    .unwrap_or_else(|_| Err("The resident reading timed out.".into())),
                Err(reason) => Err(reason.clone()),
            };
            if let Err(reason) = &first {
                connection = Err(reason.clone());
            }
            if answers.send(first).is_err() {
                return;
            }
            wake.wake();
            loop {
                let request = match requests.recv_timeout(std::time::Duration::from_millis(250)) {
                    Ok(request) => Some(request),
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                };
                if request.is_none() {
                    let notice = match &mut connection {
                        Ok(connection) => tokio::time::timeout(deadline, connection.take_notice())
                            .await
                            .map_err(|_| "The resident notice read timed out.".to_string())
                            .and_then(|notice| notice),
                        Err(_) => continue,
                    };
                    match notice {
                        Ok(false) => continue,
                        Ok(true) => {},
                        Err(reason) => {
                            connection = Err(reason.clone());
                            if answers.send(Err(reason)).is_err() {
                                break;
                            }
                            wake.wake();
                            continue;
                        },
                    }
                } else if connection.is_err() {
                    connection = tokio::time::timeout(deadline, Connection::open(&domain))
                        .await
                        .unwrap_or_else(|_| Err("The resident connection timed out.".into()));
                }
                let answer = match &mut connection {
                    Ok(connection) => tokio::time::timeout(deadline, async {
                        if let Some(request) = request {
                            connection.take(request);
                        }
                        connection.read().await
                    })
                    .await
                    .unwrap_or_else(|_| Err("The resident operation timed out.".into())),
                    Err(reason) => Err(reason.clone()),
                };
                if let Err(reason) = &answer {
                    connection = Err(reason.clone());
                }
                if answers.send(answer).is_err() {
                    break;
                }
                wake.wake();
            }
        });
    });
    (sender, receiver)
}
