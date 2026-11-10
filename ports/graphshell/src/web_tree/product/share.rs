// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Copy link and Open in this browser for the selected object (S1).
use super::*;
use backstep::{ClipboardEvent, ClipboardRequest};
use cambium::{button, el};
use graphshell_host::handlers::{
    ATTACHED_BROWSER_HANDLER, ReferenceReach, attached_browser_target, prepare_reference,
};

impl SavedProduct {
    fn copy_link(&mut self) {
        let link = match prepare_reference(&self.address) {
            Ok(link) => link,
            Err(refusal) => {
                self.status = format!("Copy link unavailable · {refusal}");
                return;
            },
        };
        let reach = match link.reach {
            ReferenceReach::Network => "",
            ReferenceReach::ThisDevice => " · only this device can open it",
        };
        let outcome = self.share_outcome.clone();
        let done = format!("Copied link · {}{reach}", link.address);
        let write = ClipboardRequest::Write(link.address);
        crate::web_share::clipboard(write, move |event| {
            let failure = match event {
                ClipboardEvent::Written(Ok(())) => None,
                ClipboardEvent::Written(Err(error)) => Some(error.to_string()),
                ClipboardEvent::Read(_) => Some("the browser answered a read".into()),
            };
            *outcome.borrow_mut() = Some(failure.map_or(done, |error| {
                format!("Copy failed · {error} · select the address above to copy it")
            }));
        });
    }

    fn open_in_browser(&mut self) {
        if self.selection_locked() {
            return;
        }
        let address = self.address.clone();
        if let Err(error) = attached_browser_target(&address) {
            self.status = format!("Open refused · {error}");
            return;
        }
        let Some(app) = self.app.as_mut() else {
            return;
        };
        if let Some(profile) = &self.local_profile {
            app.host
                .set_access_context(profile.access_context(js_sys::Date::now() as u64));
        }
        // The owner's acceptance comes first; only an accepted intent opens a tab.
        self.status = match app.open_address(&address, ATTACHED_BROWSER_HANDLER) {
            Ok(chirograph::IntentResult::Accepted) => match crate::web_share::open_tab(
                &address,
                self.share_outcome.clone(),
                format!("Opened in this browser · {address}"),
                "Open failed".into(),
            ) {
                Ok(()) => format!("Opening {address}…"),
                Err(error) => format!("Open failed · {error}"),
            },
            Ok(chirograph::IntentResult::Rejected { reason }) => format!("Open refused · {reason}"),
            Ok(result) => format!("Open not accepted · {result:?}"),
            Err(error) => format!("Open failed · {error}"),
        };
    }
}

/// Copy link is primary; opening is offered only where a browser is attached.
pub(super) fn controls(product: &SavedProduct) -> Child {
    let mut children: Vec<Child> = vec![Box::new(button("Copy link", |page: &mut TreePage, _| {
        if let Some(product) = &mut page.product {
            product.copy_link();
        }
    }))];
    if product.local_profile.is_some() {
        children.push(Box::new(button(
            "Open in this browser",
            |page: &mut TreePage, _| {
                if let Some(product) = &mut page.product {
                    product.open_in_browser();
                }
            },
        )));
    }
    Box::new(
        el("div", children)
            .attr("class", "tree-share")
            .attr("aria-label", "Share and open"),
    )
}
