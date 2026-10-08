// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Native lifecycle and acceptance lane for the shared Tabard workshop.
//! The runner owns the product's `WorkshopState` directly. The host supplies
//! a window, a private default library path and frame captures.

mod reader;

use std::{
    cell::{Cell, RefCell},
    ffi::OsString,
    io,
    path::PathBuf,
    rc::Rc,
};

use cambium::{Key, KeyEvent, NamedKey};
use cambium_genet_winit_host::{
    AppCtx, CloseDisposition, FocusedTextSlot, Harness, HostHooks, HostOptions, Init, Runner,
    WindowFrame, inert_hooks,
};
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use mesquite::{CaptureRecord, LaneConfig};
use tabard_workshop::{
    ExportArtifact, GRAPH_LEAF_KEY, READER_LEAF_KEY, STYLESHEET_LEAF_KEY, StylesheetSpecimen,
    WorkshopState, WorkshopView, workshop_stylesheet, workshop_view,
};
use taproot::ProbeSnapshot;

pub type Logic = fn(&WorkshopState) -> WorkshopView;
pub type WorkshopHarness = Harness<WorkshopState, Logic, WorkshopView>;
type Context<'a> = AppCtx<'a, WorkshopState, Logic, WorkshopView>;
type WorkshopRunner = Runner<WorkshopState, Logic, WorkshopView>;

/// The host never chooses another product's theme directory implicitly.
pub fn default_library_path() -> io::Result<PathBuf> {
    dirs::data_local_dir()
        .map(|directory| directory.join("mere").join("tabard").join("themes.json"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "No application data directory was found; pass --library PATH.",
            )
        })
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Run { library: PathBuf },
    Help,
}

/// Parse OS strings so a library path need not be UTF-8. An explicit path
/// bypasses application-data discovery entirely.
pub fn parse_arguments(arguments: impl IntoIterator<Item = OsString>) -> io::Result<Command> {
    let mut arguments = arguments.into_iter();
    let mut library = None;
    while let Some(argument) = arguments.next() {
        if argument == "--help" || argument == "-h" {
            return Ok(Command::Help);
        }
        if argument == "--library" {
            if library.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--library may be supplied only once",
                ));
            }
            let path = arguments
                .next()
                .filter(|path| !path.is_empty())
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "--library needs a file path")
                })?;
            library = Some(PathBuf::from(path));
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Unknown argument: {}", argument.to_string_lossy()),
            ));
        }
    }
    Ok(Command::Run {
        library: match library {
            Some(path) => path,
            None => default_library_path()?,
        },
    })
}

pub const USAGE: &str = "Tabard appearance workshop\n\nUsage: tabard-desktop [--library PATH]\n\nThe default authored library is mere/tabard/themes.json under the platform's\nlocal application data directory. --library selects a separate library file.\n\nTABARD_SCENARIO, TABARD_CAPTURE_DIR and TABARD_RECEIPT enable the headed\nMesquite acceptance lane. TABARD_WIDTH and TABARD_HEIGHT override window size.";

/// Shared initialization for the headed window and windowless routing tests.
pub fn initialize(state: WorkshopState) -> Init<WorkshopState, Logic> {
    Init {
        state,
        logic: workshop_view as Logic,
        sheet: workshop_stylesheet(),
        fonts: Vec::new(),
        images: Vec::new(),
    }
}

pub fn host_options() -> HostOptions {
    HostOptions {
        title: "Tabard — Appearance workshop".into(),
        initial_logical_size: (1180.0, 800.0),
        size_env: Some(("TABARD_WIDTH".into(), "TABARD_HEIGHT".into())),
        window_frame: WindowFrame::Host,
        ..Default::default()
    }
}

pub fn hooks() -> HostHooks<WorkshopState, Logic, WorkshopView> {
    hooks_with_exporter(|artifact| {
        let extension = if artifact.format == tabard_workshop::ExportFormat::Css {
            "css"
        } else {
            "json"
        };
        cambium_genet_winit_host::choose_save_path(
            "Export theme",
            &artifact.suggested_name,
            &[extension],
        )
    })
}

