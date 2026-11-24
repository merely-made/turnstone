// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Private selection listener in each frame of this WebView. Chromium puts
//! cross-site frames in separate renderer targets; WebView2 delivers those
//! targets' events on the same UI thread, tagged with the target session.
use super::*;
use webview2_com::{
    DevToolsProtocolEventReceivedEventHandler,
    Microsoft::Web::WebView2::Win32::{
        ICoreWebView2, ICoreWebView2_11, ICoreWebView2DevToolsProtocolEventReceivedEventArgs2,
        ICoreWebView2DevToolsProtocolEventReceiver,
    },
};
use windows::core::{Interface, PWSTR};

const WORLD: &str = "Turnstone page practice";
const BINDING: &str = "__turnstoneSelectPlayer";
const LISTENER: &str = include_str!("../../practice_audio/page_selection.js");

#[derive(Clone)]
pub(super) struct Selection {
    pub session: String,
    pub context: String,
    pub frame: String,
    pub token: String,
}

#[derive(Default)]
pub(super) struct Frames {
    subscriptions: Vec<(ICoreWebView2DevToolsProtocolEventReceiver, i64)>,
    contexts: HashMap<(String, i64), (String, String)>,
    parents: HashMap<String, String>,
    pub selected: Option<Selection>,
}

impl Drop for Frames {
    fn drop(&mut self) {
        for (receiver, token) in self.subscriptions.drain(..) {
            unsafe {
                let _ = receiver.remove_DevToolsProtocolEventReceived(token);
            }
        }
    }
}

pub(super) fn call(
    web: &ICoreWebView2,
    session: &str,
    method: &str,
    params: Value,
    done: impl FnOnce(Result<Value, String>) + 'static,
) -> Result<(), String> {
    let mut done = Some(done);
    let callback = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
        move |status: windows::core::Result<()>, reply: String| {
            if let Some(done) = done.take() {
                done(status.map_err(|e| e.to_string()).and_then(|_| {
                    let reply: Value = serde_json::from_str(&reply).map_err(|e| e.to_string())?;
                    if reply.get("error").is_some() {
                        Err("The browser frame changed or is unavailable.".into())
                    } else {
                        Ok(reply)
                    }
                }));
            }
            Ok(())
        },
    ));
    let method = CoTaskMemPWSTR::from(method);
    let params = CoTaskMemPWSTR::from(params.to_string().as_str());
    // The WebView retains callback. All COM invocations stay on its UI apartment.
    unsafe {
        if session.is_empty() {
            web.CallDevToolsProtocolMethod(
                *method.as_ref().as_pcwstr(),
                *params.as_ref().as_pcwstr(),
                &callback,
            )
        } else {
            let sid = CoTaskMemPWSTR::from(session);
            let extended: ICoreWebView2_11 = web.cast().map_err(|e| e.to_string())?;
            extended.CallDevToolsProtocolMethodForSession(
                *sid.as_ref().as_pcwstr(),
                *method.as_ref().as_pcwstr(),
                *params.as_ref().as_pcwstr(),
                &callback,
            )
        }
    }
    .map_err(|e| e.to_string())
}

fn configure(web: &ICoreWebView2, session: &str) {
    for (method, params) in [
        ("Page.enable", json!({})),
        ("Runtime.enable", json!({})),
        (
            "Runtime.addBinding",
            json!({"name":BINDING,"executionContextName":WORLD}),
        ),
        (
            "Page.addScriptToEvaluateOnNewDocument",
            json!({"worldName":WORLD,"source":LISTENER,"runImmediately":true}),
        ),
        (
            "Target.setAutoAttach",
            json!({"autoAttach":true,"flatten":true,"waitForDebuggerOnStart":false,
            "filter":[{"type":"iframe"},{"exclude":true}]}),
        ),
    ] {
        if let Err(e) = call(web, session, method, params, move |reply| {
            if let Err(e) = reply {
                tracing::warn!(%method, %e, "page player selection setup");
            }
        }) {
            tracing::warn!(%method, %e, "page player selection setup");
        }
    }
}

