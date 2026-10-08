// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The page's buttons for shared commands say what the catalogue says, so a
//! command has one name on screen (Scenograph editor plan, SE52).

use cambium::catalogue::{self, ids};

const PAGE: &str = include_str!("../web/component.html");

/// Buttons whose text follows state rather than the catalogue: the physics
/// toggle reads "Pause physics" or "Resume physics" (SE52).
const STATE_TEXT: &[&str] = &[ids::PHYSICS_TOGGLE];

/// Each `<button ... data-command="id" ...>text</button>` on the page.
fn buttons(page: &str) -> Vec<(&str, &str)> {
    let mut found = Vec::new();
    let mut rest = page;
    while let Some(start) = rest.find("data-command=\"") {
        let after = &rest[start + "data-command=\"".len()..];
        let id = &after[..after.find('"').expect("a closed attribute")];
        let tag_end = after.find('>').expect("a closed tag");
        let body = &after[tag_end + 1..];
        let text = body[..body.find('<').unwrap_or(body.len())].trim();
        found.push((id, text));
        rest = &after[tag_end..];
    }
    found
}

#[test]
fn shared_command_buttons_use_the_catalogue_label() {
    let shared: Vec<_> = buttons(PAGE)
        .into_iter()
        .filter(|(id, _)| catalogue::label(id).is_some() && !STATE_TEXT.contains(id))
        .collect();
    assert!(shared.len() >= 6, "the page's shared buttons: {shared:?}");
    for (id, text) in shared {
        assert_eq!(Some(text), catalogue::label(id), "the button for {id}");
    }
}

#[test]
fn a_renamed_button_is_caught() {
    let page = r#"<button data-command="node:new">Add address</button>"#;
    let (id, text) = buttons(page)[0];
    assert_ne!(Some(text), catalogue::label(id));
}
