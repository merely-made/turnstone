//! The Shell Transcript pane: the visible half of `ShellServices`.
//!
//! [`crate::shell_services::ShellTranscript`] has recorded every intentional
//! shell interaction since A6 landed its data model, with correlation ids,
//! targets, outcomes, and a `repeat` that mints a fresh entry from an old one.
//! None of it was reachable: `ChromeBlueprint`'s transcript placement defaults
//! to `Hidden` and nothing rendered it. This is the projection, so the plan's
//! clause -- a command "can be repeated from a docked or floating transcript"
//! -- has something to be repeated *from*.
//!
//! Two things it deliberately is not. It is not a tracing console: the
//! transcript is a typed record of what a person asked for, so Steward keeps
//! operational status and Comms keeps conversation. And it does not re-derive
//! outcomes; it reads the ledger the shell already wrote, so what a row claims
//! happened is what the shell recorded, not a second opinion.

use std::cell::RefCell;
use std::rc::Rc;

use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, ListRow, ListSection, PointerClick,
    el, sectioned_list,
};
use genet_layout::{IncrementalLayout, ScrollOffsets};
use genet_scripted_dom::{NodeId, ScriptedDom};

use crate::shell_services::{
    EntryPrivacy, ShellEntry, ShellEntryId, ShellInput, ShellIntent, ShellOutcome, ShellTranscript,
};

/// What a transcript row activation asks the shell to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptPaneAction {
    /// Run this entry's intent again, minting a fresh correlated entry.
    Repeat(ShellEntryId),
}

impl cambium::Action for TranscriptPaneAction {}

/// One row as the view holds it: the entry's identity plus already-derived
/// text, so laying out never re-reads the ledger.
#[derive(Clone, Debug, PartialEq)]
struct TranscriptRow {
    id: ShellEntryId,
    input: String,
    outcome: String,
    /// Whether the entry is still awaiting its result. A pending row offers no
    /// repeat, because repeating a command whose first run has not landed is
    /// how a person doubles an effect by accident.
    pending: bool,
}

struct TranscriptState {
    rows: Vec<TranscriptRow>,
    viewport_w: f32,
    viewport_h: f32,
}

type TranscriptView =
    Box<dyn AnyView<TranscriptState, TranscriptPaneAction, GenetCtx, GenetElement>>;
type TranscriptRunner = GenetAppRunner<
    TranscriptState,
    fn(&TranscriptState) -> TranscriptView,
    TranscriptView,
    TranscriptPaneAction,
>;

/// How an entry's input reads in a row.
///
/// A redacted entry shows its label and never its text. The ledger is local,
/// but a transcript is the one surface that puts old input back on screen long
/// after the person who typed it stopped thinking about it.
fn input_text(entry: &ShellEntry) -> String {
    match &entry.input {
        ShellInput::Redacted { label } => format!("{label} (redacted)"),
        ShellInput::Omnibar(text) => match entry.privacy {
            EntryPrivacy::Redacted => "(redacted)".to_string(),
            EntryPrivacy::Ordinary => text.clone(),
        },
    }
}

/// How an entry's result reads in a row.
fn outcome_text(entry: &ShellEntry) -> String {
    match &entry.outcome {
        ShellOutcome::Pending => "running".to_string(),
        ShellOutcome::Completed { summary } => summary.clone(),
        ShellOutcome::Rejected { message } => format!("refused: {message}"),
    }
}

/// The intent's own name, which is what a person recognises a past command by.
fn intent_text(entry: &ShellEntry) -> String {
    match &entry.resolved_intent {
        ShellIntent::SelectNode { url } => format!("select {url}"),
        ShellIntent::Navigate { url } => format!("go {url}"),
        ShellIntent::Command { label, .. } => label.clone(),
    }
}

fn rows_from(transcript: &ShellTranscript) -> Vec<TranscriptRow> {
    // Newest first: a transcript is read from the recent end.
    let mut rows: Vec<TranscriptRow> = transcript
        .entries()
        .map(|entry| TranscriptRow {
            id: entry.id,
            input: format!("{} - {}", input_text(entry), intent_text(entry)),
            outcome: outcome_text(entry),
            pending: matches!(entry.outcome, ShellOutcome::Pending),
        })
        .collect();
    rows.reverse();
    rows
}

