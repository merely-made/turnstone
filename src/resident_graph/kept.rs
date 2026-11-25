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
use graphshell::goal_surface::{self, GoalSurface, Start};
use graphshell_door::projection::AppBrokerClient;
use graphshell_host::goal_surface::{RetainedPage, RetainedPages, door, source_id};
use sceno::InstanceId;
use scenotime::{Revision, SceneEpoch};
use std::rc::Rc;

use super::{GraphView, State, worker};

const RESULT_LIMIT: usize = 20;
const SNAPSHOT_OWNER: &str = "turnstone-resident";

pub(super) const CSS: &str = "
.resident-kept { box-sizing: border-box; padding: 8px; }
.resident-kept h2, .resident-kept h3, .resident-kept p { margin: 4px 0; }
.resident-kept label, .resident-kept li { display: block; margin: 6px 0; }
.resident-kept .kept-input { display: block; min-height: 25px; width: 100%; background: var(--tabard-color-surface, #22262c); border: 1px solid #6d7783; }
.resident-kept p, .resident-kept pre { overflow-wrap: anywhere; white-space: pre-wrap; }
.resident-kept ul { padding-left: 0; }
.resident-kept .goal-input { min-width: 0; max-width: 100%; box-sizing: border-box; background: var(--tabard-color-surface, #22262c); color: inherit; }
.resident-kept .goal-cite { background: var(--tabard-color-surface, #22262c); }
.resident-kept .goal-meta { color: inherit; }
.resident-kept .goal-notice, .resident-kept .goal-step-failed { color: var(--tabard-color-text, rgb(220, 226, 238)); }
";

pub(super) enum Request {
    Refresh,
    SetKeep(bool),
    Capture(String),
}

pub(super) struct Reply {
    pages: Result<KeptReading, String>,
    notice: Option<String>,
}

struct KeptReading {
    pages: Option<RetainedPages>,
    keeping: RetainedKeeping,
    partial: bool,
}

impl KeptReading {
    fn from_wire(wire: RetainedPagesResponse) -> Result<Self, String> {
        let pages = if matches!(&wire.keeping, RetainedKeeping::Paused { .. }) {
            None
        } else {
            Some(door::from_wire(SNAPSHOT_OWNER, &wire)?)
        };
        Ok(Self {
            pages,
            keeping: wire.keeping,
            partial: !wire.truncated.is_complete(),
        })
    }
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
                chirograph::wire::encode(&KeepEnabledV1 { enabled }).expect("keep setting encodes");
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
) -> Result<KeptReading, String> {
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
    KeptReading::from_wire(*pages)
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
    let payload = chirograph::wire::encode(&KeepPagesV1 {
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
    traversal: Option<GoalSurface>,
    follow_links: bool,
    keeping: Option<RetainedKeeping>,
    partial: bool,
    notice: String,
}

impl Panel {
    pub(super) fn apply(&mut self, reply: &Reply) {
        self.notice = reply.notice.clone().unwrap_or_default();
        self.keeping = None;
        self.partial = false;
        match &reply.pages {
            Ok(reading) => {
                self.keeping = Some(reading.keeping.clone());
                self.partial = reading.partial;
                if let Some(pages) = &reading.pages {
                    self.replace_pages(pages);
                } else {
                    self.traversal = None;
                }
            },
            Err(reason) => {
                self.traversal = None;
                self.notice = reason.clone();
            },
        }

        self.hits = self
            .pages()
            .map_or_else(Vec::new, |pages| search(pages, &self.submitted));
        if self.reading().is_none() {
            self.selected = None;
        }
    }

    pub(super) fn reset_scope(&mut self) {
        *self = Self {
            open: self.open,
            ..Self::default()
        };
    }

    fn replace_pages(&mut self, pages: &RetainedPages) {
        let Some(surface) = &mut self.traversal else {
            let mut surface = GoalSurface::new(Rc::new(pages.clone()));
            surface.heading = false;
            self.traversal = Some(surface);
            return;
        };
        if pages == surface.pages() {
            return;
        }

        if let Start::Source(index) = surface.start {
            let old_sources = surface.pages().sources();
            surface.start = old_sources
                .get(index)
                .and_then(|(chosen, _)| {
                    pages
                        .sources()
                        .iter()
                        .position(|(source, _)| source == chosen)
                })
                .map_or(Start::All, Start::Source);
        }
        surface.run = None;
        surface.receipt = None;
        surface.replace_pages(Rc::new(pages.clone()));
    }

    fn pages(&self) -> Option<&RetainedPages> {
        self.traversal.as_ref().map(GoalSurface::pages)
    }

    fn page(&self, id: &str) -> Option<&RetainedPage> {
        let pages = self.pages()?;
        pages.page(&source_id(pages.owner(), id.parse().ok()?))
    }

    fn reading(&self) -> Option<(&RetainedPage, &str)> {
        let (id, version) = self.selected.as_ref()?;
        let page = self.page(id)?;
        Some((page, page.versions.get(version)?))
    }

    fn keeping(&self) -> Option<&RetainedKeeping> {
        self.keeping.as_ref()
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

fn search(pages: &RetainedPages, query: &str) -> Vec<DocumentHit<String>> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    DocumentIndex::from_documents(
        Default::default(),
        pages
            .sources()
            .into_iter()
            .map(|(source, page)| SearchDocument {
                key: source.source,
                primary_address: page.url.clone(),
                aliases: Vec::new(),
                title: page.title.clone(),
                body: page.versions.get(&page.current).cloned(),
            }),
    )
    .search(query, RESULT_LIMIT)
}

fn search_view(state: &State) -> GraphView {
    let panel = &state.kept;
    let busy = state.requested != state.completed;
    let rows: Vec<GraphView> = panel
        .hits
        .iter()
        .filter_map(|hit| {
            let page = panel.page(&hit.key)?;
            let chosen = (hit.key.clone(), page.current.clone());
            let title = page.title.clone().unwrap_or_else(|| page.url.clone());
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
                    .attr("data-kept-page", hit.key.clone()),
                    page.title
                        .as_ref()
                        .filter(|title| *title != &page.url)
                        .map(|_| el("p", page.url.clone())),
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
    Box::new(el(
        "div",
        (
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
            el("p", summary)
                .attr("role", "status")
                .attr("aria-label", "Kept page matches"),
            el("ul", rows),
        ),
    ))
}

pub(super) fn view(state: &State) -> GraphView {
    let panel = &state.kept;
    if let Some((page, text)) = panel.reading() {
        return Box::new(
            el::<_, State, ()>(
                "article",
                (
                    button("Back to kept pages", |state: &mut State, _| {
                        state.kept.selected = None;
                    }),
                    el("h2", page.title.clone().unwrap_or_else(|| page.url.clone())),
                    page.title
                        .as_ref()
                        .filter(|title| *title != &page.url)
                        .map(|_| el("p", page.url.clone())),
                    el("pre", text.to_owned()),
                ),
            )
            .attr("aria-label", "Kept page text")
            .attr("class", "resident-kept"),
        );
    }
    let can_set = matches!(
        panel.keeping(),
        Some(RetainedKeeping::On | RetainedKeeping::Off)
    );
    let on = matches!(panel.keeping(), Some(RetainedKeeping::On));
    let busy = state.requested != state.completed;
    let finding: Option<GraphView> = if panel.follow_links {
        panel.traversal.is_some().then(|| {
            Box::new(lens(
                |surface: &mut GoalSurface| goal_surface::view(surface),
                |state: &mut State| {
                    state
                        .kept
                        .traversal
                        .as_mut()
                        .expect("visible kept traversal")
                },
            )) as GraphView
        })
    } else {
        Some(search_view(state))
    };
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
                    "nav",
                    (
                        button("Search text", |state: &mut State, _| {
                            state.kept.follow_links = false;
                        })
                        .attr("aria-pressed", (!panel.follow_links).to_string()),
                        button("Follow kept links", |state: &mut State, _| {
                            state.kept.follow_links = true;
                        })
                        .attr("aria-pressed", panel.follow_links.to_string()),
                    ),
                )
                .attr("aria-label", "Find kept pages"),
                panel.partial.then(|| {
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
                finding,
            ),
        )
        .attr("class", "resident-kept"),
    )
}

#[cfg(test)]
mod tests;
