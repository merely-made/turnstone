// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The host owns the gesture, document identity, destination and native scratch file.
//! The player tap runs in a private world of the clicked document. The outer page owns provenance.
mod frames;
use super::{
    CallDevToolsProtocolMethodCompletedHandler, CoTaskMemPWSTR, PlatformWebSurfaceProducer,
};
use crate::{
    action::PageMediaCommand,
    shell::{
        Shell,
        practice_audio::page_capture::{PageCapture, SCRIPT},
    },
    woodshed_client::{PagePhrase, connect_or_launch},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::HashMap,
    future::Future,
    path::Path,
    pin::Pin,
    rc::{Rc, Weak},
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};
use tokio::sync::oneshot;
use uuid::Uuid;

type Producer = RefCell<PlatformWebSurfaceProducer>;
#[derive(Clone)]
struct Page {
    producer: Weak<Producer>,
    frames: Rc<RefCell<frames::Frames>>,
}
thread_local! {
    // The factory is Send + Sync, but its WebView2 controllers are apartment-local.
    static PAGES: RefCell<HashMap<(u64, Uuid), Page>> = RefCell::new(HashMap::new());
}

pub(super) fn register(factory: u64, profile: &Path, page: &Rc<Producer>) {
    if let Some(member) = profile
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(|s| s.parse().ok())
    {
        let frames = frames::Frames::install(&page.borrow().webview()).unwrap_or_else(|e| {
            tracing::warn!(%e, "could not enable embedded player selection");
            Rc::default()
        });
        PAGES.with_borrow_mut(|pages| {
            pages.retain(|_, page| page.producer.strong_count() != 0);
            pages.insert(
                (factory, member),
                Page {
                    producer: Rc::downgrade(page),
                    frames,
                },
            );
        });
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Document {
    frame: String,
    loader: String,
    url: String,
}

impl Drop for super::TurnstoneScryProducer {
    fn drop(&mut self) {
        let producer = Rc::downgrade(&self.producer);
        PAGES.with_borrow_mut(|pages| {
            pages.retain(|_, page| !page.producer.ptr_eq(&producer));
        });
    }
}

async fn cdp_in(
    page: &Weak<Producer>,
    session: &str,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let (sender, result) = oneshot::channel();
    {
        let owner = page.upgrade().ok_or("The page was closed.")?;
        frames::call(
            &owner.borrow().webview(),
            session,
            method,
            params,
            move |r| {
                let _ = sender.send(r);
            },
        )?;
    }
    result.await.map_err(|_| "The page stopped responding.")?
}

async fn cdp(page: &Weak<Producer>, method: &str, params: Value) -> Result<Value, String> {
    cdp_in(page, "", method, params).await
}

fn field(value: &Value, name: &str) -> Result<String, String> {
    value
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("Missing browser media field {name}."))
}

async fn document(page: &Weak<Producer>) -> Result<Document, String> {
    let tree = cdp(page, "Page.getFrameTree", json!({})).await?;
    let frame = tree
        .pointer("/frameTree/frame")
        .ok_or("Missing top-level browser frame.")?;
    let url = field(frame, "url")?;
    let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("Open an HTTP or HTTPS page to mark its playing media.".into());
    }
    Ok(Document {
        frame: field(frame, "id")?,
        loader: field(frame, "loaderId")?,
        url,
    })
}

async fn evaluate(
    page: &Weak<Producer>,
    session: &str,
    object: &str,
    expression: String,
) -> Result<Value, String> {
    let reply = cdp_in(
        page,
        session,
        "Runtime.callFunctionOn",
        json!({
            "objectId":object,
            "functionDeclaration":format!("function() {{ return ({expression}); }}"),
            "returnByValue":true,"awaitPromise":true,
        }),
    )
    .await?;
    if reply.get("exceptionDetails").is_some() {
        return Err("The page changed or the browser could not capture this player.".into());
    }
    let value = reply
        .pointer("/result/value")
        .ok_or("The player did not return a result.")?;
    if value.get("ok") == Some(&Value::Bool(false)) {
        return Err(value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("The player refused capture.")
            .to_owned());
    }
    Ok(value.clone())
}

#[derive(Clone)]
struct Take {
    document: Document,
    session: String,
    object: String,
    title: String,
    phrase: Option<PagePhrase>,
}

struct MediaWake(crate::shell::ShellEventProxy);

impl Wake for MediaWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.0.send_event(crate::shell::ShellEvent::PageMediaReady);
    }
}

struct Work {
    waker: Waker,
    member: Uuid,
    page: Weak<Producer>,
    since: Instant,
    task: Pin<Box<dyn Future<Output = Result<(Option<Take>, String), String>>>>,
}

#[derive(Default)]
pub(in crate::shell) struct Practice {
    take: Option<(Uuid, Weak<Producer>, Take)>,
    work: Option<Work>,
}

fn source_title(raw: &str) -> String {
    let mut result = String::new();
    for c in raw.trim().chars().filter(|c| !c.is_control()) {
        if result.len() + c.len_utf8() > 160 {
            break;
        }
        result.push(c);
    }
    if result.trim().is_empty() {
        "Page audio".to_owned()
    } else {
        result
    }
}

