// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Explicitly match a graph's media clock to the recording opened by the user.

use super::view::{PracticeView, button, label, row};
use super::{State, format_time};
use cambium::TextInput;
use std::sync::mpsc;
use turnstone_ontology::Passage;

pub(super) struct MatchedSource {
    pub note: String,
    pub source: url::Url,
}

enum Finished {
    Read(Vec<Passage>),
    Saved(std::path::PathBuf),
}

#[derive(Default)]
pub(super) struct SelectionState {
    work: Option<mpsc::Receiver<Result<Finished, String>>>,
    candidates: Vec<Passage>,
    index: usize,
    matched: Option<Passage>,
}

impl SelectionState {
    pub(super) fn busy(&self) -> bool {
        self.work.is_some()
    }

    fn start(
        &mut self,
        operation: impl FnOnce() -> Result<Finished, String> + Send + 'static,
    ) -> Result<(), String> {
        let (send, receive) = mpsc::channel();
        std::thread::Builder::new()
            .name("practice-selection".into())
            .spawn(move || {
                let _ = send.send(operation());
            })
            .map_err(|error| error.to_string())?;
        self.work = Some(receive);
        Ok(())
    }

    fn load_file(&mut self, path: std::path::PathBuf) -> Result<(), String> {
        self.start(move || {
            let profile = turnstone_ontology::selected_profile().map_err(str::to_owned)?;
            turnstone_ontology::passages_from_file(profile, &path).map(Finished::Read)
        })?;
        self.candidates.clear();
        self.index = 0;
        Ok(())
    }

    pub(super) fn poll(&mut self) -> Option<String> {
        let result = match self.work.as_ref()?.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("The timed selection worker stopped.".into())
            },
        };
        self.work = None;
        Some(match result {
            Err(error) => error,
            Ok(Finished::Saved(path)) => format!("Saved matched clip to {}.", path.display()),
            Ok(Finished::Read(mut candidates)) => {
                candidates.retain(|p| matches!(p.time_range_ms(), Some((_, Some(_)))));
                self.candidates = candidates;
                if self.candidates.is_empty() {
                    "This document has no bounded timed selections under the active lens.".into()
                } else {
                    String::new()
                }
            },
        })
    }

    pub(super) fn handoff(&self, start: u64, end: u64) -> Result<Option<MatchedSource>, String> {
        let Some(passage) = &self.matched else {
            return Ok(None);
        };
        if passage.time_range_ms() != Some((start, Some(end))) {
            return Err(
                "Restore the matched times or clear the match to send edited marks.".into(),
            );
        }
        Ok(Some(MatchedSource {
            note: passage.note.clone(),
            source: passage.source_address(),
        }))
    }
}

impl State {
    fn choose_selection(&mut self) -> Result<(), String> {
        if self.selections.busy() {
            return Ok(());
        }
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Choose a document with timed selections")
            .pick_file()
        {
            self.selections.load_file(path)?;
            self.status = "Reading timed selections…".into();
        }
        Ok(())
    }

    fn save_matched(&mut self) -> Result<(), String> {
        if self.selections.busy() {
            return Ok(());
        }
        let request = self.recording_import()?;
        let Some(destination) = rfd::FileDialog::new()
            .set_title("Save matched clip")
            .add_filter("WAV recording", &["wav"])
            .set_file_name(format!("{} clip.wav", self.title))
            .save_file()
        else {
            return Ok(());
        };
        self.selections.start(move || {
            request.save(&destination)?;
            Ok(Finished::Saved(destination))
        })?;
        self.status = "Saving matched WAV…".into();
        Ok(())
    }

    fn apply_selection(&mut self) -> Result<(), String> {
        let passage = self
            .selections
            .candidates
            .get(self.selections.index)
            .ok_or("Choose a timed selection first.")?;
        let (start, Some(end)) = passage
            .time_range_ms()
            .ok_or("The selection has no clock.")?
        else {
            return Err("Choose a selection with both an in and an out point.".into());
        };
        if end > self.duration || end <= start {
            return Err("The selected interval does not fit this recording.".into());
        }
        self.start = TextInput::new(format_time(start));
        self.end = TextInput::new(format_time(end));
        self.selections.matched = Some(passage.clone());
        self.selections.candidates.clear();
        self.selections.index = 0;
        self.status.clear();
        Ok(())
    }
}

pub(super) fn controls(state: &State) -> Vec<PracticeView> {
    let mut controls = vec![row(vec![button(
        "Match timed selection…",
        State::choose_selection,
    )])];
    let selections = &state.selections;
    if let Some(p) = selections.candidates.get(selections.index) {
        controls.push(label(format!(
            "{} of {} · {} · {}",
            selections.index + 1,
            selections.candidates.len(),
            p.label(),
            p.note
        )));
        let mut actions = Vec::new();
        if selections.index > 0 {
            actions.push(button("Previous selection", |s| {
                s.selections.index = s.selections.index.saturating_sub(1);
                Ok(())
            }));
        }
        if selections.index + 1 < selections.candidates.len() {
            actions.push(button("Next selection", |s| {
                if s.selections.index + 1 < s.selections.candidates.len() {
                    s.selections.index += 1;
                }
                Ok(())
            }));
        }
        actions.push(button(
            "Match this recording to this source",
            State::apply_selection,
        ));
        controls.push(row(actions));
    }
    if let Some(p) = &selections.matched {
        controls.push(row(vec![
            label(format!(
                "Matched source: {} · {}",
                p.source_address(),
                p.note
            )),
            button("Save matched clip…", State::save_matched),
            button("Clear match", |s| {
                s.selections.matched = None;
                Ok(())
            }),
        ]));
    }
    controls
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
