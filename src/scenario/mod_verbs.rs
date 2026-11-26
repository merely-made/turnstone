// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The scenario verbs that drive Wasm mod projects (mod projects MP5):
//! `mod-create [dir]`, `mod-open <dir> [dev]`, `mod-check|build|attach|fixtures|
//! reload|detach <id>`, `mod-level <id> dev|conformant`, `mod-turn <id> <kind>
//! [payload]` and `copy-file <from> <to>` (bytes, to swap a component).

use super::expand_env::expand_env;
use crate::action::ModProjectAction as Step;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ModVerb {
    Step(Step),
    CopyFile { from: String, to: String },
}

/// `None` when `line` is not a mod verb.
pub(crate) fn parse(line: &str) -> Option<Result<ModVerb, String>> {
    let (verb, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    if !(verb.starts_with("mod-") || verb == "copy-file") {
        return None;
    }
    let rest = match expand_env(rest.trim(), 1) {
        Ok(rest) => rest,
        Err(error) => return Some(Err(error)),
    };
    let words: Vec<&str> = rest.split_whitespace().collect();
    let id = || {
        words
            .first()
            .map(|id| (*id).to_string())
            .ok_or_else(|| format!("{verb} wants a project id"))
    };
    Some(match verb {
        "mod-create" => Ok(ModVerb::Step(Step::Create {
            from: words.first().map(|dir| (*dir).into()),
        })),
        "mod-open" => match words.as_slice() {
            [path] => Ok(ModVerb::Step(Step::Open { path: (*path).into(), dev: false })),
            [path, "dev"] => Ok(ModVerb::Step(Step::Open { path: (*path).into(), dev: true })),
            _ => Err("mod-open wants '<dir> [dev]'".into()),
        },
        "mod-check" => id().map(|id| ModVerb::Step(Step::Check(id))),
        "mod-build" => id().map(|id| ModVerb::Step(Step::Build(id))),
        "mod-attach" => id().map(|id| ModVerb::Step(Step::Attach(id))),
        "mod-fixtures" => id().map(|id| ModVerb::Step(Step::RunFixtures(id))),
        "mod-reload" => id().map(|id| ModVerb::Step(Step::Reload(id))),
        "mod-detach" => id().map(|id| ModVerb::Step(Step::Detach(id))),
        "mod-level" => match words.as_slice() {
            [id, level @ ("dev" | "conformant")] => Ok(ModVerb::Step(Step::SetLevel {
                id: (*id).into(),
                dev: *level == "dev",
            })),
            _ => Err("mod-level wants '<id> dev|conformant'".into()),
        },
        "mod-turn" => match rest.split_once(char::is_whitespace) {
            Some((id, tail)) => {
                let (kind, payload) = tail.trim().split_once(char::is_whitespace).unwrap_or((tail.trim(), ""));
                Ok(ModVerb::Step(Step::Turn {
                    id: id.into(),
                    kind: kind.into(),
                    payload: payload.trim().into(),
                }))
            },
            None => Err("mod-turn wants '<id> <kind> [payload]'".into()),
        },
        "copy-file" => match words.as_slice() {
            [from, to] => Ok(ModVerb::CopyFile { from: (*from).into(), to: (*to).into() }),
            _ => Err("copy-file wants '<from> <to>'".into()),
        },
        other => Err(format!("unknown verb {other}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_verbs_parse_and_other_lines_pass_through() {
        assert_eq!(
            parse("mod-create /f/starter"),
            Some(Ok(ModVerb::Step(Step::Create { from: Some("/f/starter".into()) })))
        );
        assert_eq!(
            parse("mod-create"),
            Some(Ok(ModVerb::Step(Step::Create { from: None })))
        );
        assert_eq!(
            parse("mod-open /p/x dev"),
            Some(Ok(ModVerb::Step(Step::Open { path: "/p/x".into(), dev: true })))
        );
        assert_eq!(
            parse("mod-turn notes-mod set Two words"),
            Some(Ok(ModVerb::Step(Step::Turn {
                id: "notes-mod".into(),
                kind: "set".into(),
                payload: "Two words".into()
            })))
        );
        assert_eq!(
            parse("copy-file /a /b"),
            Some(Ok(ModVerb::CopyFile { from: "/a".into(), to: "/b".into() }))
        );
        assert!(matches!(parse("mod-check"), Some(Err(_))));
        assert!(parse("pack-reload x").is_none());
        assert!(parse("settle 10").is_none());
    }
}
