// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn worker(
    owner: String,
    generation: Option<uuid::Uuid>,
    sources: Arc<dyn RetainedSourceOwner>,
    commands: mpsc::Receiver<Command>,
    snapshot: Arc<Mutex<ClientSnapshot>>,
    wake: Wake,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(error) => {
            update(
                &snapshot,
                &wake,
                ClientSnapshot {
                    status: format!("Answer connection could not start: {error}"),
                    ..Default::default()
                },
            );
            return;
        },
    };
    let mut connection = None;
    let mut next_connect = Instant::now();
    let mut next_sources = Instant::now();
    loop {
        let command = match commands.recv_timeout(wire::INFERENCE_ASK_NOTICE_POLL) {
            Ok(c) => Some(c),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if connection.is_none() && command.is_none() && Instant::now() < next_connect {
            continue;
        }
        let previous = snapshot.lock().unwrap().clone();
        let result = runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(5), async {
                if sources.generation() != generation {
                    return Err(
                        "This answer connection belongs to an earlier session opening.".into(),
                    );
                }
                // Local source availability is independent of the resident.
                // Prepare once, before connecting, so an outage cannot turn
                // a valid saved choice into an unavailable source row.
                let mut ask_scope = None;
                let corpus = if connection
                    .as_ref()
                    .is_none_or(|c: &Connection| c.published.is_none())
                    || Instant::now() >= next_sources
                    || matches!(&command, Some(Command::Ask { .. }) | Some(Command::Refresh))
                {
                    let retained = sources.snapshot(&owner)?;
                    if let Some(Command::Ask { chosen, .. }) = &command {
                        ask_scope = Some(crate::research::scope(&owner, chosen, &retained)?);
                    }
                    Some(publication(&owner, retained)?)
                } else {
                    None
                };
                if connection.is_none() {
                    crate::inference_activity::record_route_attempt();
                    let mut client = AppBrokerClient::open_declared_route(
                        AppId::new(crate::identity::CUSTODY_APP),
                        AppRouteId::new(wire::INFERENCE_ASK_ROUTE).map_err(|e| e.to_string())?,
                        crate::identity::app_declaration(),
                    )
                    .await
                    .map_err(|e| format!("Answers needs the inference resident. {e}"))?;
                    let opened = client.open_session().await.map_err(|e| e.to_string())?;
                    let request = opened
                        .descriptor
                        .projections
                        .first()
                        .ok_or("The resident exposes no answer view.")?
                        .request
                        .clone();
                    connection = Some(Connection {
                        client,
                        request,
                        published: None,
                    });
                }
                let connection = connection.as_mut().unwrap();
                let mut changed = false;
                if let Some(bytes) = corpus {
                    let fingerprint = eidetic::Hash::of(&bytes);
                    if connection.published != Some(fingerprint) {
                        intent(connection, PUBLISH_CORPUS_INTENT, bytes).await?;
                        connection.published = Some(fingerprint);
                        changed = true;
                    }
                    next_sources = Instant::now() + Duration::from_secs(1);
                }
                let mut next = previous.clone();
                if let Some(command) = command {
                    match command {
                        Command::Ask {
                            run,
                            question,
                            answer_format,
                            ..
                        } => {
                            intent(
                                connection,
                                ASK_PUBLISHED_INTENT,
                                serde_json::to_vec(&PublishedAskRequestV1 {
                                    answer_format,
                                    owner: owner.clone(),
                                    ask: wire::AskRequestV1 { run, question },
                                    scope: ask_scope.ok_or(
                                        "No explicitly chosen saved-source scope is available.",
                                    )?,
                                })
                                .map_err(|e| e.to_string())?,
                            )
                            .await?;
                        },
                        Command::Control { run, control } => {
                            if !run.starts_with(&format!("{owner}/")) {
                                return Err(
                                    "The answer belongs to another browsing session.".into()
                                );
                            }
                            intent(
                                connection,
                                control.intent(),
                                serde_json::to_vec(&wire::CancelRequestV1 { run })
                                    .map_err(|e| e.to_string())?,
                            )
                            .await?;
                        },
                        Command::Source {
                            run,
                            handle,
                            claim_link,
                        } => {
                            // Refresh first so an old click cannot reopen a withdrawn citation.
                            next.runs = read_runs(connection)
                                .await?
                                .runs
                                .into_iter()
                                .filter(|r| r.run.starts_with(&format!("{owner}/")))
                                .collect();
                            let opened = match claim_link {
                                Some((claim, link)) => resolve_claim_source(
                                    sources.as_ref(),
                                    &next.runs,
                                    &run,
                                    handle,
                                    claim,
                                    link,
                                )?,
                                None => resolve_source(sources.as_ref(), &next.runs, &run, handle)?,
                            };
                            sources.open_source(&opened, generation)?;
                            // The intent is semantic work even if an identical
                            // source snapshot would not advance its revision.
                            wake();
                            next.source = Some(opened);
                        },
                        Command::Refresh => {},
                    }
                    changed = true;
                }
                if changed
                    || connection
                        .client
                        .take_notice()
                        .await
                        .map_err(|e| e.to_string())?
                        .is_some()
                {
                    let table = read_runs(connection).await?;
                    next.ready = table.session.is_some() && table.load_error.is_none();
                    next.model = table.model;
                    next.status = table.load_error.unwrap_or_else(|| {
                        if next.ready {
                            "Connected to resident inference.".into()
                        } else {
                            "Loading the answer model…".into()
                        }
                    });
                    next.runs = table
                        .runs
                        .into_iter()
                        .filter(|r| r.run.starts_with(&format!("{owner}/")))
                        .collect();
                }
                next.choices = sources.choices();
                Ok::<_, String>(next)
            })
            .await
            .map_err(|_| "The answer connection timed out.".to_string())?
        });
        match result {
            Ok(next) => update(&snapshot, &wake, next),
            Err(error) => {
                connection = None;
                next_connect = Instant::now() + Duration::from_secs(2);
                update(
                    &snapshot,
                    &wake,
                    ClientSnapshot {
                        status: error,
                        ready: false,
                        source: None,
                        choices: sources.choices(),
                        ..previous
                    },
                );
            },
        }
    }
    if let Some(connection) = connection {
        let _ = runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(2), connection.client.close()).await
        });
    }
}
