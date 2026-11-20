// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(crate) const LEGACY_COMMAND: &str = "--gemtext-v1";
pub(crate) const REQUEST_LIMIT: u64 = 4 * 1024 * 1024;
pub(crate) const REPLY_LIMIT: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Parser {
    Gemtext,
    Markdown,
}

impl Parser {
    pub(crate) const ALL: [Self; 2] = [Self::Gemtext, Self::Markdown];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Gemtext => "gemtext",
            Self::Markdown => "markdown",
        }
    }

    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::Gemtext => "--gemtext-v2",
            Self::Markdown => "--markdown-v2",
        }
    }

    pub(crate) fn source_kind(self) -> &'static str {
        match self {
            Self::Gemtext => "nematic.gemtext",
            Self::Markdown => "nematic.markdown",
        }
    }

    pub(crate) fn accepts(self, input: &inker::EngineInput, legacy: bool) -> bool {
        let Some((scheme, _)) = input.address.split_once("://") else {
            return false;
        };
        let schemes: &[&str] = if legacy {
            &["gemini"]
        } else {
            &["gemini", "spartan", "http", "https"]
        };
        if !schemes
            .iter()
            .any(|candidate| scheme.eq_ignore_ascii_case(candidate))
        {
            return false;
        }
        let formats: &[&str] = match self {
            Self::Gemtext => &["text/gemini"],
            Self::Markdown => &["text/markdown", "text/x-markdown"],
        };
        match input.content_type.as_deref() {
            Some(mime) => {
                let essence = mime.split_once(';').map_or(mime, |(essence, _)| essence);
                formats
                    .iter()
                    .any(|format| essence.trim().eq_ignore_ascii_case(format))
            },
            None => {
                self == Self::Gemtext
                    && ["gemini", "spartan"]
                        .iter()
                        .any(|candidate| scheme.eq_ignore_ascii_case(candidate))
            },
        }
    }
}