/// Embedding/test hosts provide destination selection; the shared state owns
/// validation and writing, including explicit collision replacement.
pub fn hooks_with_exporter(
    mut destination: impl FnMut(&ExportArtifact) -> Option<PathBuf> + 'static,
) -> HostHooks<WorkshopState, Logic, WorkshopView> {
    let mut reader_producer: Option<Rc<RefCell<reader::ReaderProducer>>> = None;
    let mut stylesheet_producer: Option<
        Rc<RefCell<reader::ScenePreviewProducer<StylesheetSpecimen>>>,
    > = None;
    HostHooks {
        frame: Box::new(move |ctx| {
            ctx.leaves
                .insert(GRAPH_LEAF_KEY, Box::new(ctx.runner.state().graph_leaf()));
            let current_reader = ctx.runner.state().reader_preview();
            let producer = reader_producer.get_or_insert_with(|| {
                Rc::new(RefCell::new(reader::ReaderProducer::new(
                    current_reader.clone(),
                    reader::READER_RASTER_KEY,
                )))
            });
            producer.borrow_mut().set_reader(current_reader);
            if !ctx.producers.contains(READER_LEAF_KEY) {
                ctx.producers
                    .register(READER_LEAF_KEY, producer.clone(), &[])
                    .expect("reader uses its own bounded producer key");
            }
            let current_stylesheet = ctx.runner.state().stylesheet_preview();
            let producer = stylesheet_producer.get_or_insert_with(|| {
                Rc::new(RefCell::new(reader::ScenePreviewProducer::new(
                    current_stylesheet.clone(),
                    0x7461_6261_7264_6373,
                )))
            });
            producer.borrow_mut().set_preview(current_stylesheet);
            if !ctx.producers.contains(STYLESHEET_LEAF_KEY) {
                ctx.producers
                    .register(STYLESHEET_LEAF_KEY, producer.clone(), &[])
                    .expect("stylesheet uses its own bounded producer key");
            }
            false
        }),
        after_dispatch: Box::new(move |ctx| {
            ctx.runner.update(WorkshopState::sync_controls);
            let mut export = None;
            ctx.runner.update(|state| export = state.take_export());
            if let Some(artifact) = export {
                let path = destination(&artifact);
                ctx.runner
                    .update(|state| state.complete_export(artifact, path));
            }
            if ctx.runner.state().exit_requested() {
                *ctx.close = true;
            }
        }),
        close_request: Box::new(|ctx, _| {
            let mut allow = false;
            ctx.runner.update(|state| allow = state.request_close());
            if allow {
                cambium_genet_winit_host::CloseDisposition::Exit
            } else {
                cambium_genet_winit_host::CloseDisposition::KeepVisible
            }
        }),
        focused_text: Box::new(|runner| {
            let node = runner.focus()?;
            let dom = runner.dom();
            let dom = dom.borrow();
            let field = dom
                .attribute(node, &Namespace::from(""), &LocalName::from("data-field"))?
                .to_string();
            runner.state().text_field(&field)?;
            let field_mut = field.clone();
            Some(FocusedTextSlot {
                node,
                get: Box::new(move |state| {
                    state
                        .text_field(&field)
                        .expect("the mounted field is registered")
                }),
                get_mut: Box::new(move |state| {
                    state
                        .text_field_mut(&field_mut)
                        .expect("the mounted field is registered")
                }),
            })
        }),
        ..inert_hooks()
    }
}

/// Mount the production view with the production host configuration.
pub fn harness(state: WorkshopState) -> WorkshopHarness {
    let mut harness = Harness::with_hooks_and_options(initialize(state), hooks(), host_options());
    harness.layout_at(1180.0, 800.0);
    harness
}

fn focus_name(runner: &WorkshopRunner) -> String {
    let Some(node) = runner.focus() else {
        return "none".into();
    };
    let dom = runner.dom();
    let dom = dom.borrow();
    for name in [
        "data-action",
        "data-mode",
        "data-field",
        "data-seed",
        "aria-label",
    ] {
        if let Some(value) = dom.attribute(node, &Namespace::from(""), &LocalName::from(name)) {
            return format!("{name}={value}");
        }
    }
    "other".into()
}

struct WorkshopLane {
    completion: Rc<Cell<Option<bool>>>,
    sheet: String,
}

impl mesquite::Product for WorkshopLane {
    type State = WorkshopState;
    type Logic = Logic;
    type View = WorkshopView;
    const KIND: &'static str = "tabard-workshop";
    const SURFACE: &'static str = "app";
    const LOG_PREFIX: &'static str = "tabard";

    fn sheet(&self) -> &str {
        &self.sheet
    }