// Runtime.enable announces existing documents and future same-process child
// navigations. The default world only identifies the frame; the listener is
// evaluated exclusively in its named isolated world, using a non-recycled ID.
fn initialize_context(web: &ICoreWebView2, session: &str, context: &Value) {
    let kind = context.pointer("/auxData/type").and_then(Value::as_str);
    let (method, params) = match kind {
        Some("default") => {
            let Some(frame) = context.pointer("/auxData/frameId").and_then(Value::as_str) else {
                return;
            };
            (
                "Page.createIsolatedWorld",
                json!({"frameId":frame,"worldName":WORLD}),
            )
        },
        Some("isolated") if context["name"].as_str() == Some(WORLD) => {
            let Some(unique) = context["uniqueId"].as_str() else {
                return;
            };
            (
                "Runtime.evaluate",
                json!({"uniqueContextId":unique,"expression":LISTENER}),
            )
        },
        _ => return,
    };
    let _ = call(web, session, method, params, move |result| {
        if let Err(e) = result {
            tracing::debug!(%method, %e, "initializing frame player selection");
        }
    });
}

impl Frames {
    pub fn install(web: &ICoreWebView2) -> windows::core::Result<Rc<RefCell<Self>>> {
        let state = Rc::new(RefCell::new(Self::default()));
        for event in [
            "Runtime.bindingCalled",
            "Runtime.executionContextCreated",
            "Runtime.executionContextDestroyed",
            "Runtime.executionContextsCleared",
            "Target.attachedToTarget",
            "Target.detachedFromTarget",
        ] {
            let name = CoTaskMemPWSTR::from(event);
            let receiver =
                unsafe { web.GetDevToolsProtocolEventReceiver(*name.as_ref().as_pcwstr())? };
            let weak = Rc::downgrade(&state);
            let handler =
                DevToolsProtocolEventReceivedEventHandler::create(Box::new(move |web, args| {
                    let (Some(web), Some(args), Some(state)) = (web, args, weak.upgrade()) else {
                        return Ok(());
                    };
                    let session_args: ICoreWebView2DevToolsProtocolEventReceivedEventArgs2 =
                        args.cast()?;
                    let mut id = PWSTR::null();
                    unsafe {
                        session_args.SessionId(&mut id)?;
                    }
                    let session = CoTaskMemPWSTR::from(id).to_string();
                    let mut body = PWSTR::null();
                    unsafe {
                        args.ParameterObjectAsJson(&mut body)?;
                    }
                    if let Ok(data) = serde_json::from_str(&CoTaskMemPWSTR::from(body).to_string())
                    {
                        let attached = state.borrow_mut().event(event, &session, &data);
                        if event == "Runtime.executionContextCreated"
                            && (session.is_empty() || state.borrow().parents.contains_key(&session))
                        {
                            initialize_context(&web, &session, &data["context"]);
                        }
                        if let Some(new_session) = attached {
                            configure(&web, &new_session);
                        }
                    }
                    Ok(())
                }));
            let mut token = 0;
            unsafe {
                receiver.add_DevToolsProtocolEventReceived(&handler, &mut token)?;
            }
            state.borrow_mut().subscriptions.push((receiver, token));
        }
        configure(web, "");
        Ok(state)
    }

    fn event(&mut self, event: &str, session: &str, data: &Value) -> Option<String> {
        tracing::debug!(%event, child = !session.is_empty(), world = ?data.pointer("/context/name"), "page player event");
        // Ignore targets attached by other DevTools consumers or non-iframe targets.
        if !session.is_empty() && !self.parents.contains_key(session) {
            return None;
        }
        match event {
            "Target.attachedToTarget"
                if data.pointer("/targetInfo/type")?.as_str()? == "iframe" =>
            {
                let child = data["sessionId"].as_str()?.to_owned();
                self.parents.insert(child.clone(), session.to_owned());
                return Some(child);
            },
            "Target.detachedFromTarget" => {
                let child = data["sessionId"].as_str()?;
                self.parents.remove(child);
                self.contexts.retain(|(s, _), _| s != child);
            },
            "Runtime.executionContextCreated" => {
                let context = &data["context"];
                if context["name"].as_str() == Some(WORLD)
                    && context.pointer("/auxData/type").and_then(Value::as_str) == Some("isolated")
                {
                    self.contexts.insert(
                        (session.to_owned(), context["id"].as_i64()?),
                        (
                            context["uniqueId"].as_str()?.to_owned(),
                            context.pointer("/auxData/frameId")?.as_str()?.to_owned(),
                        ),
                    );
                }
            },
            "Runtime.executionContextDestroyed" => {
                self.contexts
                    .remove(&(session.to_owned(), data["executionContextId"].as_i64()?));
            },
            "Runtime.executionContextsCleared" => {
                self.contexts.retain(|(s, _), _| s != session);
            },
            "Runtime.bindingCalled" if data["name"].as_str()? == BINDING => {
                let token = data["payload"].as_str()?;
                let (context, frame) = self
                    .contexts
                    .get(&(session.to_owned(), data["executionContextId"].as_i64()?))?;
                if token.len() == 32 && token.bytes().all(|c| c.is_ascii_hexdigit()) {
                    tracing::debug!(
                        child = !session.is_empty(),
                        "selected clicked practice player"
                    );
                    self.selected = Some(Selection {
                        session: session.to_owned(),
                        context: context.clone(),
                        frame: frame.clone(),
                        token: token.to_owned(),
                    });
                }
            },
            _ => {},
        }
        None
    }
}

