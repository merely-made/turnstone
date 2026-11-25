// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Saved linked queries are portable definitions. Bindings and source readings
//! belong to the current session and must be admitted again for every run.
use crate::{
    research,
    saved_sources::{Admission, Key},
};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};
use turnstone_ontology::{SavedView, SourceResult, ViewQuery};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Menu,
    Catalog,
    Named(String),
    Open { request: Uuid, source: String },
    Class(String),
    Create,
    Run,
    Rebind,
    Import,
    Export,
    PreviewShare,
    Share,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub context: research::Context,
    pub revision: u64,
    pub epoch: u64,
}

impl Stamp {
    pub(crate) fn current(bridge: &research::Bridge) -> Result<Self, String> {
        let (context, revision) = bridge.context_revision()?;
        let epoch = (bridge.clip_access()?.epoch)()
            .ok_or("Unlock the profile to use saved linked queries.")?;
        Ok(Self {
            context,
            revision,
            epoch,
        })
    }
    pub fn scope(&self) -> String {
        format!("turnstone/{}", self.context.session.as_uuid())
    }
}

pub(crate) fn source_id(admission: &Admission) -> String {
    let key = &admission.key;
    let mut url = url::Url::parse(&format!(
        "mere://turnstone-capture/{}/{}/{}",
        key.session.as_uuid(),
        key.graph.as_uuid(),
        key.node
    ))
    .expect("fixed scheme and UUIDs");
    url.query_pairs_mut().append_pair("url", &admission.url);
    url.to_string()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    Catalog,
    Named(String),
    Run,
    Open {
        source: String,
        revision: String,
    },
    Bind(Vec<Key>),
    Preview(String),
    Share {
        domain: String,
        destination: String,
        session: String,
        expected: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub token: Uuid,
    pub stamp: Stamp,
    pub view: Option<SavedView>,
    pub operation: Operation,
}

pub struct OwnerRequest {
    pub(crate) request: Box<Request>,
    pub(crate) bridge: research::Bridge,
}

pub struct Response {
    pub token: Uuid,
    pub stamp: Stamp,
    pub result: Result<Outcome, String>,
}

#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub names: Vec<String>,
    pub active: Option<SavedView>,
    pub open: Option<crate::saved_sources::Target>,
    pub rows: Vec<SourceResult>,
    pub issues: Vec<String>,
    pub message: String,
    pub destination: Option<Destination>,
}

#[derive(Clone, Debug)]
pub struct Destination {
    pub domain: String,
    pub session: String,
    pub scope: String,
    pub count: usize,
    pub digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileAction {
    Import,
    Export(SavedView),
}

pub enum FileResult {
    Opened(SavedView),
    Saved,
}

pub(crate) fn export(view: &SavedView, path: &Path) -> Result<(), String> {
    let directory = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
    file.write_all(view.to_json()?.as_bytes())
        .map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn import(path: &Path) -> Result<SavedView, String> {
    let f = File::open(path).map_err(|e| e.to_string())?;
    let mut text = String::new();
    f.take(SavedView::MAX_BYTES as u64 + 1)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    SavedView::from_json(&text)
}

pub(crate) fn pick_file(
    action: FileAction,
    permitted: impl Fn() -> bool,
) -> Option<Result<FileResult, String>> {
    let dialog = rfd::FileDialog::new().add_filter("Saved linked query", &["json"]);
    let path = match &action {
        FileAction::Import => dialog
            .set_title("Open linked query definition")
            .pick_file()?,
        FileAction::Export(_) => dialog
            .set_title("Export linked query definition")
            .set_file_name("linked-query.json")
            .save_file()?,
    };
    if !permitted() {
        return Some(Err("Source access changed while choosing the file.".into()));
    }
    Some(match action {
        FileAction::Import => import(&path).map(FileResult::Opened),
        FileAction::Export(view) => export(&view, &path).map(|()| FileResult::Saved),
    })
}

#[derive(Default)]
pub(crate) struct State {
    pub name: String,
    pub query: Option<ViewQuery>,
    pub view: Option<SavedView>,
    pub pending: Option<Uuid>,
    pub accepted: Option<(Uuid, Stamp)>,
    pub outcome: Outcome,
    pub domain: String,
}
