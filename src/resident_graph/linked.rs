// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Share only through a graph pane that already holds its exact resident route.
use crate::linked_queries::Destination;
use graphshell_door::projection::AppBrokerClient;
use mere_overmap::saved_views::{SavedViewsClient, SourceMaterial};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};
use turnstone_ontology::{SavedView, selected_profile};

#[derive(Default, Clone)]
pub(crate) struct LinkedRoutes(
    Arc<Mutex<BTreeMap<String, tokio::sync::mpsc::WeakSender<super::worker::Request>>>>,
);

pub(crate) struct Share {
    pub domain: String,
    pub view: SavedView,
    pub material: Vec<SourceMaterial>,
    pub digest: String,
    pub expected: Option<Destination>,
    pub permitted: Arc<dyn Fn() -> bool + Send + Sync>,
}

pub(super) struct Packet {
    share: Share,
    deadline: Instant,
    reply: mpsc::SyncSender<Result<Destination, String>>,
}

impl LinkedRoutes {
    pub(super) fn register(
        &self,
        route: &ostiary::app_admission::AppRouteId,
        connection: &super::worker::Connection,
    ) {
        let mut routes = self.0.lock().unwrap();
        routes.retain(|_, s| s.strong_count() > 0);
        routes.insert(route.as_str().to_owned(), connection.requests.downgrade());
    }

    pub(crate) fn send(&self, share: Share) -> Result<Destination, String> {
        let route = super::resident_route(&share.domain)?;
        let sender = self
            .0
            .lock()
            .unwrap()
            .get(route.as_str())
            .and_then(|s| s.upgrade())
            .ok_or("Open this resident graph pane and attach its destination session first.")?;
        let (reply, reader) = mpsc::sync_channel(1);
        let wait = Duration::from_secs(15);
        sender
            .try_send(super::worker::Request::Linked(Packet {
                share,
                deadline: Instant::now() + wait,
                reply,
            }))
            .map_err(|_| "The resident graph pane is busy or closed.")?;
        reader
            .recv_timeout(wait)
            .map_err(|_| "The resident graph pane did not confirm the request.")?
    }
}

impl Packet {
    pub(super) async fn complete(
        self,
        client: &mut AppBrokerClient,
        session: Option<pandect::SessionId>,
    ) -> Result<(), String> {
        let execute = async {
            let share = &self.share;
            let permitted = || {
                if Instant::now() > self.deadline || !(share.permitted)() {
                    return Err("The selected evidence or its access changed.".to_string());
                }
                Ok(())
            };
            permitted()?;
            let session = session.ok_or("The resident graph has no attached session.")?;
            let mut client = SavedViewsClient::borrow_held(
                client,
                crate::identity::CUSTODY_APP,
                &share.domain,
                session,
            )
            .await?;
            let target = Destination {
                domain: share.domain.clone(),
                scope: client.scope().to_owned(),
                session: session.as_uuid().to_string(),
                digest: share.digest.clone(),
                count: share.material.iter().map(|s| s.statements.len()).sum(),
            };
            if let Some(expected) = &share.expected {
                if target.scope != expected.scope
                    || target.session != expected.session
                    || target.digest != expected.digest
                {
                    return Err(
                        "The destination or evidence changed. Review the share again.".into(),
                    );
                }
                let profile = selected_profile().map_err(str::to_owned)?;
                let view = SavedView::new(
                    share.view.name.clone(),
                    target.scope.clone(),
                    share.view.sources.clone(),
                    share.view.query.clone(),
                    profile,
                )?;
                permitted()?;
                client
                    .share_if(&view, profile, &share.material, || permitted().is_ok())
                    .await?;
            }
            Ok(target)
        };
        match tokio::time::timeout_at(self.deadline.into(), execute).await {
            Ok(result) => {
                let _ = self.reply.send(result);
                Ok(())
            },
            Err(_) => {
                // Cancellation can interrupt a broker frame. Retire this connection.
                let reason = "The resident did not confirm before the deadline.".to_string();
                let _ = self.reply.send(Err(reason.clone()));
                Err(reason)
            },
        }
    }
}

#[cfg(test)]
pub(crate) fn open_fixture_graph(
    routes: &LinkedRoutes,
    domain: &str,
) -> Result<impl Sized, String> {
    let route = super::resident_route(domain)?;
    let (sender, receiver) = mpsc::channel();
    let mut connection = super::worker::Connection::start(
        route.clone(),
        Arc::new(move || {
            let _ = sender.send(());
        }),
    )?;
    routes.register(&route, &connection);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let update = connection.updates.borrow_and_update().clone();
        match update {
            super::worker::Update::Read(read)
                if read.model.sessions.iter().any(|s| s.attached && !s.trashed) =>
            {
                return Ok(connection);
            },
            super::worker::Update::Failed(error) => return Err(error),
            _ => {},
        }
        receiver
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| "The fixture resident did not attach a graph.".to_string())?;
    }
}