fn find_frame<'a>(tree: &'a Value, frame: &str) -> Option<&'a Value> {
    if tree.pointer("/frame/id")?.as_str()? == frame {
        return tree.get("frame");
    }
    tree.get("childFrames")?
        .as_array()?
        .iter()
        .find_map(|child| find_frame(child, frame))
}

pub(super) async fn verify_ancestors(
    page: &Weak<Producer>,
    frames: &RefCell<Frames>,
    selection: &Selection,
    top: &Document,
) -> Result<(), String> {
    let mut session = selection.session.clone();
    let mut frame = selection.frame.clone();
    while !session.is_empty() || frame != top.frame {
        let tree = cdp_in(page, &session, "Page.getFrameTree", json!({})).await?;
        let tree = &tree["frameTree"];
        let doc = find_frame(tree, &frame)
            .ok_or("The selected frame has changed. Click the player again.")?;
        let parent = field(doc, "parentId")?;
        if tree.pointer("/frame/id").and_then(Value::as_str) == Some(&frame) {
            session = frames
                .borrow()
                .parents
                .get(&session)
                .cloned()
                .ok_or("The embedded player was closed.")?;
        }
        let owner = cdp_in(
            page,
            &session,
            "DOM.getFrameOwner",
            json!({"frameId":frame}),
        )
        .await?;
        let node = cdp_in(
            page,
            &session,
            "DOM.describeNode",
            json!({"backendNodeId":owner["backendNodeId"]}),
        )
        .await?;
        if let Some(attrs) = node.pointer("/node/attributes").and_then(Value::as_array) {
            for attr in attrs.chunks_exact(2) {
                if attr[0].as_str() == Some("sandbox") {
                    let flags: Vec<_> = attr[1]
                        .as_str()
                        .unwrap_or("")
                        .split_ascii_whitespace()
                        .collect();
                    if !flags.contains(&"allow-scripts") || !flags.contains(&"allow-same-origin") {
                        return Err(
                            "This embedded player's sandbox does not allow phrase capture.".into(),
                        );
                    }
                }
            }
        }
        frame = parent;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_named_world_in_an_attached_frame_can_choose_the_player() {
        let mut frames = Frames::default();
        let context = json!({"context":{"name":WORLD,"id":7,"uniqueId":"old","auxData":{"type":"isolated","frameId":"child"}}});
        let token = "1234567890abcdef1234567890abcdef";
        let click = json!({"name":BINDING,"payload":token,"executionContextId":7});
        frames.event("Runtime.executionContextCreated", "unknown", &context);
        frames.event("Runtime.bindingCalled", "unknown", &click);
        assert!(frames.selected.is_none());
        frames.event(
            "Target.attachedToTarget",
            "",
            &json!({"sessionId":"frame-session","targetInfo":{"type":"iframe"}}),
        );
        let mut main_world = context.clone();
        main_world["context"]["auxData"]["type"] = json!("default");
        frames.event(
            "Runtime.executionContextCreated",
            "frame-session",
            &main_world,
        );
        frames.event("Runtime.bindingCalled", "frame-session", &click);
        assert!(frames.selected.is_none());
        frames.event("Runtime.executionContextCreated", "frame-session", &context);
        frames.event("Runtime.bindingCalled", "frame-session", &click);
        let selected = frames.selected.as_ref().unwrap();
        assert_eq!(
            (&*selected.session, &*selected.context, &*selected.frame),
            ("frame-session", "old", "child")
        );
        frames.event(
            "Runtime.executionContextsCleared",
            "frame-session",
            &json!({}),
        );
        let mut replacement = context;
        replacement["context"]["uniqueId"] = json!("new");
        frames.event(
            "Runtime.executionContextCreated",
            "frame-session",
            &replacement,
        );
        // A recycled numeric ID without another real click never rebinds the take.
        assert_eq!(frames.selected.as_ref().unwrap().context, "old");
        frames.event("Runtime.bindingCalled", "frame-session", &click);
        assert_eq!(frames.selected.as_ref().unwrap().context, "new");
    }
}
