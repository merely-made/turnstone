// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::{
    App,
    research_search_state::{Command as Search, View},
};
use crate::{
    action::Effect,
    linked_queries::{
        self as linked, Command, FileAction, FileResult, Operation, Outcome, Request, Response,
        Stamp,
    },
    research_search::Member,
};
use turnstone_ontology::{SavedView, ViewQuery, selected_profile};
use uuid::Uuid;

impl App {
    pub(super) fn linked_query_rows(&self, push: &mut impl FnMut(String, Search)) {
        let state = &self.research_search.linked;
        let mut cmd = |text: String, action| push(text, Search::Linked(action));
        for name in &state.outcome.names {
            cmd(
                format!("Open saved query · {name}"),
                Command::Named(name.clone()),
            );
        }
        if let Some(view) = &state.view {
            cmd(format!("Saved query · {}", view.name), Command::Menu);
            cmd("Run saved query".into(), Command::Run);
            cmd("Browse saved queries".into(), Command::Catalog);
            cmd("Use selected revisions".into(), Command::Rebind);
            cmd("Export definition…".into(), Command::Export);
        }
        cmd("Open definition…".into(), Command::Import);
        if let Some(destination) = &state.outcome.destination {
            cmd(
                format!(
                    "Share {} statements to {} · session {}",
                    destination.count, destination.domain, destination.session
                ),
                Command::Share,
            );
        }
        if let Some(query) = &state.query {
            let target = match query {
                ViewQuery::Type { class } => format!("type {class}"),
                ViewQuery::Incoming { record } => format!("incoming to {record}"),
                ViewQuery::Outgoing { record } => format!("outgoing from {record}"),
            };
            cmd(
                format!("Create from selected revisions · {} · {target}", state.name),
                Command::Create,
            );
        }
        for result in &state.outcome.rows {
            let details = match &result.revision {
                None => "Unavailable".to_string(),
                Some(v) => format!("{} matches · revision {v}", result.statements.len()),
            };
            cmd(format!("{details} · {}", result.source), Command::Menu);
            for row in &result.statements {
                let action = state
                    .accepted
                    .as_ref()
                    .map_or(Command::Menu, |(request, _)| Command::Open {
                        request: *request,
                        source: result.source.clone(),
                    });
                cmd(
                    format!("{} → {} → {}", row.subject, row.predicate, row.object),
                    action,
                );
            }
        }
        for issue in &state.outcome.issues {
            cmd(issue.clone(), Command::Menu);
        }
        if !state.outcome.message.is_empty() {
            cmd(state.outcome.message.clone(), Command::Menu);
        }
        push("Name query…".into(), Search::Input("linked_name".into()));
        push(
            "Incoming links to record…".into(),
            Search::Input("linked_incoming".into()),
        );
        push(
            "Outgoing links from record…".into(),
            Search::Input("linked_outgoing".into()),
        );
        if let Ok(profile) = selected_profile() {
            for (_, term) in profile.terms.iter().filter(|(_, t)| {
                t.kind == turnstone_ontology::TermKind::Class
                    && t.pack != mere::profile::core::CORE_PACK_ID
            }) {
                push(
                    format!(
                        "Records of type · {} · {}",
                        term.display_label("en"),
                        term.iri
                    ),
                    Search::Linked(Command::Class(term.iri.clone())),
                );
            }
        }
        if state.view.is_some() {
            push(
                "Share to resident domain…".into(),
                Search::Input("linked_domain".into()),
            );
        }
        push("Choose retained revisions…".into(), Search::Sources);
        push("Back to results".into(), Search::Results);
    }

    fn show_linked(&mut self) -> Vec<Effect> {
        self.research_search.view = View::Linked;
        self.recompute_omnibar_suggestions();
        vec![Effect::Redraw]
    }

    pub(super) fn linked_query_input(&mut self, field: &str, value: String) -> Vec<Effect> {
        let state = &mut self.research_search.linked;
        match field {
            "name" => state.name = value.trim().to_string(),
            "incoming" => {
                state.query = Some(ViewQuery::Incoming {
                    record: value.trim().into(),
                })
            },
            "outgoing" => {
                state.query = Some(ViewQuery::Outgoing {
                    record: value.trim().into(),
                })
            },
            "domain" => {
                state.domain = value.trim().into();
                return self.linked_query_command(Command::PreviewShare);
            },
            _ => {},
        }
        self.show_linked()
    }

    pub(super) fn linked_query_command(&mut self, command: Command) -> Vec<Effect> {
        match self.linked_action(command) {
            Ok(Some(effect)) => {
                let mut effects = self.show_linked();
                effects.insert(0, effect);
                effects
            },
            Ok(None) => self.show_linked(),
            Err(error) => {
                self.research_search.linked.outcome = Outcome {
                    message: error,
                    ..Default::default()
                };
                self.show_linked()
            },
        }
    }

