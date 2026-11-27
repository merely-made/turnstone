// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Parser and viewer projects (mod projects MP7): what `check` asks of their
//! components, and the calls the dispatching engine and the panes make.
//!
//! A parser takes the pages whose media type its `mod.toml` declares (D44), by
//! the rule in [`ModProjectHost::route_for`] (D39). A trap drops its instance
//! and is the project's last trap (D20); the next page attaches afresh.

use inker::{DocumentTrustState, EngineDocument};
use mod_host::{
    Diagnostic, Limits, ModKind, ParseContext, ParseFailure, ParserHost, ViewedScene, ViewerHost,
    Watchdog, wasmtime::Engine,
};

use super::{ModProjectHost, Project, lock};

/// One engine and its watchdog, shared by every guest instance.
pub(super) struct GuestRuntime {
    engine: Engine,
    _watchdog: Watchdog,
}

/// A page a parser mod parsed.
#[derive(Clone, Debug)]
pub struct ParsedPage {
    pub document: EngineDocument,
    pub diagnostics: Vec<Diagnostic>,
    pub mod_id: String,
    pub display_name: String,
    pub version: String,
    pub media_type: String,
    /// Whether the mod holds the network grant: images on other origins load only then (D43).
    pub foreign_images: bool,
}

/// Why a parser mod produced no page; it names the mod and says why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseRefusal {
    pub mod_id: String,
    pub display_name: String,
    pub why: String,
}

impl std::fmt::Display for ParseRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Parser mod {} ({}) {}", self.display_name, self.mod_id, self.why)
    }
}

impl ModProjectHost {
    fn guests(&self) -> Result<&GuestRuntime, String> {
        if let Some(runtime) = self.guests.get() {
            return Ok(runtime);
        }
        let engine = mod_host::guarded_engine().map_err(|e| format!("guest engine: {e}"))?;
        let _watchdog = mod_host::watchdog(&engine);
        Ok(self.guests.get_or_init(|| GuestRuntime { engine, _watchdog }))
    }

    fn component_bytes(p: &Project) -> Result<Vec<u8>, String> {
        std::fs::read(&p.component).map_err(|e| format!("{}: {e}", p.component.display()))
    }

    /// What a parser or viewer component asks of the host beyond its exports:
    /// a parser reports the media types `mod.toml` declares, a viewer reads the
    /// graph format the host sends. Nothing is pinned yet: this is the check
    /// that precedes admission of an edited component.
    pub(super) fn check_guest(&self, p: &Project) -> Result<(), String> {
        let bytes = Self::component_bytes(p)?;
        let engine = &self.guests()?.engine;
        let granted = vec!["mere:script/log".to_string()];
        match p.kind {
            ModKind::Document => Ok(()),
            ModKind::Parser => {
                let mut host = ParserHost::attach_bytes(engine, &bytes, None, granted, Limits::default())
                    .map_err(|e| format!("attach: {e:#}"))?;
                let mut reported = host.describe().map_err(|e| format!("describe: {e:#}"))?;
                reported.sort();
                reported.dedup();
                if reported == p.media_types {
                    Ok(())
                } else {
                    Err(format!(
                        "the component reports media types [{}] but mod.toml declares [{}]",
                        reported.join(", "),
                        p.media_types.join(", ")
                    ))
                }
            },
            ModKind::Viewer => ViewerHost::attach_bytes(engine, &bytes, None, granted, Limits::default())
                .map(drop)
                .map_err(|e| format!("attach: {e:#}")),
        }
    }

    /// The installed parser to hand a page of `media_type` to, if any: a project
    /// that declares it, and, when a built-in already claims the type, one the
    /// user turned it on for (D39).
    pub fn route_for(&self, media_type: &str, builtin_claims: bool) -> Option<String> {
        let media = mod_host::normalize_media_type(media_type)?;
        lock(&self.state)
            .projects
            .iter()
            .find(|(_, p)| {
                p.kind == ModKind::Parser
                    && p.media_types.contains(&media)
                    && (!builtin_claims || p.claimed_on.contains(&media))
            })
            .map(|(id, _)| id.clone())
    }

