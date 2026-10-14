// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Event-driven shared mere panel over one admitted endpoint. SessionDriver
//! owns wire correlation and resource validation; this adapter sequences the
//! sessions, graph and optional archive readings and never owns a store.
use crate::{
    mere_route,
    session_item::{MERE_ARCHIVE, MERE_GRAPH, MERE_SESSIONS},
};
use chirograph::{
    CapabilityProfile, ContentHash, EndpointDescriptor, ProjectionSession, ProjectionSnapshot,
    ProtocolVersion,
};
use graphshell_client::core::Outcome;
use graphshell_client::driver::Advance;
use graphshell_client::{MountedScene, ResolvedContent, SessionDriver};
use mere_view::{HostAction, MereViewModel, MereViewRequest, StatusNote, ViewStatus};
use sceno::InstanceId;
use std::collections::{HashMap, VecDeque};
#[derive(Clone)]
enum Task {
    Mount(usize),
    Resolve(ProjectionSession, InstanceId, ContentHash),
    Invoke(MereViewRequest),
    Media(ProjectionSession, InstanceId),
}
pub struct MereRouteSession {
    driver: SessionDriver,
    indices: Vec<usize>,
    sessions: ProjectionSession,
    graph: ProjectionSession,
    archive: Option<ProjectionSession>,
    tasks: VecDeque<Task>,
    current: Option<Task>,
    cards: HashMap<ContentHash, Vec<u8>>,
    outbox: Vec<String>,
    model: MereViewModel,
    notice: Option<String>,
    closed: bool,
    primary: Option<MountedScene>,
    saved_view: pandect::ViewIntent,
    read_only: bool,
    material: Option<crate::session_item::SessionGraphV1>,
    records: HashMap<String, chirograph::PortableCardV1>,
    inspected: Option<String>,
    transient_layout: Option<String>,
}
impl MereRouteSession {
    pub fn from_descriptor(
        descriptor: EndpointDescriptor,
        profile: CapabilityProfile,
    ) -> Result<Self, String> {
        let find = |name: &str| -> Result<Option<usize>, String> {
            let matching: Vec<_> = descriptor
                .projections
                .iter()
                .enumerate()
                .filter(|(_, offer)| offer.request.session.0 == name)
                .map(|(index, _)| index)
                .collect();
            if matching.len() > 1 {
                return Err(format!(
                    "The route advertises duplicate {name} projections."
                ));
            }
            Ok(matching.first().copied())
        };
        let sessions_index =
            find(MERE_SESSIONS)?.ok_or("The route offers no sessions projection.")?;
        let graph_index = find(MERE_GRAPH)?.ok_or("The route offers no graph projection.")?;
        let archive_index = find(MERE_ARCHIVE)?;
        let sessions = descriptor.projections[sessions_index]
            .request
            .session
            .clone();
        let graph = descriptor.projections[graph_index].request.session.clone();
        let archive =
            archive_index.map(|index| descriptor.projections[index].request.session.clone());
        let indices = [Some(sessions_index), Some(graph_index), archive_index]
            .into_iter()
            .flatten()
            .collect();
        let mut result = Self {
            driver: SessionDriver::from_descriptor(descriptor, profile),
            indices,
            sessions,
            graph,
            archive,
            tasks: VecDeque::new(),
            current: None,
            cards: HashMap::new(),
            outbox: Vec::new(),
            model: MereViewModel {
                title: "Shared mere".into(),
                hide_graph: true,
                status: ViewStatus::Building(StatusNote {
                    message: "Reading the resident's mere.".into(),
                    action: None,
                }),
                ..Default::default()
            },
            notice: None,
            closed: false,
            primary: None,
            saved_view: Default::default(),
            read_only: false,
            material: None,
            records: HashMap::new(),
            inspected: None,
            transient_layout: None,
        };
        result.refresh()?;
        Ok(result)
    }
    pub fn show_notice(&mut self, reason: String) {
        self.model.notice = Some(reason);
    }
    pub fn model(&self) -> &MereViewModel {
        &self.model
    }
    pub fn saved_view(&self) -> &pandect::ViewIntent {
        &self.saved_view
    }
    pub fn records(&self) -> &HashMap<String, chirograph::PortableCardV1> {
        &self.records
    }
    pub fn read_only(&self) -> bool {
        self.read_only
    }
    pub fn view_saved(&self) -> bool {
        let Some(key) = self.focus_key() else {
            return false;
        };
        let address = self
            .records
            .get(&key)
            .and_then(|card| card.values.iter().find(|value| value.label == "Address"));
        !self.in_flight()
            && address.is_some_and(|address| self.saved_view.focus.as_ref() == Some(&address.value))
            && self
                .saved_view
                .strategy
                .as_deref()
                .unwrap_or("spectral.default")
                == self.model.layout
    }
    pub fn focus_instance(&self) -> Option<String> {
        let key = self.focus_key()?;
        let primary = self.primary()?;
        primary
            .scene
            .active_items_in_order()
            .into_iter()
            .find(|(_, item)| {
                primary.scene.tables.sources[item.source.0 as usize]
                    .as_ref()
                    .is_some_and(|source| source.id == key)
            })
            .map(|(instance, _)| instance.0.to_string())
    }
    pub fn focus_key(&self) -> Option<String> {
        if let Some(key) = &self.inspected {
            return self.records.contains_key(key).then(|| key.clone());
        }
        let snapshot = self.snapshot(&self.graph).ok()?;
        let focus = self.saved_view.focus.as_ref()?;
        mere_route::node_cards(&snapshot, &self.cards)
            .into_iter()
            .find(|(_, card)| {
                card.values
                    .iter()
                    .any(|value| value.label == "Address" && &value.value == focus)
            })
            .map(|(key, _)| key)
    }
    pub fn primary(&self) -> Option<&MountedScene> {
        if self.closed {
            return None;
        }
        self.primary.as_ref()
    }
    pub fn in_flight(&self) -> bool {
        self.current.is_some() || !self.tasks.is_empty() || self.driver.is_awaiting()
    }
    pub fn status(&self) -> &str {
        if self.closed {
            "unavailable"
        } else if self.in_flight() {
            "reading shared mere"
        } else {
            "open"
        }
    }
    pub fn take_outgoing(&mut self) -> Vec<String> {
        std::mem::take(&mut self.outbox)
    }
    pub fn request(&mut self, request: MereViewRequest) -> Result<(), String> {
        if self.closed {
            return Err("The resident mere is unavailable; reconnect.".into());
        }
        if self.in_flight() {
            return Err("Wait for the resident's current operation.".into());
        }
        if matches!(&request,MereViewRequest::Host(key) if key=="refresh") {
            return self.refresh();
        }
        self.notice = None;
        match &request {
            MereViewRequest::Activate(key) => {
                if !self.records.contains_key(key) {
                    return Err("The record is no longer disclosed.".into());
                }
                self.inspected = Some(key.clone());
            },
            MereViewRequest::Layout(key) => {
                if !self.model.layouts.iter().any(|(id, _)| id == key) {
                    return Err("That layout is not offered.".into());
                }
                self.transient_layout = Some(key.clone());
            },
            _ => {},
        }
        if self.read_only
            && matches!(
                &request,
                MereViewRequest::Activate(_) | MereViewRequest::Layout(_)
            )
        {
            self.notice = Some("Viewing a read-only archive; this view is not saved.".into());
            return self.advance();
        }
        self.tasks.push_back(Task::Invoke(request));
        self.advance()
    }
    pub fn on_line(&mut self, line: &str) {
        if self.closed {
            return;
        }
        match self.driver.on_line(line) {
            Ok(advance) => {
                if let Err(reason) = self.accept(advance).and_then(|()| self.advance()) {
                    self.fail(reason);
                }
            },
            Err(reason) => self.fail(reason),
        }
    }
    pub fn fail(&mut self, reason: String) {
        self.closed = true;
        self.tasks.clear();
        self.current = None;
        self.cards.clear();
        self.outbox.clear();
        self.primary = None;
        self.saved_view = Default::default();
        self.read_only = false;
        self.material = None;
        self.records.clear();
        self.inspected = None;
        self.transient_layout = None;
        self.driver.disconnect();
        if let Some(core) = self.driver.core_mut() {
            core.purge();
        }
        self.model = MereViewModel {
            title: "Shared mere".into(),
            hide_graph: true,
            status: ViewStatus::Unavailable(StatusNote {
                message: reason,
                action: None,
            }),
            ..Default::default()
        };
    }
    fn snapshot(&self, session: &ProjectionSession) -> Result<ProjectionSnapshot, String> {
        let mounted = self
            .driver
            .core()
            .and_then(|core| core.client().mounted(session))
            .ok_or("The resident projection is not mounted.")?;
        Ok(ProjectionSnapshot {
            version: ProtocolVersion { major: 1, minor: 0 },
            session: session.clone(),
            scene: mounted.scene.clone(),
            presentation: mounted.presentation.clone(),
            cache_policy: mounted.cache_policy.clone(),
        })
    }
    fn refresh(&mut self) -> Result<(), String> {
        self.cards.clear();
        self.tasks
            .extend(self.indices.iter().copied().map(Task::Mount));
        self.advance()
    }
    fn advance(&mut self) -> Result<(), String> {
        loop {
            if self.current.is_some() {
                return Ok(());
            }
            let Some(task) = self.tasks.pop_front() else {
                let sessions = self.snapshot(&self.sessions)?;
                let graph = self.snapshot(&self.graph)?;
                let mut primary = self
                    .driver
                    .core()
                    .and_then(|core| core.client().mounted(&self.graph))
                    .cloned()
                    .ok_or("The graph is not mounted.")?;
                let hidden: Vec<_> = primary
                    .scene
                    .active_items_in_order()
                    .into_iter()
                    .filter(|(_, item)| {
                        primary.scene.tables.sources[item.source.0 as usize]
                            .as_ref()
                            .is_some_and(|source| {
                                source.adapter == crate::session_item::SESSION_ITEM_SOURCE
                            })
                    })
                    .map(|(instance, _)| instance.0 as usize)
                    .collect();
                for index in hidden {
                    primary.scene.tables.items[index] = None;
                    primary.scene.tables.item_order[index] = None;
                }
                mere_route::fill(&mut self.model, &sessions, &graph, &self.cards);
                self.records = mere_route::node_cards(&graph, &self.cards);
                if let Some(material) = &self.material {
                    mere_route::add_node_facets(&mut self.records, &material.facets)?;
                }
                if self
                    .inspected
                    .as_ref()
                    .is_some_and(|key| !self.records.contains_key(key))
                {
                    self.inspected = None;
                }
                self.model.layouts = vec![
                    ("spectral.default".into(), "Spectral".into()),
                    ("grid.default".into(), "Grid".into()),
                    ("phyllotaxis.default".into(), "Spiral".into()),
                ];
                self.model.layout = "spectral.default".into();
                if let Some(strategy) = self
                    .transient_layout
                    .as_ref()
                    .or(self.saved_view.strategy.as_ref())
                {
                    self.model.layout = strategy.clone();
                    let positions = mere_view::lay_out(&self.model.graph, strategy, 1280, 720);
                    for item in primary.scene.tables.items.iter_mut().flatten() {
                        if let Some(source) =
                            primary.scene.tables.sources[item.source.0 as usize].as_ref()
                        {
                            if let Some((x, y)) = positions.get(&source.id) {
                                item.transform.translate.x = x * 1280.;
                                item.transform.translate.y = y * 720.;
                            }
                        }
                    }
                    primary.scene.tables.bounds.origin.x = 0.;
                    primary.scene.tables.bounds.origin.y = 0.;
                    primary.scene.tables.bounds.size.w = 1280.;
                    primary.scene.tables.bounds.size.h = 720.;
                }
                self.primary = Some(primary);
                if self.read_only {
                    if let Some(session) =
                        self.model.sessions.iter_mut().find(|entry| entry.attached)
                    {
                        session.detail = Some("Read-only archive".into());
                    }
                }
                if let Some(archive) = &self.archive {
                    let archive = self.snapshot(archive)?;
                    mere_route::fill_archive(&mut self.model, &sessions, &archive, &self.cards);
                }
                self.model.status = ViewStatus::Ready;
                self.model.notice = self.notice.clone();
                self.model.actions = vec![HostAction::new("refresh", "Refresh")];
                if self.drain_notices()? {
                    return self.refresh();
                }
                return Ok(());
            };
            let progress = match &task {
                Task::Mount(index) => self
                    .driver
                    .core_mut()
                    .ok_or("The route is closed.")?
                    .mount(*index)?,
                Task::Resolve(session, instance, _) => self
                    .driver
                    .core_mut()
                    .ok_or("The route is closed.")?
                    .resolve(session, *instance)?,
                Task::Media(session, instance) => self
                    .driver
                    .core_mut()
                    .ok_or("The route is closed.")?
                    .media_resource(session, *instance, 0)?,
                Task::Invoke(request) => {
                    let sessions = self.snapshot(&self.sessions)?;
                    let invocation = self
                        .view_invocation(request)?
                        .or_else(|| mere_route::intent(request, &sessions))
                        .or_else(|| {
                            self.archive
                                .as_ref()
                                .and_then(|archive| self.snapshot(archive).ok())
                                .and_then(|archive| {
                                    mere_route::archive_intent(request, &sessions, &archive)
                                })
                        })
                        .ok_or("The owner does not advertise that action.")?;
                    let snapshot = self.snapshot(&invocation.session)?;
                    let action = snapshot
                        .presentation
                        .offers_for(invocation.target)
                        .and_then(|offers| {
                            offers
                                .iter()
                                .flat_map(|offer| offer.semantics.actions.iter())
                                .find(|action| action.intent.0 == invocation.intent)
                        })
                        .cloned()
                        .ok_or("The owner no longer advertises that action.")?;
                    let payload: serde_json::Value =
                        serde_json::from_slice(&invocation.payload).map_err(|e| e.to_string())?;
                    self.driver
                        .core_mut()
                        .ok_or("The route is closed.")?
                        .invoke(&invocation.session, invocation.target, &action, &payload)?
                },
            };
            self.current = Some(task);
            let advance = self.driver.begin(progress)?;
            let completed = matches!(&advance, Advance::Done(_));
            self.accept(advance)?;
            if !completed {
                return Ok(());
            }
        }
    }
    fn view_invocation(
        &self,
        request: &MereViewRequest,
    ) -> Result<Option<chirograph::IntentInvocation>, String> {
        let mut view = self.saved_view.clone();
        match request {
            MereViewRequest::Activate(key) => {
                view.focus = Some(
                    self.records
                        .get(key)
                        .and_then(|card| card.values.iter().find(|value| value.label == "Address"))
                        .ok_or("The selected record has no disclosed address.")?
                        .value
                        .clone(),
                );
                view.strategy = Some(view.strategy.unwrap_or_else(|| "spectral.default".into()));
            },
            MereViewRequest::Layout(key) => view.strategy = Some(key.clone()),
            _ => return Ok(None),
        }
        if self.read_only {
            return Err("A read-only archive cannot save this view implicitly.".into());
        }
        let snapshot = self.snapshot(&self.graph)?;
        let target = snapshot
            .scene
            .active_items_in_order()
            .into_iter()
            .find(|(_, item)| {
                snapshot.scene.tables.sources[item.source.0 as usize]
                    .as_ref()
                    .is_some_and(|source| {
                        source.adapter == crate::session_item::SESSION_ITEM_SOURCE
                    })
            })
            .map(|(instance, _)| instance)
            .ok_or("The owner offers no session view target.")?;
        Ok(Some(chirograph::IntentInvocation {
            session: self.graph.clone(),
            target,
            observed_epoch: snapshot.scene.epoch,
            observed_revision: snapshot.scene.revision,
            intent: crate::session_item::SET_VIEW_INTENT.into(),
            payload: serde_json::to_vec(&crate::session_item::SetViewV1::new(
                "graphshell.mere",
                view,
            ))
            .map_err(|e| e.to_string())?,
        }))
    }
    fn drain_notices(&mut self) -> Result<bool, String> {
        let mut moved = false;
        while let Some(notice) = self.driver.take_notice() {
            if notice.session != self.sessions
                && notice.session != self.graph
                && self.archive.as_ref() != Some(&notice.session)
            {
                return Err("The resident notice names an undisclosed projection role.".into());
            }
            let current = self
                .driver
                .core()
                .and_then(|core| core.client().mounted(&notice.session));
            if current.is_some_and(|mounted| {
                notice.epoch.0 < mounted.scene.epoch.0
                    || (notice.epoch == mounted.scene.epoch
                        && notice.revision.0 <= mounted.scene.revision.0)
            }) {
                continue;
            }
            moved = true;
        }
        Ok(moved)
    }
    fn accept(&mut self, advance: Advance) -> Result<(), String> {
        match advance {
            Advance::Send(line) => self.outbox.push(line),
            Advance::Noted => {
                if self.current.is_none() && self.tasks.is_empty() {
                    if self.drain_notices()? {
                        self.refresh()?;
                    }
                }
            },
            Advance::Done(outcome) => {
                let task = self
                    .current
                    .take()
                    .ok_or("Unexpected resident operation completion.")?;
                match (task, outcome) {
                    (Task::Mount(_), Outcome::Mounted(session)) => {
                        let snapshot = self.snapshot(&session)?;
                        let mut resolves = Vec::new();
                        for binding in &snapshot.presentation.bindings {
                            if let Some(offer) = snapshot
                                .presentation
                                .offers
                                .get(&binding.key)
                                .and_then(|offers| offers.first())
                            {
                                resolves.push(Task::Resolve(
                                    session.clone(),
                                    binding.instance,
                                    offer.resource,
                                ));
                            }
                        }
                        for task in resolves.into_iter().rev() {
                            self.tasks.push_front(task);
                        }
                    },
                    (Task::Resolve(session, instance, hash), Outcome::Resolved(presentation)) => {
                        if let ResolvedContent::PortableCard(card) = presentation.content {
                            let snapshot = self.snapshot(&session)?;
                            let is_session = session == self.graph
                                && snapshot.scene.tables.items[instance.0 as usize]
                                    .as_ref()
                                    .and_then(|item| {
                                        snapshot.scene.tables.sources[item.source.0 as usize]
                                            .as_ref()
                                    })
                                    .is_some_and(|source| {
                                        source.adapter == crate::session_item::SESSION_ITEM_SOURCE
                                    });
                            if is_session {
                                if card.media.is_empty() {
                                    return Err("The explicit session does not disclose its scoped graph material.".into());
                                }
                                self.tasks.push_front(Task::Media(session, instance));
                            }
                            self.cards.insert(
                                hash,
                                serde_json::to_vec(&card).map_err(|e| e.to_string())?,
                            );
                        } else {
                            return Err("The mere panel requires a disclosed portable card.".into());
                        }
                    },
                    (Task::Media(_, instance), Outcome::MediaResource(resource)) => {
                        let material: crate::session_item::SessionGraphV1 =
                            serde_json::from_slice(&resource.bytes)
                                .map_err(|e| format!("Invalid scoped graph material: {e}"))?;
                        let graph = self.snapshot(&self.graph)?;
                        let attached = graph.scene.tables.items[instance.0 as usize]
                            .as_ref()
                            .and_then(|item| {
                                graph.scene.tables.sources[item.source.0 as usize].as_ref()
                            })
                            .and_then(|source| uuid::Uuid::parse_str(&source.id).ok())
                            .ok_or("The material card names no attached session.")?;
                        if material.schema != crate::session_item::SESSION_GRAPH_SCHEMA
                            || material.application != "graphshell"
                            || *material.session.as_uuid() != attached
                            || material.epoch != graph.scene.epoch
                            || material.revision != graph.scene.revision
                        {
                            return Err("The graph material has another application, session or acknowledged position.".into());
                        }
                        if self
                            .material
                            .as_ref()
                            .is_some_and(|previous| previous.session != material.session)
                        {
                            self.inspected = None;
                            self.transient_layout = None;
                        }
                        self.saved_view = material
                            .views
                            .get("graphshell.mere")
                            .cloned()
                            .unwrap_or_default();
                        self.read_only = material.read_only;
                        self.material = Some(material);
                    },
                    (Task::Invoke(request), Outcome::Intent(result)) => {
                        self.notice = mere_route::declined(&result);
                        if matches!(request, MereViewRequest::Layout(_)) {
                            self.transient_layout = None;
                        }
                        self.tasks
                            .extend(self.indices.iter().copied().map(Task::Mount));
                    },
                    _ => return Err("The resident answered another operation.".into()),
                }
            },
        }
        Ok(())
    }
}
