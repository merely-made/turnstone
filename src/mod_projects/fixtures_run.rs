// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Running a project's fixtures (D6): turns in, the pinned DOM and turn log out.

use document_host::{Fixture, FixtureOutcome, Quota, run_fixture};

use super::ModProjectHost;

impl ModProjectHost {
    /// Run every `fixtures/*.toml` against the project's component; a pass,
    /// or the diff of each mismatch, is kept for the section.
    pub fn run_fixtures(&self, id: &str) -> Result<String, String> {
        self.with(id, |p, state| {
            let dir = p.root.join("fixtures");
            let mut files: Vec<_> = std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|path| path.extension().is_some_and(|x| x == "toml"))
                .collect();
            files.sort();
            if files.is_empty() {
                return Err(format!("{id} has no fixtures in {}", dir.display()));
            }
            let mut record = self.runtime.record(id);
            let (mut lines, mut passed, mut logs) = (Vec::new(), 0, Vec::new());
            for path in &files {
                let name = path
                    .file_stem()
                    .map_or_else(String::new, |n| n.to_string_lossy().into());
                let outcome = Fixture::from_file(path).and_then(|fixture| {
                    run_fixture(
                        &fixture,
                        &p.component,
                        &self.grant(p),
                        Quota::default(),
                        None,
                        Vec::new(),
                        record.as_mut(),
                    )
                });
                match outcome {
                    Ok(FixtureOutcome::Pass(run)) => {
                        passed += 1;
                        lines.push(format!("{name}: pass"));
                        logs = run.guest_logs;
                    },
                    Ok(mismatch) => {
                        lines.push(format!("{name}: mismatch"));
                        if let FixtureOutcome::Mismatch { run, .. } = &mismatch {
                            logs.clone_from(&run.guest_logs);
                        }
                        lines.extend(mismatch.words().lines().map(|l| format!("  {l}")));
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
                Ok(tally)
            } else {
                Err(format!("{id}: {tally}"))
            }
        })
    }
}
