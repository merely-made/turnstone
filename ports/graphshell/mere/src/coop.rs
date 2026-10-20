// SPDX-License-Identifier: MPL-2.0
//! Co-op controls beside the ordinary MereView canvas. The window owns no store.
use super::*;
use cambium::{TextInput, button, lens, text_field_typed};
use graphshell::coop::{self, Contact, Data, Operation, control, network};
use graphshell_endpoint::door::{
    app_admission::{AppId, AppRouteId},
    app_client::AppBrokerClient,
};
use std::{path::PathBuf, time::Duration};

pub(super) enum Command {
    Read(pandect::SessionId),
    Invite {
        graph: pandect::SessionId,
        guest: PathBuf,
        output: PathBuf,
        write: bool,
    },
    Revoke(pandect::SessionId),
    Edit(coop::Edit),
    Reconnect,
}
#[derive(Clone, Debug, serde::Serialize)]
pub(super) struct Update {
    pub acknowledged: u64,
    pub data: Option<Data>,
    pub status: String,
}
pub(super) struct Panel {
    tx: mpsc::Sender<(u64, Command)>,
    rx: mpsc::Receiver<Update>,
    pending: u64,
    pub update: Update,
    pub guest: bool,
    pub inputs: Vec<TextInput>,
}
impl Panel {
    pub fn new(
        domain: String,
        visit: Option<(PathBuf, PathBuf)>,
        wake: cambium_rootstock::HostWake,
    ) -> Self {
        let guest = visit.is_some();
        let (tx, rx) = mpsc::channel();
        let (answers, updates) = mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("co-op runtime");
            let send = |update| {
                let ok = answers.send(update).is_ok();
                wake.wake();
                ok
            };
            if let Some((invitation, contact)) = visit {
                guest_worker(&runtime, invitation, contact, rx, send)
            } else {
                owner_worker(&runtime, domain, rx, send)
            }
        });
        Self {
            tx,
            rx: updates,
            guest,
            pending: 0,
            update: Update {
                acknowledged: 0,
                data: None,
                status: if guest {
                    "Connecting to invited graph"
                } else {
                    "Choose the graph in Sessions, then enable its co-op controls."
                }
                .into(),
            },
            inputs: (0..4).map(|_| TextInput::new("")).collect(),
        }
    }
    pub fn send(&mut self, command: Command) {
        self.pending = self
            .pending
            .checked_add(1)
            .expect("request sequence exhausted");
        self.update.status = if self.tx.send((self.pending, command)).is_ok() {
            "Working…"
        } else {
            "Co-op worker stopped"
        }
        .into();
    }
    pub fn drain(&mut self) -> bool {
        let mut changed = false;
        while let Ok(update) = self.rx.try_recv() {
            let pending = update.acknowledged < self.pending;
            self.update = update;
            if pending && self.update.data.is_some() {
                self.update.status = "Working…".into();
            }
            changed = true;
        }
        changed
    }
}
fn field(i: usize, label: &'static str) -> Child {
    Box::new(el(
        "label",
        (
            label,
            lens(
                move |input: &mut TextInput| {
                    text_field_typed(input)
                        .attr("aria-label", label)
                        .attr("id", format!("coop-field-{i}"))
                },
                move |app: &mut App| &mut app.coop.as_mut().unwrap().inputs[i],
            ),
        ),
    ))
}
pub(super) fn view(app: &App) -> Child {
    let Some(panel) = &app.coop else {
        return Box::new(el("span", ""));
    };
    let graph = app.attached_session();
    let mut children: Vec<Child> = vec![
        Box::new(el(
            "h2",
            if panel.guest {
                "Invited graph"
            } else {
                "Co-op"
            },
        )),
        Box::new(
            el("p", panel.update.status.clone())
                .attr("id", "coop-status")
                .attr("role", "status"),
        ),
    ];
    if !panel.guest {
        children.push(Box::new(
            button("Use this graph for co-op", move |app: &mut App, _| {
                if let Some(graph) = graph
                    && app.attached_session() == Some(graph)
                    && !app.pending
                {
                    app.coop.as_mut().unwrap().send(Command::Read(graph));
                }
            })
            .attr("id", "coop-use"),
        ));
    }
    if let Some(data) = &panel.update.data {
        children.push(Box::new(
            el(
                "p",
                format!(
                    "Graph {} · {} · guest {}",
                    data.graph.as_uuid(),
                    data.access,
                    data.guest.as_deref().unwrap_or("none")
                ),
            )
            .attr("id", "coop-scope"),
        ));
        let same = panel.guest || Some(data.graph) == graph;
        if same && (!panel.guest || data.access == "editor") {
            children.push(field(0, "Address"));
            children.push(field(1, "Title"));
            children.push(Box::new(
                button("Add address", |app: &mut App, _| edit(app, false)).attr("id", "coop-add"),
            ));
            children.push(Box::new(
                button("Save title", |app: &mut App, _| edit(app, true)).attr("id", "coop-title"),
            ));
        }
        if !panel.guest && same {
            let chosen = data.graph;
            children.push(field(2, "Guest contact file"));
            children.push(field(3, "Invitation folder"));
            for (label, write) in [("Invite editor", true), ("Invite reader", false)] {
                children.push(Box::new(button(label, move |app: &mut App, _| {
                    if app.attached_session() != Some(chosen) || app.pending {
                        return;
                    }
                    let p = app.coop.as_mut().unwrap();
                    let graph = p.update.data.as_ref().unwrap().graph;
                    p.send(Command::Invite {
                        graph,
                        guest: p.inputs[2].text().into(),
                        output: p.inputs[3].text().into(),
                        write,
                    });
                })));
            }
            children.push(Box::new(
                button("End guest access", move |app: &mut App, _| {
                    if app.attached_session() != Some(chosen) || app.pending {
                        return;
                    }
                    let p = app.coop.as_mut().unwrap();
                    p.send(Command::Revoke(p.update.data.as_ref().unwrap().graph));
                })
                .attr("id", "coop-revoke"),
            ));
            children.push(Box::new(
                button("Export current invitation", move |app: &mut App, _| {
                    if app.attached_session() != Some(chosen) || app.pending {
                        return;
                    }
                    let p = app.coop.as_mut().unwrap();
                    let graph = p.update.data.as_ref().unwrap().graph;
                    p.send(Command::Invite {
                        graph,
                        guest: PathBuf::new(),
                        output: p.inputs[3].text().into(),
                        write: false,
                    });
                })
                .attr("id", "coop-export"),
            ));
        }
        let authors = data
            .authors
            .iter()
            .rev()
            .take(3)
            .map(|a| format!("{} via {}", a.id, a.via.as_deref().unwrap_or("")))
            .collect::<Vec<_>>()
            .join("; ");
        children.push(Box::new(
            el("p", format!("Saved activity: {authors}")).attr("id", "coop-authors"),
        ));
    }
    if panel.guest {
        children.push(Box::new(
            button("Reconnect", |app: &mut App, _| {
                app.coop.as_mut().unwrap().send(Command::Reconnect)
            })
            .attr("id", "coop-reconnect"),
        ));
    }
    Box::new(
        el("section", children)
            .attr("class", "coop-panel")
            .attr("aria-label", "Graph co-op"),
    )
}
fn edit(app: &mut App, rename: bool) {
    let Some(panel) = app.coop.as_mut() else {
        return;
    };
    let Some(data) = panel.update.data.as_ref() else {
        return;
    };
    if !panel.guest
        && Some(data.graph) != app.model.sessions.iter().find(|s| s.attached).map(|s| s.id)
    {
        return;
    }
    let title = panel.inputs[1].text().trim().to_string();
    let operation = if rename {
        let Some(id) = app
            .selected
            .as_deref()
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
        else {
            panel.update.status = "Select a graph node first".into();
            return;
        };
        Operation::Rename { id, title }
    } else {
        Operation::Add {
            id: uuid::Uuid::new_v4(),
            address: panel.inputs[0].text().trim().into(),
            title,
        }
    };
    panel.send(Command::Edit(data.edit(operation)));
}
async fn owner_call(
    client: &mut AppBrokerClient,
    session: &chirograph::ProjectionRequest,
    command: control::Request,
    observed: &mut (scenotime::SceneEpoch, scenotime::Revision),
) -> Result<control::Reply, String> {
    let result = client
        .request_body(chirograph::CarrierRequestBody::Intent(
            chirograph::IntentInvocation {
                session: session.session.clone(),
                target: sceno::InstanceId(0),
                observed_epoch: observed.0,
                observed_revision: observed.1,
                intent: control::INTENT.into(),
                payload: serde_json::to_vec(&command).map_err(|e| e.to_string())?,
            },
        ))
        .await
        .map_err(|e| e.to_string())?;
    match result {
        chirograph::CarrierResponseBody::Intent(chirograph::IntentResult::Accepted) => {},
        chirograph::CarrierResponseBody::Intent(chirograph::IntentResult::Rejected { reason }) => {
            return Err(reason);
        },
        _ => return Err("unexpected co-op acknowledgement".into()),
    }
    let snapshot = client
        .snapshot(session.clone())
        .await
        .map_err(|e| e.to_string())?;
    *observed = (snapshot.scene.epoch, snapshot.scene.revision);
    let offer = snapshot
        .presentation
        .offers
        .values()
        .flatten()
        .next()
        .ok_or("missing co-op reply")?;
    let bytes = client
        .resource(snapshot.session, offer.resource)
        .await
        .map_err(|e| e.to_string())?;
    let card: chirograph::PortableCardV1 =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let value = card
        .values
        .iter()
        .find(|v| v.label == control::INTENT)
        .ok_or("missing owner reply")?;
    serde_json::from_str(&value.value).map_err(|e| e.to_string())
}
fn owner_worker(
    runtime: &tokio::runtime::Runtime,
    domain: String,
    rx: mpsc::Receiver<(u64, Command)>,
    send: impl Fn(Update) -> bool,
) {
    let mut connection = None;
    while let Ok((acknowledged, command)) = rx.recv() {
        let result = runtime.block_on(async {
            if connection.is_none() {
                let mut client = AppBrokerClient::open_route(
                    AppId::new("graphshell"),
                    AppRouteId::new(format!("coop/{domain}")).map_err(|e| e.to_string())?,
                )
                .await
                .map_err(|e| e.to_string())?;
                let opened = client.open_session().await.map_err(|e| e.to_string())?;
                let request = opened
                    .descriptor
                    .projections
                    .first()
                    .ok_or("no owner controls")?
                    .request
                    .clone();
                connection = Some((
                    client,
                    request,
                    (scenotime::SceneEpoch(1), scenotime::Revision(0)),
                ));
            }
            let (client, session, observed) = connection.as_mut().unwrap();
            let mut export = None;
            let request = match command {
                Command::Read(graph) => control::Request::Read { graph },
                Command::Revoke(graph) => control::Request::Revoke { graph },
                Command::Edit(edit) => control::Request::Edit(edit),
                Command::Invite {
                    graph,
                    guest,
                    output,
                    write,
                } => {
                    export = Some(output);
                    if guest.as_os_str().is_empty() {
                        control::Request::Read { graph }
                    } else {
                        control::Request::Invite {
                            graph,
                            guest: network::read_json::<Contact>(&guest)?,
                            write,
                        }
                    }
                },
                Command::Reconnect => return Err("select this graph again".into()),
            };
            let reply = owner_call(client, session, request, observed).await?;
            if let Some(folder) = export {
                if folder.as_os_str().is_empty() {
                    return Err("Choose an invitation folder".into());
                }
                std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
                let invitation = reply
                    .invitation
                    .as_ref()
                    .ok_or("no invitation for this graph")?;
                network::write_json(&folder.join("contact.json"), &reply.contact)?;
                network::write_json(&folder.join("invitation.json"), invitation)?;
            }
            Ok::<_, String>(reply.data)
        });
        let update = match result {
            Ok(data) => Update {
                acknowledged,
                data: Some(data),
                status: "Saved · owner acknowledged".into(),
            },
            Err(reason) => {
                connection = None;
                Update {
                    acknowledged,
                    data: None,
                    status: format!("Refused: {reason}"),
                }
            },
        };
        if !send(update) {
            break;
        }
    }
}
fn guest_worker(
    runtime: &tokio::runtime::Runtime,
    invitation: PathBuf,
    contact: PathBuf,
    rx: mpsc::Receiver<(u64, Command)>,
    send: impl Fn(Update) -> bool,
) {
    let identity = match network::identity() {
        Ok(i) => i,
        Err(e) => {
            send(Update {
                acknowledged: 0,
                data: None,
                status: e,
            });
            return;
        },
    };
    let mut guest = None;
    let mut reconnect = true;
    let mut last = None;
    let mut acknowledged = 0;
    let mut status = "Connecting to invited graph".to_string();
    loop {
        if reconnect {
            reconnect = false;
            match network::read_json(&invitation).and_then(|i| {
                network::read_json(&contact)
                    .and_then(|c| network::Guest::connect(runtime, &*identity, i, c))
            }) {
                Ok(g) => {
                    guest = Some(g);
                    status = "Connected to invited graph".into();
                },
                Err(e) => {
                    guest = None;
                    status = format!("Unavailable: {e}");
                },
            }
        }
        let command = match rx.recv_timeout(Duration::from_millis(200)) {
            Ok((sequence, c)) => {
                acknowledged = sequence;
                Some(c)
            },
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(_) => None,
        };
        if identity.is_locked() {
            send(Update {
                acknowledged,
                data: None,
                status: "Identity locked".into(),
            });
            break;
        }
        let outcome = match (&mut guest, command) {
            (_, Some(Command::Reconnect)) => {
                guest = None;
                reconnect = true;
                continue;
            },
            (Some(g), Some(Command::Edit(edit))) => g.edit(edit).map(|_| {
                status = "Saved · owner acknowledged".into();
            }),
            (Some(g), _) => g.poll(),
            _ => Ok(()),
        };
        if let Err(e) = outcome {
            status = if guest.as_ref().is_some_and(|g| g.data.is_some()) {
                format!("Save refused: {e}")
            } else {
                format!("Unavailable: {e}")
            };
        }
        let update = Update {
            acknowledged,
            data: guest.as_ref().and_then(|g| g.data.clone()),
            status: status.clone(),
        };
        let encoded = serde_json::to_string(&update).unwrap();
        if last.as_ref() != Some(&encoded) {
            last = Some(encoded);
            if !send(update) {
                break;
            }
        }
    }
}
pub(super) fn guest_reading(data: &Data) -> (MereViewModel, HashMap<String, PortableCardV1>) {
    let model = MereViewModel {
        title: "Invited graph".into(),
        status: ViewStatus::Ready,
        graph: mere_view::GraphModel {
            nodes: data
                .nodes
                .iter()
                .map(|n| mere_view::NodeEntry {
                    key: n.id.to_string(),
                    label: if n.title.is_empty() {
                        n.address.clone()
                    } else {
                        n.title.clone()
                    },
                    state: mere_view::NodeState::Available,
                })
                .collect(),
            relations: data
                .relations
                .iter()
                .enumerate()
                .map(|(i, r)| mere_view::RelationEntry {
                    key: format!("{}>{}:{}:{i}", r.from, r.to, r.kind),
                    from: r.from.to_string(),
                    to: r.to.to_string(),
                    provenance: mere_view::Provenance::Other(
                        r.kind
                            .rsplit('#')
                            .next()
                            .unwrap_or(&r.kind)
                            .replace('.', " "),
                    ),
                })
                .collect(),
        },
        ..Default::default()
    };
    let cards = data
        .nodes
        .iter()
        .map(|n| {
            (
                n.id.to_string(),
                PortableCardV1 {
                    title: n.title.clone(),
                    values: vec![chirograph::CardValueV1 {
                        label: "Address".into(),
                        value: n.address.clone(),
                    }],
                    badges: vec![],
                    media: vec![],
                },
            )
        })
        .collect();
    (model, cards)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_old_background_reading_cannot_acknowledge_a_new_save() {
        let (tx, commands) = mpsc::channel();
        let (answers, rx) = mpsc::channel();
        let data = Data {
            graph: pandect::SessionId(uuid::Uuid::new_v4()),
            owner: "owner".into(),
            guest: None,
            access: "uninvited".into(),
            epoch: scenotime::SceneEpoch(1),
            revision: scenotime::Revision(0),
            nodes: vec![],
            relations: vec![],
            authors: vec![],
        };
        let initial = Update {
            acknowledged: 0,
            data: Some(data.clone()),
            status: "Ready".into(),
        };
        let mut panel = Panel {
            tx,
            rx,
            pending: 0,
            update: initial,
            guest: false,
            inputs: vec![],
        };
        panel.send(Command::Edit(data.edit(Operation::Add {
            id: uuid::Uuid::new_v4(),
            address: "https://save.test/".into(),
            title: "Source".into(),
        })));
        let (sequence, _) = commands.recv().unwrap();
        assert_eq!(panel.update.status, "Working…");
        answers
            .send(Update {
                acknowledged: sequence - 1,
                data: Some(data.clone()),
                status: "Saved · owner acknowledged".into(),
            })
            .unwrap();
        panel.drain();
        assert_eq!(
            panel.update.status, "Working…",
            "a prior reading is not the current save acknowledgement"
        );
        answers
            .send(Update {
                acknowledged: sequence,
                data: Some(data),
                status: "Saved · owner acknowledged".into(),
            })
            .unwrap();
        panel.drain();
        assert_eq!(panel.update.status, "Saved · owner acknowledged");
    }
}