    fn linked_action(&mut self, command: Command) -> Result<Option<Effect>, String> {
        if command == Command::Menu {
            return Ok(None);
        }
        if let Command::Class(class) = command {
            self.research_search.linked.query = Some(ViewQuery::Type { class });
            return Ok(None);
        }
        let stamp = Stamp::current(&self.research)?;
        let state = &mut self.research_search.linked;
        if command == Command::Import {
            return Ok(Some(Effect::LinkedQueryFile(FileAction::Import, stamp)));
        }
        if matches!(command, Command::Catalog | Command::Named(_)) {
            let token = Uuid::new_v4();
            state.pending = Some(token);
            state.accepted = None;
            let operation = match command {
                Command::Named(name) => Operation::Named(name),
                _ => Operation::Catalog,
            };
            return Ok(Some(Effect::LinkedQuery(Box::new(Request {
                token,
                stamp,
                view: None,
                operation,
            }))));
        }
        let profile = selected_profile().map_err(str::to_owned)?;
        let mut bindings = Vec::new();
        let mut ids = Vec::new();
        if matches!(command, Command::Create | Command::Rebind) {
            let selection = self
                .research_search
                .selected
                .as_deref()
                .filter(|s| !s.is_empty())
                .ok_or("Choose one or more specific retained revisions.")?;
            for member in selection {
                let Member::Capture(key) = member else {
                    return Err("Choose kept HTML captures for linked queries.".into());
                };
                let admitted = stamp
                    .context
                    .admissions
                    .iter()
                    .find(|a| &a.key == key && a.error.is_none() && a.key.is_sealed())
                    .ok_or("The selected sealed capture is unavailable.")?;
                let id = linked::source_id(admitted);
                if ids.contains(&id) {
                    return Err("Choose just one revision of each source.".into());
                }
                ids.push(id);
                bindings.push(key.clone());
            }
        }
        let view = if command == Command::Create {
            SavedView::new(
                state.name.clone(),
                stamp.scope(),
                ids,
                state
                    .query
                    .clone()
                    .ok_or("Choose a query type or a record.")?,
                profile,
            )?
        } else {
            state.view.clone().ok_or("Create or open a query first.")?
        };
        view.check_scope(&stamp.scope(), profile)?;
        let operation = match command {
            Command::Create | Command::Rebind => Operation::Bind(bindings),
            Command::Run => Operation::Run,
            Command::Open { request, source } => {
                if !state
                    .accepted
                    .as_ref()
                    .is_some_and(|(id, s)| id == &request && s == &stamp)
                {
                    return Err("The linked query result is stale.".into());
                }
                let revision = state
                    .outcome
                    .rows
                    .iter()
                    .find(|row| row.source == source)
                    .and_then(|row| row.revision.clone())
                    .ok_or("The linked source is unavailable.")?;
                Operation::Open { source, revision }
            },
            Command::Export => {
                return Ok(Some(Effect::LinkedQueryFile(
                    FileAction::Export(view),
                    stamp,
                )));
            },
            Command::PreviewShare => Operation::Preview(state.domain.clone()),
            Command::Share => {
                let target = state
                    .outcome
                    .destination
                    .as_ref()
                    .ok_or("Choose the resident domain again.")?;
                Operation::Share {
                    domain: target.domain.clone(),
                    session: target.session.clone(),
                    destination: target.scope.clone(),
                    expected: target.digest.clone(),
                }
            },
            _ => return Ok(None),
        };
        let token = Uuid::new_v4();
        state.pending = Some(token);
        state.accepted = None;
        state.outcome = Outcome {
            message: "Reading selected revisions…".into(),
            ..Default::default()
        };
        Ok(Some(Effect::LinkedQuery(Box::new(Request {
            token,
            stamp,
            view: Some(view),
            operation,
        }))))
    }

    pub(crate) fn linked_query_file(
        &mut self,
        stamp: Stamp,
        result: Result<FileResult, String>,
    ) -> Vec<Effect> {
        if Stamp::current(&self.research).ok().as_ref() != Some(&stamp) {
            return vec![];
        }
        let state = &mut self.research_search.linked;
        match result {
            Ok(FileResult::Opened(view)) => {
                if let Err(e) = selected_profile()
                    .map_err(str::to_owned)
                    .and_then(|p| view.check_scope(&stamp.scope(), p))
                {
                    state.outcome = Outcome {
                        message: e,
                        ..Default::default()
                    };
                } else {
                    state.name = view.name.clone();
                    state.query = Some(view.query.clone());
                    state.view = Some(view);
                    return self.linked_query_command(Command::Run);
                }
            },
            Ok(FileResult::Saved) => {
                state.outcome.message = "Exported the query definition.".into()
            },
            Err(e) => state.outcome.message = e,
        }
        self.show_linked()
    }

    pub(crate) fn accept_linked_query(&mut self, reply: Response) -> Vec<Effect> {
        if self.research_search.linked.pending != Some(reply.token)
            || Stamp::current(&self.research).ok().as_ref() != Some(&reply.stamp)
        {
            return vec![];
        }
        self.research_search.linked.pending = None;
        self.research_search.linked.accepted = Some((reply.token, reply.stamp));
        self.research_search.linked.outcome = reply.result.unwrap_or_else(|message| Outcome {
            message,
            ..Default::default()
        });
        if let Some(view) = self.research_search.linked.outcome.active.take() {
            let state = &mut self.research_search.linked;
            state.name = view.name.clone();
            state.query = Some(view.query.clone());
            state.view = Some(view);
        }
        if let Some(target) = self.research_search.linked.outcome.open.take() {
            if !self.saved_source_admitted(&target.key) {
                return self
                    .saved_source_refusal("The linked source is no longer admitted.".into());
            }
            self.omnibar.open = false;
            self.shell.close_omnibar();
            return self.open_saved_source(target);
        }
        self.recompute_omnibar_suggestions();
        vec![Effect::Redraw]
    }
}