fn transcript_pane_view(state: &TranscriptState) -> TranscriptView {
    // A pending row is muted, so it is visibly not activatable: repeating a
    // command whose first run has not landed is how a person doubles an
    // effect by accident.
    let rows: Vec<ListRow> = state
        .rows
        .iter()
        .map(|row| {
            let text = format!("{} - {}", row.input, row.outcome);
            if row.pending {
                ListRow::muted(text)
            } else {
                ListRow::action(text)
            }
        })
        .collect();

    let title = if state.rows.is_empty() {
        "No commands yet"
    } else {
        "Recent commands"
    };
    let sections = vec![ListSection::new(title, rows)];
    let list = sectioned_list(
        &sections,
        |state: &mut TranscriptState, _si: usize, ri: usize| -> Option<TranscriptPaneAction> {
            let row = state.rows.get(ri)?;
            (!row.pending).then_some(TranscriptPaneAction::Repeat(row.id))
        },
    );

    Box::new(
        el::<_, TranscriptState, TranscriptPaneAction>("div", list)
            .attr("class", "pane")
            .attr(
                "style",
                format!(
                    "width: {}px; height: {}px;",
                    state.viewport_w, state.viewport_h
                ),
            ),
    )
}

/// The Transcript pane: a retained cambium runner over the shell ledger.
pub struct TranscriptPane {
    dom: DomHandle,
    runner: TranscriptRunner,
}

impl Default for TranscriptPane {
    fn default() -> Self {
        Self::new()
    }
}

impl TranscriptPane {
    pub fn new() -> Self {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = TranscriptState {
            rows: Vec::new(),
            viewport_w: 0.0,
            viewport_h: 0.0,
        };
        let runner = TranscriptRunner::new(
            dom.clone(),
            transcript_pane_view as fn(&TranscriptState) -> TranscriptView,
            state,
        );
        Self { dom, runner }
    }

    /// Refresh from the shell ledger at the pane's size.
    pub fn sync(&mut self, transcript: &ShellTranscript, pane_w: f32, pane_h: f32) {
        let rows = rows_from(transcript);
        self.runner.update(|state| {
            state.rows = rows;
            state.viewport_w = pane_w;
            state.viewport_h = pane_h;
        });
    }

    pub fn scene(&self, w: u32, h: u32) -> netrender::Scene {
        crate::ui::scene_from_dom(&self.dom.borrow(), crate::ui::CAMBIUM_SHEET, w, h)
    }

    /// Route a click at pane-local `(x, y)`, the same round trip the Trail and
    /// Roster panes take.
    pub fn click(&mut self, x: f32, y: f32, w: u32, h: u32) -> Vec<TranscriptPaneAction> {
        let hit = {
            let dom = self.dom.borrow();
            let layout =
                IncrementalLayout::new(&*dom, &[crate::ui::CAMBIUM_SHEET], w as f32, h as f32);
            let scroll = ScrollOffsets::<NodeId>::default();
            layout.hit_test(&*dom, x, y, &scroll)
        };
        match hit {
            Some(node) => self.runner.dispatch_click(node, PointerClick::at((x, y))),
            None => Vec::new(),
        }
    }

    pub fn resolve(&self, sel: &genet_probe::Selector, rect: [f32; 4]) -> Option<(f32, f32)> {
        let dom = self.dom.borrow();
        let surfaces = [genet_probe::ProbeSurface {
            name: "transcript",
            dom: &dom,
            rect,
            sheet: crate::ui::CAMBIUM_SHEET,
        }];
        genet_probe::resolve(&surfaces, sel).map(|h| h.point)
    }

    pub fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.dom.borrow()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell_services::ContextSnapshot;

    const RECT: [f32; 4] = [0.0, 0.0, 512.0, 600.0];
    const W: u32 = 512;
    const H: u32 = 600;

    fn ledger() -> ShellTranscript {
        ShellTranscript::default()
    }

    /// Record a completed navigation and hand back its id.
    fn completed(transcript: &mut ShellTranscript, url: &str) -> ShellEntryId {
        let id = transcript.record(
            ShellInput::Omnibar(url.to_string()),
            ShellIntent::Navigate {
                url: url.to_string(),
            },
            ContextSnapshot::default(),
            EntryPrivacy::Ordinary,
        );
        transcript.complete(
            id,
            ShellOutcome::Completed {
                summary: format!("opened {url}"),
            },
        );
        id
    }