async fn discard(page: &Weak<Producer>, take: &Take) {
    if document(page)
        .await
        .is_ok_and(|current| current == take.document)
    {
        let _ = evaluate(
            page,
            &take.session,
            &take.object,
            "globalThis.__turnstonePractice.cancel()".into(),
        )
        .await;
    }
}

async fn run(
    page: Weak<Producer>,
    frames: Rc<RefCell<frames::Frames>>,
    selected: Option<frames::Selection>,
    command: PageMediaCommand,
    take: Option<Take>,
) -> Result<(Option<Take>, String), String> {
    if command == PageMediaCommand::Cancel {
        let take = take.ok_or("This page has no audio marks to discard.")?;
        discard(&page, &take).await;
        return Ok((None, "Page audio marks discarded.".into()));
    }
    let doc = document(&page).await?;
    if command == PageMediaCommand::MarkIn {
        let (session, identity, token) = if let Some(selected) = selected {
            frames::verify_ancestors(&page, &frames, &selected, &doc).await?;
            (
                selected.session,
                json!({"uniqueContextId": selected.context}),
                Value::String(selected.token),
            )
        } else {
            let world = cdp(
                &page,
                "Page.createIsolatedWorld",
                json!({"frameId":doc.frame,"worldName":"Turnstone page practice"}),
            )
            .await?;
            (
                String::new(),
                json!({"contextId":world["executionContextId"]}),
                Value::Null,
            )
        };
        let _ = cdp_in(
            &page,
            &session,
            "Runtime.releaseObjectGroup",
            json!({"objectGroup":"turnstone.practice"}),
        )
        .await;
        let mut params = identity;
        params["expression"] = json!("globalThis");
        params["objectGroup"] = json!("turnstone.practice");
        let global = cdp_in(&page, &session, "Runtime.evaluate", params).await?;
        let object = global
            .pointer("/result/objectId")
            .and_then(Value::as_str)
            .ok_or("The isolated player context was closed.")?
            .to_owned();
        if document(&page).await? != doc {
            return Err("The page navigated. Mark in again.".into());
        }
        let result = evaluate(
            &page,
            &session,
            &object,
            if token.is_null() {
                format!("({SCRIPT}, globalThis.__turnstonePractice.markIn())")
            } else {
                format!("({SCRIPT}, globalThis.__turnstonePractice.markIn({token}))")
            },
        )
        .await?;
        if document(&page).await? != doc {
            return Err("The page navigated. Mark in again.".into());
        }
        let title = source_title(result["title"].as_str().unwrap_or("Page audio"));
        let status = format!("Recording {title}. Choose Mark out at the end of the phrase.");
        return Ok((
            Some(Take {
                document: doc,
                session,
                object,
                title,
                phrase: None,
            }),
            status,
        ));
    }
    let mut take = take.ok_or("Mark in on this page while a player is playing.")?;
    if doc != take.document {
        return Err("This page has changed. Mark in again.".into());
    }
    // The opaque object belongs to this precise frame document. Navigation or
    // replacement destroys it even if the new child has the same address.
    if evaluate(&page, &take.session, &take.object, "true".into())
        .await
        .is_err()
    {
        return Ok((
            None,
            "The selected player frame changed. Click the player and mark in again.".into(),
        ));
    }
    if command == PageMediaCommand::MarkOut {
        let metadata = evaluate(
            &page,
            &take.session,
            &take.object,
            "globalThis.__turnstonePractice.markOut()".into(),
        )
        .await?;
        let mut clip = PageCapture::new(&metadata)?;
        while let Some((offset, len)) = clip.next_chunk() {
            let chunk = evaluate(
                &page,
                &take.session,
                &take.object,
                format!("globalThis.__turnstonePractice.read({offset},{len})"),
            )
            .await?;
            clip.append(&chunk)?;
        }
        let (start_ms, end_ms) = (clip.start_ms, clip.end_ms);
        let phrase = clip.finish()?;
        if document(&page).await? != doc {
            return Err("This page changed before the phrase was complete.".into());
        }
        take.phrase = Some(PagePhrase {
            title: take.title.clone(),
            source_page_url: doc.url,
            start_ms,
            end_ms,
            note: String::new(),
            phrase,
        });
        return Ok((
            Some(take),
            format!(
                "Marked {:.3}–{:.3} s. Choose Send marked audio to Woodshed.",
                start_ms as f64 / 1000.,
                end_ms as f64 / 1000.
            ),
        ));
    }
    let phrase = take
        .phrase
        .clone()
        .ok_or("Mark out before sending the audio to Woodshed.")?;
    let (done, result) = oneshot::channel();
    std::thread::Builder::new()
        .name("page-audio-to-woodshed".into())
        .spawn(move || {
            let ack = connect_or_launch(None)
                .and_then(|mut session| session.import_page_phrase(&phrase))
                .map(|_| ())
                .map_err(|e| e.to_string());
            let _ = done.send(ack);
        })
        .map_err(|e| e.to_string())?;
    result.await.map_err(|_| "Woodshed handoff stopped.")??;
    // A successful send belongs to the persistent Woodshed loop. Release the native
    // temporary and pause only the original page element if it is still there.
    if document(&page).await.is_ok_and(|current| current == doc) {
        let _ = evaluate(
            &page,
            &take.session,
            &take.object,
            "globalThis.__turnstonePractice.pauseSentPlayer()".into(),
        )
        .await;
    }
    Ok((None, "Marked audio saved and looping in Woodshed.".into()))
}

