// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Kept text on the graph projection of the pane's admitted resident route.

use cambium::{TextInput, button, el, lens, text_field_typed};
use chirograph::{
    CarrierRequestBody, CarrierResponseBody, IntentInvocation, IntentResult, KEEP_ENABLED_INTENT,
    KeepEnabledV1, ProjectionSession, RetainedKeeping, RetainedPagesRequest, RetainedPagesResponse,
};
use eidetic::kept_text::{KEEP_PAGES_INTENT, KeepPagesV1};
use eidetic_search::{DocumentHit, DocumentIndex, SearchDocument};
use graphshell_door::projection::AppBrokerClient;
use sceno::InstanceId;
use scenotime::{Revision, SceneEpoch};
use std::sync::Arc;

use super::{GraphView, State, worker};

const RESULT_LIMIT: usize = 20;

pub(super) const CSS: &str = "
.resident-kept { box-sizing: border-box; overflow: auto; padding: 8px; }
.resident-kept h2, .resident-kept h3, .resident-kept p { margin: 4px 0; }
.resident-kept label, .resident-kept li { display: block; margin: 6px 0; }
.resident-kept .kept-input { display: block; min-height: 25px; width: 100%; background: var(--tabard-color-surface, #22262c); border: 1px solid #6d7783; }
.resident-kept p, .resident-kept pre { overflow-wrap: anywhere; white-space: pre-wrap; }
.resident-kept ul { padding-left: 0; }
";

pub(super) enum Request {
    Refresh,
    SetKeep(bool),
    Capture(String),
}

pub(super) struct Reply {
    pages: Result<RetainedPagesResponse, String>,
    notice: Option<String>,
}

pub(super) async fn run(
    client: &mut AppBrokerClient,
    graph: Option<&ProjectionSession>,
    request: Request,
) -> Reply {
    let Some(graph) = graph else {
        return Reply {
            pages: Err("This mere offers no attached graph.".into()),
            notice: None,
        };
    };
    let notice = match request {
        Request::Refresh => None,
        Request::SetKeep(enabled) => {
            let payload =
                serde_json::to_vec(&KeepEnabledV1 { enabled }).expect("keep setting encodes");
            Some(
                send(client, graph, KEEP_ENABLED_INTENT, payload)
                    .await
                    .map_or_else(
                        |reason| reason,
                        |()| {
                            if enabled {
                                "Keeping page text is on.".into()
                            } else {
                                "Keeping page text is off.".into()
                            }
                        },
                    ),
            )
        },
        Request::Capture(address) => Some(
            capture(client, graph, address)
                .await
                .unwrap_or_else(|reason| reason),
        ),
    };
    Reply {
        pages: read(client, graph).await,
        notice,
    }
}

async fn read(
    client: &mut AppBrokerClient,
    graph: &ProjectionSession,
) -> Result<RetainedPagesResponse, String> {
    let response = client
        .request_body(CarrierRequestBody::RetainedPages(RetainedPagesRequest {
            session: graph.clone(),
            limits: Default::default(),
        }))
        .await
        .map_err(|error| error.to_string())?;
    let CarrierResponseBody::RetainedPages(pages) = response else {
        return Err("The resident did not return kept pages.".into());
    };
    if pages.session != *graph {
        return Err("The resident returned kept pages for another graph.".into());
    }
    graphshell_host::goal_surface::door::from_wire("turnstone-resident", &pages)?;
    Ok(*pages)
}

async fn capture(
    client: &mut AppBrokerClient,
    graph: &ProjectionSession,
    address: String,
) -> Result<String, String> {
    match read(client, graph).await?.keeping {
        RetainedKeeping::On => {},
        RetainedKeeping::Off => {
            return Err("Turn on Keep page text before capturing a page.".into());
        },
        RetainedKeeping::Paused { reason } => {
            return Err(format!("Keeping page text is paused. {reason}"));
        },
    }
    let (answer, finished) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("turnstone-keep-page".into())
        .spawn(move || {
            let _ = answer.send(fetch::capture_kept_page(&address));
        })
        .map_err(|error| error.to_string())?;
    let (page, record) = finished.await.map_err(|_| "The page fetch stopped.")??;
    let address = page.url.clone();
    let payload = serde_json::to_vec(&KeepPagesV1 {
        pages: vec![page],
        fetches: vec![record],
    })
    .expect("kept page encodes");
    send(client, graph, KEEP_PAGES_INTENT, payload).await?;
    Ok(format!("Kept the text of {address}."))
}

async fn send(
    client: &mut AppBrokerClient,
    graph: &ProjectionSession,
    intent: &str,
    payload: Vec<u8>,
) -> Result<(), String> {
    let response = client
        .request_body(CarrierRequestBody::Intent(IntentInvocation {
            session: graph.clone(),
            target: InstanceId(0),
            observed_epoch: SceneEpoch(0),
            observed_revision: Revision(0),
            intent: intent.into(),
            payload,
        }))
        .await
        .map_err(|error| error.to_string())?;
    match response {
        CarrierResponseBody::Intent(IntentResult::Accepted) => Ok(()),
        CarrierResponseBody::Intent(IntentResult::Rejected { reason }) => Err(reason),
        CarrierResponseBody::Intent(IntentResult::Stale { .. }) => {
            Err("The resident moved on. Try again.".into())
        },
        _ => Err("The resident did not answer the keep request.".into()),
    }
}

#[derive(Default)]
pub(super) struct Panel {
    pub open: bool,
    pub address: TextInput,
    query: TextInput,
    submitted: String,
    hits: Vec<DocumentHit<String>>,
    selected: Option<(String, String)>,
    reply: Option<Arc<Reply>>,
    notice: String,
}

impl Panel {
    pub(super) fn apply(&mut self, reply: Arc<Reply>) {
        self.notice = reply.notice.clone().unwrap_or_default();
        if let Err(reason) = &reply.pages {
            self.notice = reason.clone();
        }
        self.reply = Some(reply);
        self.hits = self
            .pages()
            .map_or_else(Vec::new, |pages| search(pages, &self.submitted));
        if self.reading().is_none() {
            self.selected = None;
        }
    }

    fn pages(&self) -> Option<&RetainedPagesResponse> {
        self.reply.as_ref()?.pages.as_ref().ok()
    }

    fn reading(&self) -> Option<(&chirograph::RetainedPageWire, &str)> {
        let (id, version) = self.selected.as_ref()?;
        let page = self.pages()?.pages.iter().find(|page| page.id == *id)?;
        let text = page.versions.iter().find(|held| held.version == *version)?;
        Some((page, &text.text))
    }

    fn keeping(&self) -> Option<&RetainedKeeping> {
        self.pages().map(|pages| &pages.keeping)
    }

    fn status(&self) -> String {
        match self.keeping() {
            Some(RetainedKeeping::On) => "Keeping page text is on.".into(),
            Some(RetainedKeeping::Off) => "Keeping page text is off.".into(),
            Some(RetainedKeeping::Paused { reason }) => {
                format!("Keeping page text is paused. {reason}")
            },
            None => "Kept pages are unavailable.".into(),
        }
    }
}

fn current(page: &chirograph::RetainedPageWire) -> Option<&str> {
    page.versions
        .iter()
        .find(|version| version.version == page.current)
        .map(|version| version.text.as_str())
}

fn search(pages: &RetainedPagesResponse, query: &str) -> Vec<DocumentHit<String>> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    DocumentIndex::from_documents(
        Default::default(),
        pages.pages.iter().map(|page| SearchDocument {
            key: page.id.clone(),
            primary_address: page.address.clone(),
            aliases: Vec::new(),
            title: page.title.clone(),
            body: current(page).map(str::to_owned),
        }),
    )
    .search(query, RESULT_LIMIT)
}

pub(super) fn view(state: &State) -> GraphView {
    let panel = &state.kept;
    let scroll = format!("height: {}px", state.size.1.saturating_sub(32));
    if let Some((page, text)) = panel.reading() {
        return Box::new(
            el::<_, State, ()>(
                "article",
                (
                    button("Back to kept pages", |state: &mut State, _| {
                        state.kept.selected = None;
                    }),
                    el(
                        "h2",
                        page.title.clone().unwrap_or_else(|| page.address.clone()),
                    ),
                    page.title
                        .as_ref()
                        .filter(|title| *title != &page.address)
                        .map(|_| el("p", page.address.clone())),
                    el("pre", text.to_owned()),
                ),
            )
            .attr("aria-label", "Kept page text")
            .attr("class", "resident-kept")
            .attr("style", scroll),
        );
    }
    let can_set = matches!(
        panel.keeping(),
        Some(RetainedKeeping::On | RetainedKeeping::Off)
    );
    let on = matches!(panel.keeping(), Some(RetainedKeeping::On));
    let busy = state.requested != state.completed;
    let rows: Vec<GraphView> = panel
        .hits
        .iter()
        .filter_map(|hit| {
            let page = panel
                .pages()?
                .pages
                .iter()
                .find(|page| page.id == hit.key)?;
            let chosen = (page.id.clone(), page.current.clone());
            let title = page.title.clone().unwrap_or_else(|| page.address.clone());
            Some(Box::new(el(
                "li",
                (
                    button(
                        format!("Read kept page: {title}"),
                        move |state: &mut State, _| {
                            if state.requested == state.completed {
                                state.kept.selected = Some(chosen.clone());
                            }
                        },
                    )
                    .attr("data-kept-page", page.id.clone()),
                    page.title
                        .as_ref()
                        .filter(|title| *title != &page.address)
                        .map(|_| el("p", page.address.clone())),
                ),
            )) as GraphView)
        })
        .collect();
    let summary = match rows.len() {
        _ if panel.submitted.is_empty() || busy => String::new(),
        0 if panel.pages().is_none() => String::new(),
        0 => "No kept pages matched.".into(),
        1 => "1 match".into(),
        count if count == RESULT_LIMIT => format!("Showing {count} matches"),
        count => format!("{count} matches"),
    };
    let partial = panel
        .pages()
        .is_some_and(|pages| !pages.truncated.is_complete());
    Box::new(
        el::<_, State, ()>(
            "section",
            (
                el("h2", "Kept pages"),
                el("p", panel.status())
                    .attr("role", "status")
                    .attr("aria-label", "Keeping state"),
                el(
                    "nav",
                    (
                        button("Refresh kept pages", |state: &mut State, _| {
                            state.send(worker::Request::Kept(Request::Refresh));
                        }),
                        can_set.then(|| {
                            button(
                                if on {
                                    "Stop keeping page text"
                                } else {
                                    "Keep page text"
                                },
                                move |state: &mut State, _| {
                                    state.send(worker::Request::Kept(Request::SetKeep(!on)));
                                },
                            )
                        }),
                    ),
                ),
                el(
                    "label",
                    (
                        "Page address",
                        lens(
                            |input: &mut TextInput| {
                                text_field_typed(input)
                                    .attr("aria-label", "Page address")
                                    .attr("class", "kept-input")
                            },
                            |state: &mut State| &mut state.kept.address,
                        ),
                    ),
                ),
                on.then(|| {
                    button("Keep this page's text", |state: &mut State, _| {
                        let address = state.kept.address.text().trim().to_owned();
                        if address.is_empty() {
                            state.kept.notice = "Enter a page address.".into();
                        } else {
                            state.send(worker::Request::Kept(Request::Capture(address)));
                        }
                    })
                }),
                el(
                    "label",
                    (
                        "Search query",
                        lens(
                            |input: &mut TextInput| {
                                text_field_typed(input)
                                    .attr("aria-label", "Search query")
                                    .attr("class", "kept-input")
                            },
                            |state: &mut State| &mut state.kept.query,
                        ),
                    ),
                ),
                button("Search kept pages", |state: &mut State, _| {
                    let query = state.kept.query.text().trim().to_owned();
                    if query.is_empty() {
                        state.kept.notice = "Enter a search query.".into();
                    } else if state.send(worker::Request::Kept(Request::Refresh)) {
                        state.kept.submitted = query;
                        state.kept.hits.clear();
                        state.kept.selected = None;
                    }
                }),
                partial.then(|| {
                    el(
                        "p",
                        "The resident's read limit excluded some kept material.",
                    )
                }),
                el(
                    "p",
                    if busy {
                        "Waiting for the resident…".into()
                    } else {
                        panel.notice.clone()
                    },
                )
                .attr("role", "status")
                .attr("aria-label", "Kept page status"),
                el("p", summary)
                    .attr("role", "status")
                    .attr("aria-label", "Kept page matches"),
                el("ul", rows),
            ),
        )
        .attr("class", "resident-kept")
        .attr("style", scroll),
    )
}
