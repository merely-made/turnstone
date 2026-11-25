// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The scenario verbs that drive command packs (command packs CP4):
//! `pack-install <dir> [dev]`, `pack-reload <id>` and `write-file <path> <text>`
//! (`\n` in the text is a newline). Parsed here, run by the shell's `app_step`.

use super::expand_env::expand_env;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PackVerb {
    Install { path: String, dev: bool },
    Reload(String),
    WriteFile { path: String, text: String },
}

/// `None` when `line` is not a pack verb.
pub(crate) fn parse(line: &str) -> Option<Result<PackVerb, String>> {
    let (verb, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    if !matches!(verb, "pack-install" | "pack-reload" | "write-file") {
        return None;
    }
    let rest = match expand_env(rest.trim(), 1) {
        Ok(rest) => rest,
        Err(error) => return Some(Err(error)),
    };
    let rest = rest.trim();
    Some(match (verb, rest) {
        (_, "") => Err(format!("{verb} wants an argument")),
        ("pack-install", _) => Ok(match rest.rsplit_once(char::is_whitespace) {
            Some((path, "dev")) => PackVerb::Install {
                path: path.trim().into(),
                dev: true,
            },
            _ => PackVerb::Install {
                path: rest.into(),
                dev: false,
            },
        }),
        ("pack-reload", _) => Ok(PackVerb::Reload(rest.into())),
        (_, _) => match rest.split_once(char::is_whitespace) {
            Some((path, text)) => Ok(PackVerb::WriteFile {
                path: path.into(),
                text: text.replace("\\n", "\n"),
            }),
            None => Err("write-file wants '<path> <text>'".into()),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_verbs_parse_and_other_lines_pass_through() {
        assert_eq!(
            parse("pack-install /p/demo dev"),
            Some(Ok(PackVerb::Install {
                path: "/p/demo".into(),
                dev: true
            }))
        );
        assert_eq!(
            parse("pack-install /p/demo"),
            Some(Ok(PackVerb::Install {
                path: "/p/demo".into(),
                dev: false
            }))
        );
        assert_eq!(
            parse("pack-reload demo-pack"),
            Some(Ok(PackVerb::Reload("demo-pack".into())))
        );
        assert_eq!(
            parse(r#"write-file /p/mod.toml a = 1\nb = "x""#),
            Some(Ok(PackVerb::WriteFile {
                path: "/p/mod.toml".into(),
                text: "a = 1\nb = \"x\"".into()
            }))
        );
        assert!(matches!(parse("write-file /p/mod.toml"), Some(Err(_))));
        assert!(parse("settle 10").is_none());
    }
}