impl Shell {
    pub(in crate::shell) fn forget_page_media(&mut self, member: Uuid) {
        if let Some(factory) = self.scry_factory.as_ref() {
            PAGES.with_borrow(|pages| {
                if let Some(p) = pages.get(&(factory.id, member)) {
                    tracing::debug!(%member, "resetting player selection on navigation");
                    p.frames.borrow_mut().selected = None;
                }
            });
        }
        if self
            .page_media
            .work
            .as_ref()
            .is_some_and(|work| work.member == member)
        {
            self.page_media.work = None;
        }
        if self
            .page_media
            .take
            .as_ref()
            .is_some_and(|(node, _, _)| *node == member)
        {
            self.page_media.take = None;
        }
        if self
            .app
            .page_media_status
            .as_ref()
            .is_some_and(|(node, _, _)| *node == member)
        {
            self.app.page_media_status = None;
        }
    }

    pub(in crate::shell) fn page_media_command(&mut self, member: Uuid, command: PageMediaCommand) {
        let page = self.scry_factory.as_ref().and_then(|factory| {
            PAGES.with_borrow(|pages| {
                pages
                    .get(&(factory.id, member))
                    .filter(|p| p.producer.strong_count() > 0)
                    .cloned()
            })
        });
        let message = if let Some(entry) = page {
            let page = entry.producer;
            if self.page_media.work.is_some() {
                "Waiting for the previous page audio action."
            } else {
                let previous = self
                    .page_media
                    .take
                    .as_ref()
                    .filter(|(node, owner, _)| *node == member && Weak::ptr_eq(owner, &page))
                    .map(|(_, _, take)| take.clone());
                let replaced = if command == PageMediaCommand::MarkIn {
                    self.page_media.take.take()
                } else {
                    None
                };
                let target = page.clone();
                let selected = entry.frames.borrow().selected.clone();
                self.page_media.work = Some(Work {
                    waker: Waker::from(Arc::new(MediaWake(self.proxy.clone()))),
                    member,
                    page,
                    since: Instant::now(),
                    task: Box::pin(async move {
                        if let Some((_, old_page, take)) = replaced {
                            discard(&old_page, &take).await;
                        }
                        run(target, entry.frames, selected, command, previous).await
                    }),
                });
                match command {
                    PageMediaCommand::MarkIn => "Starting the audio mark…",
                    PageMediaCommand::MarkOut => "Preparing the marked audio…",
                    PageMediaCommand::Cancel => "Discarding the audio marks...",
                    PageMediaCommand::Send => "Sending the marked audio to Woodshed…",
                }
            }
        } else {
            "Open the page with Scry to mark its playing media."
        };
        self.page_media_notice(member, message.into());
        self.poll_page_media();
    }

    fn page_media_notice(&mut self, member: Uuid, message: String) {
        tracing::info!(%member, %message,"page practice");
        let next = if self.page_media.work.is_some() {
            None
        } else {
            Some(
                self.page_media
                    .take
                    .as_ref()
                    .filter(|(node, _, _)| *node == member)
                    .map_or(PageMediaCommand::MarkIn, |(_, _, take)| {
                        if take.phrase.is_some() {
                            PageMediaCommand::Send
                        } else {
                            PageMediaCommand::MarkOut
                        }
                    }),
            )
        };
        self.app.page_media_status = Some((member, message, next));
        self.request_redraw();
    }

    pub(in crate::shell) fn poll_page_media(&mut self) {
        if self
            .page_media
            .take
            .as_ref()
            .is_some_and(|(_, owner, _)| owner.strong_count() == 0)
        {
            self.page_media.take = None;
            self.app.page_media_status = None;
        }
        let Some(work) = self.page_media.work.as_mut() else {
            return;
        };
        let result = if work.page.strong_count() == 0 {
            Err("The page was closed.".into())
        } else if work.since.elapsed() > Duration::from_secs(45) {
            Err("The page audio action timed out.".into())
        } else {
            match work
                .task
                .as_mut()
                .poll(&mut Context::from_waker(&work.waker))
            {
                Poll::Pending => return,
                Poll::Ready(result) => result,
            }
        };
        let work = self.page_media.work.take().unwrap();
        let message = match result {
            Ok((take, message)) => {
                self.page_media.take = take.map(|take| (work.member, work.page, take));
                message
            },
            Err(message) => message,
        };
        self.page_media_notice(work.member, message);
    }
}
