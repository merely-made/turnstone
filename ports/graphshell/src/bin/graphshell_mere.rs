// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Native Graphshell mere viewer over the admitted resident route. This host
//! owns presentation only; its worker never opens persona files.
use cambium::{AnyView, GenetCtx, GenetElement, el};
use cambium_genet_winit_host::{AppCtx, HostOptions, Init, inert_hooks, run};
use chirograph::{
    CarrierRequestBody, CarrierResponseBody, ContentHash, PortableCardV1, ProjectionSnapshot,
};
use graphshell::{
    mere_route,
    native::{
        app_admission::{AppId, AppRouteId},
        app_client::AppBrokerClient,
    },
};
use mere_view::{
    HostAction, MereView, MereViewModel, MereViewRequest, MereViewState, StatusNote, ViewStatus,
};
use mesquite::{LaneConfig, Product};
use sprigging::ColorF;
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::mpsc};
use taproot::ProbeSnapshot;
const LEAF: u64 = 0x6d65_7265;
const CSS: &str = ":root{background:#16191d;color:#e8e6e1;font-family:sans-serif;font-size:14px} main{display:flex;flex-direction:column;height:100%} h1{font-size:18px;margin:8px} button{padding:4px;margin:2px;background:#22262c;color:#e8e6e1;border:1px solid #6d7783} .mere-view-session,.mere-view-archive{padding:6px;border-bottom:1px solid #353a42} .mere-view-archive-steps{gap:3px} .mere-view-session-state{font-size:12px;color:#abb4bd} .mere-view-session[data-attached=true]{color:#8cb4de} button[aria-pressed=true]{border-color:#8cb4de} .mere-record{height:180px;overflow:auto;padding:8px;border-top:1px solid #6d7783} .mere-record p{margin:3px;overflow-wrap:anywhere} .mere-view-notice{color:#f0b35a}";
struct Reading {
    model: MereViewModel,
    cards: HashMap<String, PortableCardV1>,
    graph_epoch: u64,
    saved_focus: Option<String>,
    read_only: bool,
}
struct Connection {
    client: AppBrokerClient,
    sessions: ProjectionSnapshot,
    graph: ProjectionSnapshot,
    archive: Option<ProjectionSnapshot>,
    cards: HashMap<String, PortableCardV1>,
    saved_view: pandect::ViewIntent,
    read_only: bool,
    requests: (
        chirograph::ProjectionRequest,
        chirograph::ProjectionRequest,
        Option<chirograph::ProjectionRequest>,
    ),
}
impl Connection {
    async fn open(domain: &str) -> Result<Self, String> {
        let mut client = AppBrokerClient::open_route(
            AppId::new("graphshell"),
            AppRouteId::new(format!("mere/{domain}")).map_err(|e| e.to_string())?,
        )
        .await
        .map_err(|e| e.to_string())?;
        let descriptor = client
            .open_session()
            .await
            .map_err(|e| e.to_string())?
            .descriptor;
        let (sessions, graph) = mere_route::requests(&descriptor)
            .ok_or("The route offers no mere sessions and graph.")?;
        let requests = (
            sessions.clone(),
            graph.clone(),
            mere_route::archive_request(&descriptor),
        );
        let sessions = client.snapshot(sessions).await.map_err(|e| e.to_string())?;
        let graph = client.snapshot(graph).await.map_err(|e| e.to_string())?;
        let archive = if let Some(request) = mere_route::archive_request(&descriptor) {
            Some(client.snapshot(request).await.map_err(|e| e.to_string())?)
        } else {
            None
        };
        Ok(Self {
            client,
            sessions,
            graph,
            archive,
            requests,
            cards: HashMap::new(),
            saved_view: Default::default(),
            read_only: false,
        })
    }
    async fn read(&mut self) -> Result<Reading, String> {
        self.sessions = self
            .client
            .snapshot(self.requests.0.clone())
            .await
            .map_err(|e| e.to_string())?;
        self.graph = self
            .client
            .snapshot(self.requests.1.clone())
            .await
            .map_err(|e| e.to_string())?;
        self.archive = if let Some(request) = self.requests.2.clone() {
            Some(
                self.client
                    .snapshot(request)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        let mut bytes: HashMap<ContentHash, Vec<u8>> = HashMap::new();
        for snapshot in [&self.sessions, &self.graph]
            .into_iter()
            .chain(self.archive.iter())
        {
            for request in mere_route::card_requests(snapshot) {
                let content = self
                    .client
                    .resource(request.session, request.resource)
                    .await
                    .map_err(|e| e.to_string())?;
                bytes.insert(request.resource, content);
            }
        }
        let mut model = MereViewModel::default();
        model.actions = vec![HostAction::new("refresh", "Refresh")];
        model.layout = "spectral.default".into();
        model.layouts = vec![
            ("spectral.default".into(), "Spectral".into()),
            ("grid.default".into(), "Grid".into()),
            ("phyllotaxis.default".into(), "Spiral".into()),
        ];
        mere_route::fill(&mut model, &self.sessions, &self.graph, &bytes);
        if let Some(archive) = &self.archive {
            mere_route::fill_archive(&mut model, &self.sessions, archive, &bytes);
        }
        let mut cards = mere_route::node_cards(&self.graph, &bytes);
        if let Some(request) = mere_route::session_graph_request(&self.graph, &bytes) {
            let material = self
                .client
                .resource(request.session, request.resource)
                .await
                .map_err(|e| e.to_string())?;
            let material: graphshell::session_item::SessionGraphV1 =
                serde_json::from_slice(&material).map_err(|e| e.to_string())?;
            let attached = model
                .sessions
                .iter()
                .find(|entry| entry.attached)
                .ok_or("The graph has no attached session.")?
                .id;
            if material.schema != graphshell::session_item::SESSION_GRAPH_SCHEMA
                || material.application != "graphshell"
                || material.session != attached
                || material.epoch != self.graph.scene.epoch
                || material.revision != self.graph.scene.revision
            {
                return Err(
                    "The disclosed records no longer match the selected session; refresh.".into(),
                );
            }
            self.saved_view = material
                .views
                .get("graphshell.mere")
                .cloned()
                .unwrap_or_default();
            if let Some(strategy) = &self.saved_view.strategy {
                model.layout = strategy.clone();
            }
            self.read_only = material.read_only;
            mere_route::add_node_facets(&mut cards, &material.facets)?;
        }
        let saved_focus = self.saved_view.focus.as_ref().and_then(|focus| {
            cards
                .iter()
                .find(|(_, card)| {
                    card.values
                        .iter()
                        .any(|value| value.label == "Address" && &value.value == focus)
                })
                .map(|(key, _)| key.clone())
        });
        self.cards = cards.clone();
        Ok(Reading {
            saved_focus,
            read_only: self.read_only,
            model,
            cards,
            graph_epoch: self.graph.scene.epoch.0,
        })
    }
    async fn take(&mut self, request: &MereViewRequest) -> Result<Option<String>, String> {
        let view_request = match request {
            MereViewRequest::Activate(key) | MereViewRequest::Layout(key) => {
                if self.read_only {
                    return Ok(Some(
                        "Viewing a read-only archive; this view is not saved.".into(),
                    ));
                }
                let target = self
                    .graph
                    .scene
                    .active_items_in_order()
                    .into_iter()
                    .find(|(_, item)| {
                        self.graph.scene.tables.sources[item.source.0 as usize]
                            .as_ref()
                            .is_some_and(|source| {
                                source.adapter == graphshell::session_item::SESSION_ITEM_SOURCE
                            })
                    })
                    .map(|(instance, _)| instance)
                    .ok_or("The owner offers no view action.")?;
                if !self
                    .graph
                    .presentation
                    .offers_for(target)
                    .is_some_and(|offers| {
                        offers.iter().any(|offer| {
                            offer.semantics.actions.iter().any(|action| {
                                action.intent.0 == graphshell::session_item::SET_VIEW_INTENT
                            })
                        })
                    })
                {
                    return Ok(Some("The owner does not offer saving this view.".into()));
                }
                let mut view = self.saved_view.clone();
                if matches!(request, MereViewRequest::Activate(_)) {
                    let address = self
                        .cards
                        .get(key)
                        .and_then(|card| card.values.iter().find(|value| value.label == "Address"))
                        .map(|value| value.value.clone())
                        .ok_or("The selected record is no longer disclosed.")?;
                    view.focus = Some(address);
                    view.strategy =
                        Some(view.strategy.unwrap_or_else(|| "spectral.default".into()));
                } else {
                    view.strategy = Some(key.clone());
                }
                Some(chirograph::IntentInvocation {
                    session: self.graph.session.clone(),
                    target,
                    observed_epoch: self.graph.scene.epoch,
                    observed_revision: self.graph.scene.revision,
                    intent: graphshell::session_item::SET_VIEW_INTENT.into(),
                    payload: serde_json::to_vec(&graphshell::session_item::SetViewV1::new(
                        "graphshell.mere",
                        view,
                    ))
                    .map_err(|e| e.to_string())?,
                })
            },
            _ => None,
        };
        let invocation = view_request
            .or_else(|| mere_route::intent(request, &self.sessions))
            .or_else(|| {
                self.archive.as_ref().and_then(|archive| {
                    mere_route::archive_intent(request, &self.sessions, archive)
                })
            });
        let Some(invocation) = invocation else {
            return Ok(Some(
                "The owner no longer offers that action; refresh the mere.".into(),
            ));
        };
        match self
            .client
            .request_body(CarrierRequestBody::Intent(invocation))
            .await
            .map_err(|e| e.to_string())?
        {
            CarrierResponseBody::Intent(result) => Ok(mere_route::declined(&result)),
            _ => Err("The resident did not answer the action.".into()),
        }
    }
}
struct App {
    model: MereViewModel,
    view: MereViewState,
    size: (u32, u32),
    selected: Option<String>,
    cards: HashMap<String, PortableCardV1>,
    sender: mpsc::Sender<MereViewRequest>,
    receiver: mpsc::Receiver<Result<Reading, String>>,
    pending: bool,
    events: Vec<String>,
    graph_epoch: u64,
    saved_focus: Option<String>,
    saved_layout: String,
    read_only: bool,
}
type Child = Box<dyn AnyView<App, (), GenetCtx, GenetElement>>;
type Logic = fn(&App) -> Child;
impl App {
    fn request(&mut self, request: MereViewRequest) {
        match request {
            MereViewRequest::Activate(key) => {
                self.selected = Some(key.clone());
                self.events.push(format!("selected {key}"));
                if !self.pending && !self.read_only {
                    self.pending = true;
                    if self.sender.send(MereViewRequest::Activate(key)).is_err() {
                        self.pending = false;
                        self.model.notice = Some("The resident worker stopped.".into());
                    }
                }
            },
            MereViewRequest::Layout(layout) => {
                if self.pending {
                    return;
                }
                self.model.layout = layout.clone();
                self.relayout();
                if !self.read_only {
                    self.pending = true;
                    if self.sender.send(MereViewRequest::Layout(layout)).is_err() {
                        self.pending = false;
                        self.model.notice = Some("The resident worker stopped.".into());
                    }
                }
            },
            request => {
                if self.pending {
                    return;
                }
                self.pending = true;
                if self.sender.send(request).is_err() {
                    self.pending = false;
                    self.model.notice = Some("The resident worker stopped.".into());
                }
            },
        }
    }
    fn drain(&mut self) {
        while let Ok(answer) = self.receiver.try_recv() {
            self.pending = false;
            match answer {
                Ok(reading) => {
                    self.saved_layout = reading.model.layout.clone();
                    self.read_only = reading.read_only;
                    self.saved_focus = reading.saved_focus;
                    if self.selected.is_none() {
                        self.selected = self.saved_focus.clone();
                    }
                    self.model = reading.model;
                    self.cards = reading.cards;
                    self.graph_epoch = reading.graph_epoch;
                    if self
                        .selected
                        .as_ref()
                        .is_some_and(|key| !self.cards.contains_key(key))
                    {
                        self.selected = None;
                    }
                    if self.selected.is_none() {
                        self.selected = self
                            .saved_focus
                            .clone()
                            .filter(|key| self.cards.contains_key(key));
                    }
                    self.events.push("resident refreshed".into());
                },
                Err(reason) => {
                    self.saved_focus = None;
                    self.saved_layout.clear();
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
            self.relayout();
        }
    }
    fn relayout(&mut self) {
        self.view
            .lay_out(&self.model, self.size.0, self.size.1.saturating_sub(220));
    }
    fn view_of(&self) -> MereView<'_> {
        MereView {
            model: &self.model,
            state: &self.view,
            leaf_key: LEAF,
            width: self.size.0,
            height: self.size.1.saturating_sub(220),
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
            el("h1", "Graphshell · shared mere"),
            mere_view::mere_view(
                app.view_of(),
                |app: &mut App, event| app.view.apply(event),
                |app: &mut App, request| app.request(request),
            ),
            el("section", (el("h2", title), values))
                .attr("class", "mere-record")
                .attr("aria-label", "Selected record"),
            el(
                "p",
                if app.selected.is_some()
                    && app.selected == app.saved_focus
                    && app.model.layout == app.saved_layout
                {
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
            .with_field("status", app.model.status.token())
            .with_field("sessions", app.model.sessions.len().to_string())
            .with_field("nodes", app.model.graph.nodes.len().to_string())
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
            .with_field(
                "view-saved",
                (app.selected.is_some()
                    && app.saved_focus == app.selected
                    && app.model.layout == app.saved_layout)
                    .to_string(),
            )
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
        Some(ctx.runner.state().pending)
    }
}
fn main() {
    let domain = std::env::args().nth(1).unwrap_or_else(|| "notes".into());
    assert!(
        domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "Expected a domain identifier"
    );
    let lane = LaneConfig::from_env("GRAPHSHELL_MERE").map(|config| {
        mesquite::Lane::from_config(
            config,
            LaneProduct {
                sheet: format!(
                    "{}\n{}\n{}",
                    cambium::GRAPH_CANVAS_SWATCH_CSS,
                    mere_view::MERE_VIEW_CSS,
                    CSS
                ),
            },
            cambium_genet_winit_host::read_file,
        )
        .expect("scenario")
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
            let (sender, receiver) = spawn_worker(domain, wake.clone());
            let app = App {
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
                events: Vec::new(),
                graph_epoch: 0,
                saved_focus: None,
                saved_layout: String::new(),
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
                        Ok(connection) => {
                            tokio::time::timeout(deadline, connection.client.take_notice())
                                .await
                                .map_err(|_| "The resident notice read timed out.".to_string())
                                .and_then(|notice| notice.map_err(|e| e.to_string()))
                        },
                        Err(_) => continue,
                    };
                    match notice {
                        Ok(None) => continue,
                        Ok(Some(_)) => {},
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
                        let notice = match request {
                            Some(MereViewRequest::Host(ref key)) if key == "refresh" => None,
                            Some(ref request) => connection.take(request).await?,
                            None => None,
                        };
                        connection.read().await.map(|mut reading| {
                            reading.model.notice = notice;
                            reading
                        })
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
