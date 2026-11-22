// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The query page (graphshell:mere-cli D21): join the host a `mere serve
//! --via webrtc` invite names, read the one query it grants, and show the
//! rows as a table. The session rules are graphshell-client's
//! `RemoteSession::querying`; the link is `web_rtc_link`'s.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use web_sys::{Document, HtmlElement};

use crate::web_rtc_link::{self, LiveRemote, RemoteHost};

/// What the page keeps: the joined remote, and a status from before it.
struct QueryPage {
    document: Document,
    live: Option<LiveRemote>,
    status: String,
}

impl QueryPage {
    fn element(&self, id: &str) -> Option<HtmlElement> {
        self.document
            .get_element_by_id(id)
            .and_then(|element| element.dyn_into::<HtmlElement>().ok())
    }

    /// Mirror the session into `#status` and `#rows`.
    fn render(&self) {
        let status = self
            .live
            .as_ref()
            .map_or(self.status.as_str(), |live| live.session.status());
        if let Some(element) = self.element("status") {
            element.set_text_content(Some(status));
        }
        let Some(read) = self.live.as_ref().and_then(|live| live.session.query_read()) else {
            return;
        };
        let Some(table) = self.element("rows") else {
            return;
        };
        table.set_text_content(None);
        let row = |cells: &[Option<&str>], tag: &str| -> Option<web_sys::Element> {
            let tr = self.document.create_element("tr").ok()?;
            for cell in cells {
                let td = self.document.create_element(tag).ok()?;
                match cell {
                    Some(text) => td.set_text_content(Some(text)),
                    None => {
                        td.set_text_content(Some("—"));
                        td.set_class_name("unbound");
                    },
                }
                tr.append_child(&td).ok()?;
            }
            Some(tr)
        };
        let head: Vec<Option<&str>> = read.variables.iter().map(|v| Some(v.as_str())).collect();
        if let Some(tr) = row(&head, "th") {
            let _ = table.append_child(&tr);
        }
        for values in &read.rows {
            let cells: Vec<Option<&str>> = values.iter().map(Option::as_deref).collect();
            if let Some(tr) = row(&cells, "td") {
                let _ = table.append_child(&tr);
            }
        }
        table.set_attribute("data-done", if read.done { "true" } else { "false" }).ok();
    }
}

impl RemoteHost for QueryPage {
    fn live(&mut self) -> Option<&mut LiveRemote> {
        self.live.as_mut()
    }
    fn adopt(&mut self, live: LiveRemote) {
        self.live = Some(live);
    }
    fn pre_join(&mut self, status: &str) {
        self.status = status.to_string();
    }
    fn join_failed(&mut self, error: String) {
        self.status = format!("Failed · {error}");
    }
    fn changed(&mut self) {
        if let Some(live) = self.live.as_mut() {
            live.flush();
        }
        self.render();
    }
}

/// Join and read `query`; the page shows progress in `#status` and the rows
/// in `#rows`.
pub(crate) fn start(signal_url: String, invite: Option<String>, query: String) -> Result<(), String> {
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or("the query page has no document")?;
    let page = Rc::new(RefCell::new(QueryPage {
        document,
        live: None,
        status: "joining".to_string(),
    }));
    web_rtc_link::connect_querying(page, signal_url.trim_end_matches('/').to_string(), invite, query);
    Ok(())
}
