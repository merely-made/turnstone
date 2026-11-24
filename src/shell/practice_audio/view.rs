// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use cambium::{
    AnyView, GenetCtx, GenetElement, PointerClick, TextInput, clickable, el, lens, text,
    text_field_typed,
};

use super::{Command, State};

pub(super) const SHEET: &str = "
.practice { background: #f4f3ed; color: #1f282b; min-height: 100%; padding: 10px 16px; font: 13px sans-serif; }
.practice-title { margin: 0 0 3px; font-size: 21px; font-weight: bold; }
.practice-row { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; margin: 4px 0; }
.practice-button, .practice-field [role=textbox] { background: white; color: #1f282b; border: 1px solid #728080; border-radius: 4px; padding: 4px 7px; }
.practice-field { display: inline-flex; align-items: center; gap: 5px; }
.practice-field [role=textbox] { box-sizing: border-box; min-width: 76px; min-height: 25px; line-height: 15px; }
.practice-note [role=textbox] { min-width: 240px; }
.practice-status { margin: 2px 0; }
.practice-row, .practice-status { background-color: #f4f3ed; }
";

pub(super) type PracticeView = Box<dyn AnyView<State, (), GenetCtx, GenetElement>>;

pub(super) fn button(label: &'static str, operation: fn(&mut State) -> Result<(), String>) -> PracticeView {
    Box::new(clickable(
        el::<_, State, ()>("button", text(label)).attr("class", "practice-button"),
        move |state: &mut State, _: PointerClick| {
            if let Err(error) = operation(state) {
                state.status = error;
            }
        },
    ))
}

fn field(
    label: &'static str,
    member: fn(&mut State) -> &mut TextInput,
    class: &'static str,
) -> PracticeView {
    let input = Box::new(lens(
        move |input: &mut TextInput| text_field_typed(input).attr("aria-label", label),
        member,
    )) as PracticeView;
    Box::new(
        el::<_, State, ()>("label", (el::<_, State, ()>("span", label), input))
            .attr("class", class),
    )
}

pub(super) fn row(children: Vec<PracticeView>) -> PracticeView {
    Box::new(el::<_, State, ()>("div", children).attr("class", "practice-row"))
}

pub(super) fn label(message: String) -> PracticeView {
    Box::new(el::<_, State, ()>("span", message))
}

pub(super) fn render(state: &State) -> PracticeView {
    let title =
        Box::new(el::<_, State, ()>("h1", state.title.clone()).attr("class", "practice-title"))
            as PracticeView;
    if state.duration == 0 {
        let action = match &state.preparing {
            Some(work) if work.autoplay => "Cancel queued playback",
            Some(_) => "Play when ready",
            None => "Retry preview",
        };
        let rows = vec![
            title,
            row(vec![button(action, State::play)]),
            label(state.status.clone()),
        ];
        return Box::new(el::<_, State, ()>("div", rows).attr("class", "practice"));
    }
    let mut rows: Vec<PracticeView> = vec![
        title,
        row(vec![
            button(
                if state.preparing.is_some() {
                    "Cancel preview"
                } else if state.playing {
                    "Pause preview"
                } else {
                    "Play preview"
                },
                State::play,
            ),
            label(format!(
                "{:.3} / {:.3} s",
                state.position as f64 / 1000.0,
                state.duration as f64 / 1000.0
            )),
            field("Seek (s)", |state| &mut state.seek, "practice-field"),
            button("Seek", State::seek),
        ]),
        row(vec![
            button("Mark in", |state| state.mark(true)),
            field("In (s)", |state| &mut state.start, "practice-field"),
            button("Mark out", |state| state.mark(false)),
            field("Out (s)", |state| &mut state.end, "practice-field"),
        ]),
        row(vec![field(
            "Practice note",
            |state| &mut state.note,
            "practice-field practice-note",
        )]),
        row(vec![button("Practice in Woodshed", State::send)]),
    ];
    rows.extend(super::selection::controls(state));
    if state.attached {
        let wav = state
            .remote
            .as_ref()
            .and_then(|status| status.get("wav_loop"));
        let playing = wav
            .and_then(|loop_status| loop_status.get("playing"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let mut transport = vec![
            if playing {
                button("Pause loop", |state| state.remote_command(Command::Pause))
            } else {
                button("Play loop", |state| state.remote_command(Command::Play))
            },
            field("Gain (%)", |state| &mut state.gain, "practice-field"),
            button("Set gain", |state| {
                let value = state
                    .gain
                    .text()
                    .trim()
                    .parse::<u8>()
                    .ok()
                    .filter(|value| *value <= 100)
                    .ok_or("Enter gain from 0 to 100.")?;
                state.remote_command(Command::Gain(value))
            }),
            button("Detach transport", |state| {
                state.remote_command(Command::Detach)
            }),
        ];
        if let Some(wav) = wav {
            let rate = wav
                .get("sample_rate")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let cursor = wav
                .get("region_cursor")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let region = wav
                .get("region_frames")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            if rate != 0 {
                transport.push(label(format!(
                    "{:.2} / {:.2} s",
                    cursor as f64 / rate as f64,
                    region as f64 / rate as f64
                )));
            }
        }
        rows.push(row(transport));
    } else if state.imported {
        rows.push(row(vec![button("Attach transport", |state| {
            state.remote_command(Command::Attach)
        })]));
    }
    if !state.status.is_empty() {
        rows.push(Box::new(
            el::<_, State, ()>("p", state.status.clone())
                .attr("role", "status")
                .attr("class", "practice-status"),
        ));
    }
    if state.choose_executable {
        let mut choices: Vec<_> = state
            .executable_choices
            .iter()
            .map(|path| {
                let path = path.clone();
                Box::new(clickable(
                    el::<_, State, ()>("button", path.display().to_string())
                        .attr("class", "practice-button"),
                    move |state: &mut State, _: PointerClick| {
                        if let Err(error) = state.select_executable(path.clone()) {
                            state.status = error;
                        }
                    },
                )) as PracticeView
            })
            .collect();
        choices.push(button("Browse for Woodshed", State::browse_executable));
        rows.push(row(choices));
    }
    Box::new(el::<_, State, ()>("div", rows).attr("class", "practice"))
}
