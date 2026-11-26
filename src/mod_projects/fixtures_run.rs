// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Running a project's fixtures (D6): turns in, the pinned DOM and turn log out;
//! for a parser, bytes in and the document's outline out; for a viewer, a graph
//! in and the scene's regions out (MP7, D35).

use std::path::Path;

use document_host::mod_loader::InstallRecord;
use document_host::{Fixture, FixtureOutcome, Quota, run_fixture};
use mod_host::fixture as guest;
use mod_host::{Limits, ModKind};

use super::{ModProjectHost, Project};

/// One fixture file's run: passed, or the diff; and what the guest logged.
struct Ran {
    passed: bool,
    words: String,
    logs: Vec<String>,
}

fn run_one(path: &Path, p: &Project, record: Option<&mut InstallRecord>) -> Result<Ran, String> {
    let guest_ran = |outcome: guest::FixtureOutcome| Ran {
        passed: outcome.passed(),
        words: outcome.words(),
        logs: Vec::new(),
    };
    match p.kind {
        ModKind::Parser => {
            let fixture = guest::ParserFixture::from_file(path)?;
            guest::run_parser_fixture(&fixture, &p.component, Limits::default(), record).map(guest_ran)
        },
        ModKind::Viewer => {
            let fixture = guest::ViewerFixture::from_file(path)?;
            guest::run_viewer_fixture(&fixture, &p.component, Limits::default(), record).map(guest_ran)
        },
        ModKind::Document => {
            let fixture = Fixture::from_file(path)?;
            let outcome = run_fixture(
                &fixture,
                &p.component,
                &p.grant(),
                Quota::default(),
                None,
                Vec::new(),
                record,
            )?;
            Ok(match outcome {
                FixtureOutcome::Pass(run) => Ran { passed: true, words: "pass".into(), logs: run.guest_logs },
                mismatch => {
                    let logs = match &mismatch {
                        FixtureOutcome::Mismatch { run, .. } => run.guest_logs.clone(),
                        FixtureOutcome::Pass(_) => Vec::new(),
                    };
                    Ran { passed: false, words: mismatch.words(), logs }
                },
            })
        },
    }
}

impl ModProjectHost {
    /// Run every `fixtures/*.toml` against the project's component; a pass,
    /// or the diff of each mismatch, is kept for the section.
    pub fn run_fixtures(&self, id: &str) -> Result<String, String> {
        self.with(id, |p, state| {
            let files = super::fixture_files(p);
            if files.is_empty() {
                return Err(format!("{id} has no fixtures in {}", p.root.join("fixtures").display()));
            }
            self.check_in(id, p, state)?;
            let mut record = self.runtime.record(id);
            let (mut lines, mut passed, mut logs) = (Vec::new(), 0, Vec::new());
            for path in &files {
                let name = path
                    .file_stem()
                    .map_or_else(String::new, |n| n.to_string_lossy().into());
                match run_one(path, p, record.as_mut()) {
                    Ok(ran) if ran.passed => {
                        passed += 1;
                        lines.push(format!("{name}: pass"));
                        logs = ran.logs;
                    },
                    Ok(ran) => {
                        lines.push(format!("{name}: mismatch"));
                        logs = ran.logs;
                        lines.extend(ran.words.lines().map(|l| format!("  {l}")));
                    },
                    Err(why) => {
                        Self::note_in(state, format!("{id}: fixture {name}: {why}"));
                        lines.push(format!("{name}: {why}"));
                    },
                }
            }
            if let Some(trap) = record.and_then(|r| r.last_trap) {
                p.last_trap = Some(trap.note);
            }
            let tally = format!("{passed} of {} fixtures passed", files.len());
            lines.push(tally.clone());
            p.fixtures = lines;
            p.keep_logs(&logs);
            if passed == files.len() {
                super::promote(p);
                Ok(tally)
            } else {
                Err(format!("{id}: {tally}"))
            }
        })
    }
}
