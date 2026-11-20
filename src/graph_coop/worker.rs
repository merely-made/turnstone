// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Local owner controls and fielded guest reads over the admitted co-op carrier.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

use cambium::TextInput;
use castellan_client::custody_identity::CustodyIdentity;
use chirograph::PortableCardV1;
use graphshell_door::projection::AppBrokerClient;
use graphshell_host::coop::{self, Contact, Data, control, network};
use mere_view::{MereViewModel, ViewStatus};
use ostiary::app_admission::{AppId, AppRouteId};

fn connect_identity() -> Result<CustodyIdentity, String> {
    CustodyIdentity::connect_declared_timeout(
        crate::identity::CUSTODY_APP,
        crate::identity::app_declaration(),
        Duration::from_secs(10),
    )
    .map_err(|error| format!("Identity pending: {error}"))
}

pub(crate) fn export_contact(path: &Path) -> Result<(), String> {
    let identity = connect_identity()?;
    let contact = Contact::create(&identity, &coop::TERMS)?;
    network::write_json(path, &contact)
}

pub(crate) enum Command {
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
pub(crate) struct Update {
    pub acknowledged: u64,
    pub data: Option<Data>,
    pub status: String,
    /// Read-only carrier observations for connection diagnostics and receipts.
    pub connection: Vec<String>,
}
pub(crate) struct Panel {
    tx: mpsc::Sender<(u64, Command)>,
    rx: mpsc::Receiver<Update>,
    pending: u64,
    pub update: Update,
    pub guest: bool,
    pub inputs: Vec<TextInput>,
}
impl Panel {
    pub fn new(domain: String, visit: Option<(PathBuf, PathBuf)>, wake: armillary::Wake) -> Self {
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
                wake();
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
                connection: vec![],
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
            self.update.acknowledged = self.pending;
            "Co-op worker stopped"
        }
        .into();
    }
    pub fn busy(&self) -> bool {
        self.update.acknowledged < self.pending
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
// A command can include both a custodian attestation and an interactive signature.
const OWNER_DEADLINE: Duration = Duration::from_secs(330);

fn owner_worker(
    runtime: &tokio::runtime::Runtime,
    domain: String,
    rx: mpsc::Receiver<(u64, Command)>,
    send: impl Fn(Update) -> bool,
) {
    let mut connection = None;
    while let Ok((acknowledged, command)) = rx.recv() {
        let operation = async {
            if connection.is_none() {
                let mut client = AppBrokerClient::open_declared_route(
                    AppId::new(crate::identity::CUSTODY_APP),
                    AppRouteId::new(format!("coop/{domain}")).map_err(|e| e.to_string())?,
                    crate::identity::app_declaration(),
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
        };
        let result = runtime.block_on(async {
            tokio::time::timeout(OWNER_DEADLINE, operation)
                .await
                .unwrap_or_else(
                    |_| Err("co-op owner route timed out before acknowledgement".into()),
                )
        });
        let update = match result {
            Ok(data) => Update {
                acknowledged,
                data: Some(data),
                status: "Saved · owner acknowledged".into(),
                connection: vec![],
            },
            Err(reason) => {
                connection = None;
                Update {
                    acknowledged,
                    data: None,
                    status: format!("Refused: {reason}"),
                    connection: vec![],
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
    let mut identity = None;
    let mut guest = None;
    let mut reconnect = true;
    let mut last = None;
    let mut acknowledged = 0;
    let mut status = "Connecting to invited graph".to_string();
    loop {
        if reconnect {
            reconnect = false;
            let connected = (|| {
                let binding = connect_identity()?;
                let invitation = network::read_json(&invitation)?;
                let contact = network::read_json(&contact)?;
                let carrier = network::Guest::connect(runtime, &binding, invitation, contact)?;
                Ok::<_, String>((binding, carrier))
            })();
            match connected {
                Ok((binding, carrier)) => {
                    identity = Some(binding);
                    guest = Some(carrier);
                    status = "Connected to invited graph".into();
                },
                Err(error) => {
                    identity = None;
                    guest = None;
                    status = format!("Unavailable: {error}");
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
        if identity.as_ref().is_some_and(CustodyIdentity::is_locked) {
            identity = None;
            guest = None;
            status = "Identity locked".into();
        }
        let outcome = match (&mut guest, command) {
            (_, Some(Command::Reconnect)) => {
                guest = None;
                identity = None;
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
            connection: guest
                .as_ref()
                .map(|g| match g.peer_paths() {
                    Ok(paths) => paths
                        .into_iter()
                        .map(|path| {
                            format!(
                                "{} {}",
                                if path.active { "active" } else { "available" },
                                path.addr
                            )
                        })
                        .collect(),
                    Err(error) => vec![format!("unavailable: {error}")],
                })
                .unwrap_or_default(),
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
pub(crate) fn guest_reading(data: &Data) -> (MereViewModel, HashMap<String, PortableCardV1>) {
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