    fn synced(transcript: &ShellTranscript) -> TranscriptPane {
        let mut pane = TranscriptPane::new();
        pane.sync(transcript, 512.0, 600.0);
        pane
    }

    /// A6's third clause: a command "can be repeated from a docked or floating
    /// transcript". The repeat mechanism already existed; until this pane there
    /// was nothing to repeat it *from*, because the transcript's placement
    /// defaults to Hidden and nothing rendered it.
    #[test]
    fn a_recorded_command_appears_and_its_row_repeats_that_entry() {
        let mut transcript = ledger();
        let id = completed(&mut transcript, "mere://field-notes");
        let mut pane = synced(&transcript);

        let (x, y) = pane
            .resolve(
                &genet_probe::Selector::class("list-row").containing("field-notes"),
                RECT,
            )
            .expect("the entry is drawn as a row");

        assert_eq!(
            pane.click(x, y, W, H),
            vec![TranscriptPaneAction::Repeat(id)],
            "activating the row repeats that entry, by id"
        );
    }

    /// A pending entry is muted and offers no repeat. Repeating a command whose
    /// first run has not landed is how a person doubles an effect by accident,
    /// so the row must be inert rather than merely discouraged.
    #[test]
    fn a_pending_entry_offers_no_repeat() {
        let mut transcript = ledger();
        transcript.record(
            ShellInput::Omnibar("mere://slow".into()),
            ShellIntent::Navigate {
                url: "mere://slow".into(),
            },
            ContextSnapshot::default(),
            EntryPrivacy::Ordinary,
        );
        let mut pane = synced(&transcript);

        let (x, y) = pane
            .resolve(
                &genet_probe::Selector::class("list-row").containing("slow"),
                RECT,
            )
            .expect("a pending entry is still shown");
        assert!(
            pane.click(x, y, W, H).is_empty(),
            "a pending row is inert, not repeatable"
        );
    }

    /// A redacted entry never puts its text back on screen. The ledger is
    /// local, but the transcript is the one surface that shows old input long
    /// after the person who typed it stopped thinking about it.
    #[test]
    fn a_redacted_entry_never_shows_its_input() {
        let mut transcript = ledger();
        let id = transcript.record(
            ShellInput::Omnibar("hunter2-the-secret".into()),
            ShellIntent::Command {
                label: "Unlock vault".into(),
                action: crate::action::Action::OmnibarClose,
            },
            ContextSnapshot::default(),
            EntryPrivacy::Redacted,
        );
        transcript.complete(
            id,
            ShellOutcome::Completed {
                summary: "unlocked".into(),
            },
        );
        let pane = synced(&transcript);

        // Asked through the resolver rather than by formatting the DOM: it is
        // the same path a scenario runner would use to find the text, so a
        // miss here means the text is genuinely not on screen.
        assert!(
            pane.resolve(
                &genet_probe::Selector::class("list-row").containing("hunter2"),
                RECT
            )
            .is_none(),
            "the redacted input must not reach the pane's DOM"
        );
        assert!(
            pane.resolve(
                &genet_probe::Selector::class("list-row").containing("Unlock vault"),
                RECT
            )
            .is_some(),
            "but its label still identifies the row"
        );
    }

    /// Newest first: a transcript is read from the recent end.
    #[test]
    fn the_most_recent_command_leads() {
        let mut transcript = ledger();
        completed(&mut transcript, "mere://first");
        completed(&mut transcript, "mere://second");
        let pane = synced(&transcript);

        let first = pane
            .resolve(
                &genet_probe::Selector::class("list-row").containing("first"),
                RECT,
            )
            .expect("the older row is drawn");
        let second = pane
            .resolve(
                &genet_probe::Selector::class("list-row").containing("second"),
                RECT,
            )
            .expect("the newer row is drawn");
        assert!(
            second.1 < first.1,
            "the newer entry sits above the older one ({second:?} vs {first:?})"
        );
    }

    /// An empty ledger says so rather than drawing an unlabelled void.
    #[test]
    fn an_empty_transcript_names_itself() {
        let pane = synced(&ledger());
        assert!(
            pane.resolve(
                &genet_probe::Selector::class("list-section-title").containing("No commands yet"),
                RECT
            )
            .is_some()
        );
    }
}