    fn snapshot(&self, ctx: &Context<'_>, _: usize, _: f32) -> ProbeSnapshot {
        let state = ctx.runner.state();
        let primary = state.draft_theme().seeds.primary;
        ProbeSnapshot::default()
            .with_field("mode", state.mode_key())
            .with_field(
                "graph_selected",
                state
                    .selected_graph_node()
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            )
            .with_field("dirty", state.is_dirty().to_string())
            .with_field("theme", state.draft_theme().id.clone())
            .with_field("name", state.draft_theme().name.clone())
            .with_field("status", state.status())
            .with_field("focus", focus_name(ctx.runner))
            .with_field(
                "library",
                state
                    .library_path()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "none".into()),
            )
            .with_field(
                "primary",
                format!("#{:02x}{:02x}{:02x}", primary.r, primary.g, primary.b),
            )
    }

    fn busy_mut(&mut self, _: &mut Context<'_>, capture_pending: bool) -> Option<bool> {
        Some(capture_pending)
    }

    /// Keyboard delivery is the runner's event path; pointer clicks use
    /// Mesquite's ordinary selector resolution and host pointer delivery.
    fn app_step(
        &mut self,
        ctx: &mut Context<'_>,
        _: mesquite::Checkpoints<'_>,
        line: &str,
    ) -> Result<(), String> {
        if let Some(text) = line.strip_prefix("text ") {
            ctx.runner
                .dispatch_key(KeyEvent::new(Key::Character(text.into())));
            ctx.runner.update(WorkshopState::sync_controls);
            return Ok(());
        }
        if let Some(target) = line.strip_prefix("key tab-until ") {
            let target = target.trim();
            if target.is_empty() {
                return Err("tab-until needs <attribute>=<value>".into());
            }
            for _ in 0..150 {
                if focus_name(ctx.runner) == target {
                    return Ok(());
                }
                ctx.runner
                    .dispatch_key(KeyEvent::new(Key::Named(NamedKey::Tab)));
            }
            return Err(format!("Tab never reached {target}"));
        }
        Err(format!("unknown scenario verb: {line}"))
    }

    fn receipt_checks(&self, captures: &[CaptureRecord]) -> Vec<String> {
        let digest = |name: &str| {
            captures
                .iter()
                .find(|capture| capture.name == name)
                .map(|capture| capture.digest)
        };
        let mut errors = Vec::new();
        for (before, after) in [("initial", "seed_edited"), ("seed_edited", "dark")] {
            if let (Some(before_digest), Some(after_digest)) = (digest(before), digest(after))
                && before_digest == after_digest
            {
                errors.push(format!(
                    "{before} and {after} left the presented frame unchanged"
                ));
            }
        }
        errors
    }

    fn complete(&mut self, _: &mut Context<'_>, outcome: &taproot::Outcome) -> Result<(), String> {
        self.completion.set(Some(outcome.ok));
        Ok(())
    }
}

/// Load before opening the window so a malformed or unreadable authored
/// library produces a command-line error without constructing a replacement.
pub fn run(library: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let state = WorkshopState::load(library)?;
    let completion = Rc::new(Cell::new(None));
    let config = LaneConfig::from_env("TABARD");
    let receipt = config.as_ref().and_then(LaneConfig::receipt_path);
    let lane = config
        .map(|config| {
            mesquite::Lane::from_config(
                config,
                WorkshopLane {
                    completion: completion.clone(),
                    sheet: workshop_stylesheet(),
                },
                cambium_genet_winit_host::read_file,
            )
            .map(|lane| lane.with_frame_limit(Some(1800)))
        })
        .transpose()
        .map_err(io::Error::other)?;
    let scripted = lane.is_some();
    let lane = Rc::new(RefCell::new(lane));
    let frame_lane = lane.clone();
    let close_lane = lane.clone();
    let product_hooks = hooks();
    let mut product_close = product_hooks.close_request;
    let hooks = HostHooks {
        after_frame: Box::new(move |ctx| {
            if let Some(lane) = frame_lane.borrow_mut().as_mut() {
                lane.after_frame(ctx);
            }
        }),
        close_request: Box::new(move |ctx, request| {
            if let Some(lane) = close_lane.borrow_mut().as_mut()
                && !lane.finished()
            {
                lane.request_close();
                return CloseDisposition::KeepVisible;
            }
            product_close(ctx, request)
        }),
        ..product_hooks
    };
    cambium_genet_winit_host::run(host_options(), move |_, _, _| initialize(state), hooks)?;
    if scripted && completion.get() != Some(true) {
        return Err(
            io::Error::other("The Tabard acceptance scenario failed or did not complete.").into(),
        );
    }
    if let Some(receipt) = receipt {
        let result = std::fs::read_to_string(receipt)?;
        if !result.contains("RESULT ok") {
            return Err(
                io::Error::other("The Tabard acceptance receipt did not report success.").into(),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_library_is_preserved() {
        assert_eq!(
            parse_arguments([
                OsString::from("--library"),
                OsString::from("a library/themes.json")
            ])
            .unwrap(),
            Command::Run {
                library: PathBuf::from("a library/themes.json")
            }
        );
    }

    #[test]
    fn invalid_arguments_are_reported() {
        for arguments in [
            vec!["--library"],
            vec!["--unknown"],
            vec!["--library", "a", "--library", "b"],
        ] {
            assert!(parse_arguments(arguments.into_iter().map(OsString::from)).is_err());
        }
    }
}
