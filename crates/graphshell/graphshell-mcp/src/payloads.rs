// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! An agent writes a payload as JSON; the route reads only canonical CBOR
//! (Mere engine split D231, D232). Each known intent's JSON is parsed into its
//! Rust type and re-encoded, so a session id written as a uuid string crosses
//! as the bytes the route expects. An intent this server does not know is
//! refused here rather than forwarded.

use eidetic::kept_text::{KEEP_PAGES_INTENT, KEEP_SETTING_INTENT, KeepPagesV1, KeepSettingV1};
use mere_overmap::wire;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

fn typed<T: Serialize + DeserializeOwned>(json: Value) -> Result<Vec<u8>, String> {
    let value: T = serde_json::from_value(json)
        .map_err(|error| format!("the payload does not match this intent's schema: {error}"))?;
    chirograph::wire::encode(&value).map_err(|error| error.to_string())
}

/// The canonical CBOR payload for `intent`, from the agent's JSON. `None`
/// sends no payload.
pub(crate) fn encode(intent: &str, json: Option<Value>) -> Result<Vec<u8>, String> {
    let Some(json) = json else {
        return Ok(Vec::new());
    };
    match intent {
        wire::MINT_SESSION_INTENT
        | wire::ATTACH_SESSION_INTENT
        | wire::FORK_SESSION_INTENT
        | wire::BRANCH_SESSION_INTENT
        | wire::MERGE_SESSION_INTENT
        | wire::TRASH_SESSION_INTENT
        | wire::RESTORE_SESSION_INTENT => typed::<wire::SessionsActionV1>(json),
        wire::RESOLVE_MERGE_INTENT | wire::CANCEL_MERGE_INTENT => {
            typed::<wire::MergeResolutionV1>(json)
        },
        wire::SAVE_CODICIL_INTENT | wire::OPEN_CODICIL_INTENT | wire::COMPOSE_CODICILS_INTENT => {
            typed::<wire::ArchiveActionV1>(json)
        },
        wire::APPLY_EDITS_INTENT => typed::<wire::ApplyEditsV1>(json),
        wire::UNDO_INTENT | wire::REDO_INTENT => typed::<wire::StepV1>(json),
        wire::SET_VIEW_INTENT => typed::<wire::SetViewV1>(json),
        wire::SET_ATTENTION_INTENT => typed::<wire::SetAttentionV1>(json),
        wire::APPEND_NOTE_INTENT => typed::<wire::AppendNoteV1>(json),
        wire::SEARCH_NOTES_INTENT => typed::<wire::SearchNotesV1>(json),
        chirograph::KEEP_ENABLED_INTENT => typed::<chirograph::KeepEnabledV1>(json),
        KEEP_SETTING_INTENT => typed::<KeepSettingV1>(json),
        KEEP_PAGES_INTENT => typed::<KeepPagesV1>(json),
        other => Err(format!(
            "this server does not know the payload of `{other}`, so it cannot send it"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_id_written_as_a_uuid_string_crosses_as_its_type() {
        let id = "a0000000-0000-4000-8000-000000000007";
        let json = serde_json::json!({ "schema": wire::SESSIONS_SCHEMA, "session": id });
        let cbor = encode(wire::ATTACH_SESSION_INTENT, Some(json.clone())).unwrap();
        let read: wire::SessionsActionV1 = chirograph::wire::decode(&cbor).unwrap();
        assert_eq!(read.session.unwrap().as_uuid().to_string(), id);
        // The agent's JSON, read as JSON, is refused by the route's decoder.
        assert!(
            chirograph::wire::decode::<wire::SessionsActionV1>(&serde_json::to_vec(&json).unwrap())
                .is_err()
        );
    }

    #[test]
    fn an_unknown_intent_or_a_mismatched_payload_is_refused() {
        let note = serde_json::json!({ "schema": wire::APPEND_NOTE_SCHEMA, "kind": "decision", "subject": "s" });
        assert!(encode(wire::APPEND_NOTE_INTENT, Some(note.clone())).is_ok());
        let error = encode("fixture.unknown", Some(note)).unwrap_err();
        assert!(error.contains("fixture.unknown"), "{error}");
        assert!(
            encode(
                wire::APPEND_NOTE_INTENT,
                Some(serde_json::json!("not a note"))
            )
            .is_err()
        );
        assert_eq!(encode(wire::UNDO_INTENT, None).unwrap(), Vec::<u8>::new());
    }
}
