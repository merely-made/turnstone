// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Native host command declarations and their validated Hest envelope.
use cambium::{EffectClass, Invocation, Outcome};
use mere_view::{ArchiveRequest, MereViewRequest, MergeOption, MergeRequest, SessionStep};
use pandect::SessionId;

const SCHEMA: &str = "mere-view.request/v1";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
enum Args {
    Activate {
        key: String,
    },
    Mint,
    Session {
        id: String,
        step: String,
    },
    ArchiveSave {
        id: String,
    },
    ArchiveOpen {
        key: String,
    },
    ArchiveCompose {
        keys: Vec<String>,
    },
    MergeChoose {
        key: String,
        option: String,
        hand: Option<String>,
    },
    MergeApply,
    MergeCancel,
    Host {
        key: String,
    },
    Layout {
        key: String,
    },
}

impl Args {
    fn declaration(&self) -> (&'static str, EffectClass) {
        match self {
            Self::Activate { .. } => ("mere:activate", EffectClass::Curation),
            Self::Mint => ("mere:mint", EffectClass::DomainTruth),
            Self::Session { .. } => ("mere:session", EffectClass::DomainTruth),
            Self::ArchiveSave { .. } => ("mere:archive-save", EffectClass::DomainTruth),
            Self::ArchiveOpen { .. } => ("mere:archive-open", EffectClass::DomainTruth),
            Self::ArchiveCompose { .. } => ("mere:archive-compose", EffectClass::DomainTruth),
            Self::MergeChoose { .. } => ("mere:merge-choose", EffectClass::Curation),
            Self::MergeApply => ("mere:merge-apply", EffectClass::DomainTruth),
            Self::MergeCancel => ("mere:merge-cancel", EffectClass::DomainTruth),
            // Refresh is the native route's host action; it reads view state.
            Self::Host { .. } => ("mere:host", EffectClass::Curation),
            Self::Layout { .. } => ("mere:layout", EffectClass::Curation),
        }
    }

    fn from_request(request: &MereViewRequest) -> Result<Self, String> {
        Ok(match request {
            MereViewRequest::Activate(key) => Self::Activate { key: key.clone() },
            MereViewRequest::Mint => Self::Mint,
            MereViewRequest::Session(id, step) => Self::Session {
                id: id.as_uuid().to_string(),
                step: step.token().into(),
            },
            MereViewRequest::Archive(ArchiveRequest::Save(id)) => Self::ArchiveSave {
                id: id.as_uuid().to_string(),
            },
            MereViewRequest::Archive(ArchiveRequest::Open(key)) => {
                Self::ArchiveOpen { key: key.clone() }
            },
            MereViewRequest::Archive(ArchiveRequest::Compose(keys)) => {
                Self::ArchiveCompose { keys: keys.clone() }
            },
            MereViewRequest::Merge(MergeRequest::Choose(key, option)) => {
                let (option, hand) = match option {
                    MergeOption::Source => ("source", None),
                    MergeOption::Target => ("target", None),
                    MergeOption::Hand(value) => ("hand", Some(value.clone())),
                    MergeOption::AddAsNew => ("add-as-new", None),
                };
                Self::MergeChoose {
                    key: key.clone(),
                    option: option.into(),
                    hand,
                }
            },
            MereViewRequest::Merge(MergeRequest::Apply) => Self::MergeApply,
            MereViewRequest::Merge(MergeRequest::Cancel) => Self::MergeCancel,
            MereViewRequest::Host(key) => Self::Host { key: key.clone() },
            MereViewRequest::Layout(key) => Self::Layout { key: key.clone() },
            _ => return Err("This request is not supported by the native resident worker.".into()),
        })
    }

    fn into_request(self) -> Result<MereViewRequest, Outcome> {
        Ok(match self {
            Self::Activate { key } => MereViewRequest::Activate(key),
            Self::Mint => MereViewRequest::Mint,
            Self::Session { id, step } => {
                let id = parse_session_id(&id)?;
                let step = match step.as_str() {
                    "switch" => SessionStep::Switch,
                    "fork" => SessionStep::Fork,
                    "branch" => SessionStep::Branch,
                    "merge" => SessionStep::Merge,
                    "trash" => SessionStep::Trash,
                    "restore" => SessionStep::Restore,
                    _ => return refused("Invalid session operation."),
                };
                MereViewRequest::Session(id, step)
            },
            Self::ArchiveSave { id } => {
                MereViewRequest::Archive(ArchiveRequest::Save(parse_session_id(&id)?))
            },
            Self::ArchiveOpen { key } => MereViewRequest::Archive(ArchiveRequest::Open(key)),
            Self::ArchiveCompose { keys } => {
                MereViewRequest::Archive(ArchiveRequest::Compose(keys))
            },
            Self::MergeChoose { key, option, hand } => {
                let option = match (option.as_str(), hand) {
                    ("source", None) => MergeOption::Source,
                    ("target", None) => MergeOption::Target,
                    ("hand", Some(value)) => MergeOption::Hand(value),
                    ("add-as-new", None) => MergeOption::AddAsNew,
                    _ => return refused("Invalid merge choice."),
                };
                MereViewRequest::Merge(MergeRequest::Choose(key, option))
            },
            Self::MergeApply => MereViewRequest::Merge(MergeRequest::Apply),
            Self::MergeCancel => MereViewRequest::Merge(MergeRequest::Cancel),
            Self::Host { key } => MereViewRequest::Host(key),
            Self::Layout { key } => MereViewRequest::Layout(key),
        })
    }
}

