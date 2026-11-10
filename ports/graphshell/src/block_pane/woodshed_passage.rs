// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Opt-in N2 fixture shared by the desktop and web block panes.

use super::{
    Child, Document, Pane, PaneHost, action, button, el, source_revision, toolbar, workspace,
};

const CLASS: &str = "progression";
const DOCUMENT: &str = "graphshell:fixture/woodshed-tune";
const PASSAGE: &str = "n2-rehearsal-bridge";
const PASSAGE_TITLE: &str = "Rehearsal notes · Bridge";
const PROVENANCE: &str = "Woodshed adapter example · opened from Rehearsal notes · Bridge";

impl Pane {
    pub fn enable_woodshed_passage_proof(&mut self) {
        self.woodshed_passage_proof = true;
    }

    pub fn woodshed_passage_proof(&self) -> bool {
        self.woodshed_passage_proof
    }

    fn open_woodshed_tune(&mut self) -> Result<(), String> {
        if self.document.id != DOCUMENT {
            let example = self
                .adapter(CLASS)
                .and_then(|adapter| adapter.claim().example.clone())
                .ok_or("Approve the Woodshed adapter to open this tune")?;
            let mut attrs = example.attrs;
            attrs.insert("class".into(), CLASS.into());
            attrs.insert("id".into(), "n2-woodshed-chart".into());
            let block = crate::block_editing::wrap(&attrs, &example.body);
            self.document = Document::open(DOCUMENT, &block, self.editors.as_ref());
            self.reset_input();
            self.selected = 0;
            self.status.clear();
        }
        self.open = true;
        Ok(())
    }
}

pub fn woodshed_passage<P: PaneHost>(page: &P) -> Option<Child<P>> {
    let pane = page.pane();
    if !pane.woodshed_passage_proof() {
        return None;
    }
    let mut children: Vec<Child<P>> = if pane.open {
        let mut children: Vec<Child<P>> = vec![
            Box::new(el("h2", "Woodshed tune")),
            Box::new(el(
                "p",
                format!("Chart source revision {}", source_revision(pane.source())),
            )),
            Box::new(el("p", PROVENANCE)),
        ];
        children.extend(toolbar(page));
        children.extend(workspace(page));
        children
    } else {
        vec![
            Box::new(el("h2", PASSAGE_TITLE).attr("id", PASSAGE)),
            Box::new(el(
                "p",
                "For the bridge, open the Woodshed tune and write out the chart.",
            )),
            Box::new(cambium::request_focus(
                button("Open Woodshed tune", |p: &mut P, _| {
                    action(p, Pane::open_woodshed_tune)
                })
                .attr("class", "n2-open-woodshed"),
                pane.document.id == DOCUMENT,
            )),
            Box::new(el("p", pane.status.clone())),
        ]
    };
    if !pane.open && pane.adapter(CLASS).is_none() {
        children.push(Box::new(
            button("Review block adapters", |p: &mut P, _| {
                p.pane_mut().review_requested = true
            })
            .attr("class", "n2-review-adapters"),
        ));
    }
    Some(Box::new(
        el("section", children).attr("class", "n2-woodshed-passage"),
    ))
}

pub(super) fn probe_fields(pane: &Pane) -> [(&'static str, String); 4] {
    [
        ("n2-passage", if pane.open { "" } else { PASSAGE }.into()),
        ("n2-tune", if pane.open { DOCUMENT } else { "" }.into()),
        (
            "n2-provenance",
            if pane.open { PROVENANCE } else { "" }.into(),
        ),
        (
            "n2-chart-revision",
            if pane.open {
                source_revision(pane.source())
            } else {
                String::new()
            },
        ),
    ]
}
