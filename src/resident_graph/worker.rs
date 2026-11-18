// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::{sync::Arc, time::Duration};

use chirograph::{
    CapabilityProfile, CarrierNotice, CarrierOutput, CarrierRequest, CarrierRequestBody,
    CarrierResponse, CarrierResponseBody, IntentInvocation, IntentResult, PortableCardV1,
    PresentationCapability, ProtocolVersion, ResourceResponse, SessionOpen,
};
use graphshell_client::LineDriven;
use graphshell_door::projection::AppBrokerClient;
use mere_overmap::route_session::MereRouteSession;
use mere_overmap::wire::{MERE_GRAPH, MERE_NOTES};
use mere_view::{MereViewModel, MereViewRequest, ViewStatus};
use ostiary::app_admission::{AppAdmissionError, AppId, AppRouteId};
use ostiary::app_client::AppClientError;
use tokio::sync::{mpsc, watch};

use super::{actions, kept};

const DEADLINE: Duration = Duration::from_secs(15);

pub(super) enum Request {
    Graph(MereViewRequest),
    Kept(kept::Request),
    Action(Box<actions::Command>),
}

pub(super) struct Connection {
    pub requests: mpsc::Sender<Request>,
    pub updates: watch::Receiver<Update>,
}

#[derive(Clone)]
pub(super) enum Update {
    Opening,
    Failed(String),
    Read(Box<Reading>),
}

#[derive(Clone)]
pub(super) struct Reading {
    pub model: MereViewModel,
    pub focused: Option<String>,
    pub record: Option<PortableCardV1>,
    pub completed: u64,
    pub kept: Option<Arc<kept::Reply>>,
    pub offers: Option<actions::Offers>,
    pub action: Option<actions::Reply>,
}

impl Connection {
    pub(super) fn start(route: AppRouteId, wake: armillary::Wake) -> Result<Self, String> {
        let (requests, incoming) = mpsc::channel(1);
        let (outgoing, updates) = watch::channel(Update::Opening);
        std::thread::Builder::new()
            .name("turnstone-resident-graph".into())
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| error.to_string())
                    .and_then(|runtime| runtime.block_on(run(&route, incoming, &outgoing, &wake)));
                if let Err(reason) = result
                    && outgoing
                        .send(Update::Failed(format!("{route}: {reason}")))
                        .is_ok()
                {
                    wake();
                }
            })
            .map_err(|error| format!("Could not start the resident graph reader: {error}"))?;
        Ok(Self { requests, updates })
    }
}

async fn run(
    route: &AppRouteId,
    mut incoming: mpsc::Receiver<Request>,
    outgoing: &watch::Sender<Update>,
    wake: &armillary::Wake,
) -> Result<(), String> {
    let application = crate::identity::CUSTODY_APP;
    let opened = async {
        let mut client = AppBrokerClient::open_declared_route(
            AppId::new(application),
            route.clone(),
            crate::identity::app_declaration(),
        )
        .await
        .map_err(open_error)?;
        let profile = CapabilityProfile::new([PresentationCapability::PortableCard]);
        let reply = client
            .request_body(CarrierRequestBody::Open(Box::new(SessionOpen {
                version: ProtocolVersion::V1,
                capabilities: profile.clone(),
            })))
            .await
            .map_err(|error| error.to_string())?;
        let CarrierResponseBody::Opened(opened) = reply else {
            return Err("The resident did not open the graph route.".into());
        };
        let graph = opened
            .descriptor
            .projections
            .iter()
            .map(|offer| &offer.request.session)
            .find(|session| session.0 == MERE_GRAPH)
            .cloned();
        let mut panel = MereRouteSession::from_descriptor(opened.descriptor, profile, application)?;
        settle(&mut client, &mut panel, None).await?;
        Ok::<_, String>((client, panel, graph))
    };
    let (mut client, mut panel, graph) = tokio::select! {
        _ = outgoing.closed() => return Ok(()),
        result = tokio::time::timeout(DEADLINE, opened) => {
            result.map_err(|_| "Opening the resident graph timed out.".to_owned())??
        }
    };

    let mut completed = 0;
    let mut kept: Option<Arc<kept::Reply>> = None;
    let mut action = None;
    let mut notices = tokio::time::interval(Duration::from_millis(500));
    notices.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let mut model = panel.model().clone();
        model.hide_graph = false;
        let focused = panel.focus_key();
        let record = focused
            .as_ref()
            .and_then(|key| panel.records().get(key))
            .cloned();
        let offers = focused
            .as_ref()
            .and_then(|key| panel.record_actions(key).ok());
        if outgoing
            .send(Update::Read(Box::new(Reading {
                model,
                focused,
                record,
                completed,
                kept: kept.clone(),
                offers,
                action: action.clone(),
            })))
            .is_err()
        {
            break;
        }
        wake();

        // The broker notice call owns the same framed stream as requests.
        // Poll only while idle, and never cancel it to send a request mid-frame.
        let mut expected = None;
        loop {
            tokio::select! {
                biased;
                request = incoming.recv() => {
                    let Some(request) = request else {
                        let _ = tokio::time::timeout(DEADLINE, client.close()).await;
                        return Ok(());
                    };
                    kept = None;
                    action = None;
                    match request {
                        Request::Graph(request) => {
                            if let Err(reason) = panel.request(request) {
                                panel.show_notice(reason);
                            }
                        },
                        Request::Kept(request) => kept = Some(Arc::new(tokio::select! {
                            _ = outgoing.closed() => return Ok(()),
                            result = tokio::time::timeout(DEADLINE, kept::run(&mut client, graph.as_ref(), request)) => {
                                result.map_err(|_| "The kept-page request timed out.".to_owned())?
                            }
                        })),
                        Request::Action(command) => match command.queue(&mut panel) {
                            Ok(bound) => expected = Some(bound),
                            Err(reason) => action = Some(actions::Reply::new(reason)),
                        },
                    };
                    completed += 1;
                    break;
                }
                _ = notices.tick() => {
                    let notice = tokio::time::timeout(DEADLINE, client.take_notice())
                        .await
                        .map_err(|_| "Reading resident notices timed out.".to_owned())?
                        .map_err(|error| error.to_string())?;
                    if let Some(notice) = notice.filter(graph_view_notice) {
                        feed(&mut panel, CarrierOutput::Notice(notice))?;
                        break;
                    }
                }
            }
        }
        let result = tokio::select! {
            _ = outgoing.closed() => return Ok(()),
            result = tokio::time::timeout(DEADLINE, settle(
                &mut client, &mut panel, expected.as_ref().map(|(intent, _)| intent),
            )) => {
                result.map_err(|_| "The resident graph request timed out.".to_owned())??
            }
        };
        if let Some((_, address)) = expected {
            let result = result.ok_or("The resident did not answer the record action.")?;
            action = Some(actions::Reply::from_intent(result, address));
        }
    }
    let _ = tokio::time::timeout(DEADLINE, client.close()).await;
    Ok(())
}