fn parse_session_id(value: &str) -> Result<SessionId, Outcome> {
    uuid::Uuid::parse_str(value)
        .map(SessionId)
        .map_err(|_| Outcome::Refused {
            reason: "Invalid session identifier.".into(),
        })
}

fn refused<T>(reason: &str) -> Result<T, Outcome> {
    Err(Outcome::Refused {
        reason: reason.into(),
    })
}

pub(super) fn invocation_for(request: &MereViewRequest) -> Result<Invocation, String> {
    let args = Args::from_request(request)?;
    let (id, effect) = args.declaration();
    Ok(Invocation {
        id: id.into(),
        schema_id: SCHEMA.into(),
        arguments: serde_json::to_vec(&args).map_err(|error| error.to_string())?,
        effect,
    })
}

pub(super) fn decode_invocation(invocation: &Invocation) -> Result<MereViewRequest, Outcome> {
    if invocation.schema_id != SCHEMA {
        return refused("Unsupported resident request schema.");
    }
    let args: Args =
        serde_json::from_slice(&invocation.arguments).map_err(|_| Outcome::Refused {
            reason: "Invalid resident request arguments.".into(),
        })?;
    let (id, effect) = args.declaration();
    if invocation.id != id || invocation.effect != effect {
        return refused("Resident request does not match its command declaration.");
    }
    args.into_request()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sid(n: u128) -> SessionId {
        SessionId(uuid::Uuid::from_u128(n))
    }

    #[test]
    fn every_native_request_variant_round_trips_through_its_declaration() {
        let requests = [
            MereViewRequest::Activate("node".into()),
            MereViewRequest::Mint,
            MereViewRequest::Session(sid(1), SessionStep::Fork),
            MereViewRequest::Archive(ArchiveRequest::Save(sid(2))),
            MereViewRequest::Archive(ArchiveRequest::Open("archive".into())),
            MereViewRequest::Archive(ArchiveRequest::Compose(vec!["a".into(), "b".into()])),
            MereViewRequest::Merge(MergeRequest::Choose("field".into(), MergeOption::Source)),
            MereViewRequest::Merge(MergeRequest::Choose("field".into(), MergeOption::Target)),
            MereViewRequest::Merge(MergeRequest::Choose(
                "field".into(),
                MergeOption::Hand("value".into()),
            )),
            MereViewRequest::Merge(MergeRequest::Choose("field".into(), MergeOption::AddAsNew)),
            MereViewRequest::Merge(MergeRequest::Apply),
            MereViewRequest::Merge(MergeRequest::Cancel),
            MereViewRequest::Host("refresh".into()),
            MereViewRequest::Layout("grid.default".into()),
        ];
        for request in requests {
            let invocation = invocation_for(&request).unwrap();
            assert_eq!(decode_invocation(&invocation), Ok(request));
        }
    }

    #[test]
    fn declaration_schema_and_malformed_arguments_are_refused() {
        let valid = invocation_for(&MereViewRequest::Mint).unwrap();
        for kind in 0..4 {
            let mut invalid = valid.clone();
            match kind {
                0 => invalid.schema_id = "mere-view.request/v2".into(),
                1 => invalid.id = "mere:archive-open".into(),
                2 => invalid.effect = EffectClass::Curation,
                _ => invalid.arguments = br#"{"Mint":{"extra":true}}"#.to_vec(),
            }
            assert!(matches!(
                decode_invocation(&invalid),
                Err(Outcome::Refused { .. })
            ));
        }
    }

    #[test]
    fn choosing_a_merge_value_is_local_curation_until_apply() {
        let invocation = invocation_for(&MereViewRequest::Merge(MergeRequest::Choose(
            "field".into(),
            MergeOption::Source,
        )))
        .unwrap();
        assert_eq!(invocation.effect, EffectClass::Curation);
        assert_eq!(invocation.id, "mere:merge-choose");
    }

    #[test]
    fn owner_workflow_actions_keep_their_declared_effect_and_refresh_is_curation() {
        // Archive open creates a thaw at the owner, and Cancel removes its
        // pending merge. Both use the owner's domain-truth action contract.
        let archive =
            mere_overmap::wire::archive_action(mere_overmap::wire::OPEN_CODICIL_INTENT, "Open");
        let open = invocation_for(&MereViewRequest::Archive(ArchiveRequest::Open(
            "archive".into(),
        )))
        .unwrap();
        assert_eq!(open.effect, archive.effect.into());
        let cancel = invocation_for(&MereViewRequest::Merge(MergeRequest::Cancel)).unwrap();
        assert_eq!(cancel.effect, EffectClass::DomainTruth);
        let refresh = invocation_for(&MereViewRequest::Host("refresh".into())).unwrap();
        assert_eq!(refresh.effect, EffectClass::Curation);
    }
}