    /// Parse `bytes` of `media_type` with the project that routes it. `address`
    /// is the page's, `engine_id` becomes its source (`mod:<id>`).
    pub fn parse_page(
        &self,
        id: &str,
        media_type: &str,
        bytes: &[u8],
        address: &str,
    ) -> Result<ParsedPage, ParseRefusal> {
        let mut state = lock(&self.state);
        let p = state.projects.get_mut(id).ok_or_else(|| ParseRefusal {
            mod_id: id.into(),
            display_name: id.into(),
            why: "is not installed".into(),
        })?;
        let refuse = |p: &Project, why: String| ParseRefusal {
            mod_id: p.manifest.mod_id.clone(),
            display_name: p.manifest.display_name.clone(),
            why,
        };
        if p.parser.is_none() {
            let attached = self.guests().and_then(|rt| {
                let record = self.runtime.record(id).ok_or_else(|| "is not admitted".to_string())?;
                let bytes = Self::component_bytes(p)?;
                ParserHost::attach_bytes(
                    &rt.engine,
                    &bytes,
                    Some(&record),
                    vec!["mere:script/log".into()],
                    Limits::default(),
                )
                .map_err(|e| format!("could not attach: {e:#}"))
            });
            match attached {
                Ok(host) => p.parser = Some(host),
                Err(why) => {
                    p.last_trap = Some(why.clone());
                    return Err(refuse(p, why));
                },
            }
        }
        let engine_id = format!("mod:{}", p.manifest.mod_id);
        let ctx = ParseContext { address, engine_id: &engine_id, trust: DocumentTrustState::Unknown };
        let host = p.parser.as_mut().expect("attached above");
        match host.parse(bytes, media_type, &ctx) {
            Ok(parsed) => {
                let logs = host.logs().to_vec();
                p.keep_logs(&logs);
                let foreign_images = p.grant().net == document_host::CapPermission::Allow;
                Ok(ParsedPage {
                    document: parsed.document,
                    diagnostics: parsed.diagnostics,
                    mod_id: p.manifest.mod_id.clone(),
                    display_name: p.manifest.display_name.clone(),
                    version: p.version.clone(),
                    media_type: media_type.to_string(),
                    foreign_images,
                })
            },
            Err(failure) => {
                let why = failure_words(&failure);
                if failure.drops_instance() {
                    p.parser = None;
                    p.last_trap = Some(why.clone());
                }
                p.failure = Some(why.clone());
                Err(refuse(p, why))
            },
        }
    }

    /// A viewer project's scene of `graph` (`mere.graph-snapshot-and-facets/json`).
    pub fn view_graph(&self, id: &str, graph: &[u8], width: f32) -> Result<(String, ViewedScene), String> {
        let mut state = lock(&self.state);
        let p = state.projects.get_mut(id).ok_or_else(|| format!("no mod project {id}"))?;
        if p.kind != ModKind::Viewer {
            return Err(format!("{id} is not a viewer"));
        }
        if p.viewer.is_none() {
            let record = self.runtime.record(id).ok_or_else(|| format!("{id} is not admitted"))?;
            let bytes = Self::component_bytes(p)?;
            let host = ViewerHost::attach_bytes(
                &self.guests()?.engine,
                &bytes,
                Some(&record),
                vec!["mere:script/log".into()],
                Limits::default(),
            )
            .map_err(|e| format!("{id}: attach: {e:#}"))?;
            p.viewer = Some(host);
        }
        let host = p.viewer.as_mut().expect("attached above");
        let title = host.title().to_string();
        match host.view(graph, width) {
            Ok(scene) => {
                p.failure = None;
                Ok((title, scene))
            },
            Err(failure) => {
                // A trap and a refusal reach the row alike (MP7b, D52): its last trap and its
                // failure. The row is the project's, so it says the cause; the pane's text, which
                // has no row around it, also names the mod.
                if failure.drops_instance() {
                    p.viewer = None;
                }
                p.last_trap = Some(failure.to_string());
                p.failure = Some(failure.to_string());
                Err(format!("{id}: {failure}"))
            },
        }
    }

    /// Remember how the page at `address` was parsed, for the status line and
    /// the inspector (D41).
    pub fn note_parsed(&self, address: &str, short: String, detail: String) {
        lock(&self.notes).insert(address.to_string(), (short, detail));
    }

    /// The status line's few words on how the page at `address` was parsed by a mod.
    pub fn parse_note(&self, address: &str) -> Option<String> {
        lock(&self.notes).get(address).map(|(short, _)| short.clone())
    }

    /// The inspector's full account of the same.
    pub fn parse_detail(&self, address: &str) -> Option<String> {
        lock(&self.notes).get(address).map(|(_, detail)| detail.clone())
    }

    /// Turn a claimed media type on or off for a parser project (D39).
    pub fn set_claimed(&self, id: &str, media_type: &str, on: bool) -> Result<(), String> {
        self.with(id, |p, _| {
            if p.kind != ModKind::Parser {
                return Err(format!("{id} is not a parser"));
            }
            let media = mod_host::normalize_media_type(media_type)
                .filter(|m| p.media_types.contains(m))
                .ok_or_else(|| format!("{id} does not declare {media_type} in mod.toml"))?;
            p.claimed_on.retain(|m| *m != media);
            if on {
                p.claimed_on.push(media);
                p.claimed_on.sort();
            }
            Ok(())
        })?;
        self.save_list(&lock(&self.state));
        Ok(())
    }
}

fn failure_words(failure: &ParseFailure) -> String {
    match failure {
        ParseFailure::Refused(why) => format!("refused the page: {why}"),
        ParseFailure::Trapped(why) => format!("trapped and was detached: {why}"),
        other => format!("returned a result the host could not use ({other})"),
    }
}