fn graph_view_notice(notice: &CarrierNotice) -> bool {
    // The same resident route also offers Notes. MereRouteSession drives the
    // sessions, graph and archive projections, with no Notes subscription.
    notice.session.0 != MERE_NOTES
}

#[cfg(test)]
#[test]
fn resident_graph_updates_ignore_notes_notices_only() {
    use chirograph::ProjectionSession;
    use mere_overmap::wire::{MERE_ARCHIVE, MERE_GRAPH, MERE_SESSIONS};
    use scenotime::{Revision, SceneEpoch};

    let mut notice = CarrierNotice {
        session: ProjectionSession(MERE_NOTES.into()),
        epoch: SceneEpoch(2),
        revision: Revision(1),
    };
    assert!(!graph_view_notice(&notice));
    for role in [MERE_GRAPH, MERE_SESSIONS, MERE_ARCHIVE, "unexpected"] {
        notice.session = ProjectionSession(role.into());
        assert!(graph_view_notice(&notice));
    }
}

fn open_error(error: AppClientError) -> String {
    match error {
        AppClientError::Admission(AppAdmissionError::RouteNotGranted { .. }) => {
            "Djinn has no grant for Turnstone on this graph, or the graph is not served.".into()
        },
        AppClientError::Admission(AppAdmissionError::NotAllowed(_)) => {
            "Djinn has not admitted Turnstone. If you approve its request, choose Reconnect.".into()
        },
        AppClientError::Io(error) if error.kind() == std::io::ErrorKind::NotFound => {
            "Djinn is unavailable at the configured application endpoint.".into()
        },
        AppClientError::Closed => {
            "Djinn closed the connection before opening the graph. Check Turnstone’s route grant and reconnect.".into()
        },
        other => other.to_string(),
    }
}

async fn settle(
    client: &mut AppBrokerClient,
    panel: &mut MereRouteSession,
    expected: Option<&IntentInvocation>,
) -> Result<Option<IntentResult>, String> {
    let mut outcome = None;
    loop {
        let outgoing = panel.take_outgoing();
        if outgoing.is_empty() {
            if panel.in_flight() {
                return Err("The resident graph is waiting without a request.".into());
            }
            return match &panel.model().status {
                ViewStatus::Unavailable(note) => Err(note.message.clone()),
                _ => Ok(outcome),
            };
        }
        for line in outgoing {
            let request = chirograph::line::decode::<CarrierRequest>(&line)
                .map_err(|error| error.to_string())?
                .ok_or("The resident graph emitted an empty request.")?;
            let bound = matches!((&request.body, expected),
                (CarrierRequestBody::Intent(actual), Some(expected)) if actual == expected);
            let body = match request.body {
                CarrierRequestBody::Resource(request) => {
                    let bytes = client
                        .resource(request.session.clone(), request.resource)
                        .await
                        .map_err(|error| error.to_string())?;
                    CarrierResponseBody::Resource(ResourceResponse::new(request.session, bytes))
                },
                body => client
                    .request_body(body)
                    .await
                    .map_err(|error| error.to_string())?,
            };
            if bound && let CarrierResponseBody::Intent(result) = &body {
                outcome = Some(result.clone());
            }
            // The application door correlates its own frames. Preserve the view driver's id.
            feed(
                panel,
                CarrierOutput::Response(CarrierResponse {
                    id: request.id,
                    body: Ok(body),
                }),
            )?;
        }
    }
}

fn feed(panel: &mut MereRouteSession, output: CarrierOutput) -> Result<(), String> {
    let line = chirograph::line::encode(&output).map_err(|error| error.to_string())?;
    LineDriven::on_line(panel, &line)
}
