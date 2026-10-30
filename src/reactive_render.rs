// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Evidence at the product's actual Genet calls and native queue submission.
//! This reports executed work, never inferred work or physical presentation.

use cambium::composition_scheduler::MutationRecord;
use cambium::reactive_diagnostics::{
    CauseRelation, DiagnosticDetails, DiagnosticKind, DiagnosticReason, Producer, ProducerRef,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

const MAX_INPUTS: usize = 256;
static NEXT_FRAME: AtomicU64 = AtomicU64::new(1);
static NEXT_WINDOW: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static ACTIVE: RefCell<Option<Rc<RefCell<FrameState>>>> = const { RefCell::new(None) };
    static WORK: RefCell<Option<Rc<RefCell<WorkState>>>> = const { RefCell::new(None) };
    static COVERAGE_GAPS: Cell<u64> = const { Cell::new(0) };
    static WINDOWS: RefCell<Vec<(Weak<winit::window::Window>, u64)>> = const { RefCell::new(Vec::new()) };
}

struct FrameState {
    producer: Option<Producer>,
    frame: u64,
    window: u64,
    inputs: Vec<ProducerRef>,
    omitted: usize,
    outputs: Vec<ProducerRef>,
    submitted: bool,
}

/// A window-scoped render attempt. Dropping an unsubmitted attempt records
/// that outcome, including early acquisition, raster or device refusals.
pub struct FrameScope {
    state: Rc<RefCell<FrameState>>,
    previous: Option<Rc<RefCell<FrameState>>>,
    staged: bool,
}

/// A render attempt staged for deferred native presentation. It reports its
/// submission at the actual present, or its refusal if dropped unpresented.
pub(crate) struct StagedFrame(Rc<RefCell<FrameState>>);

impl FrameScope {
    /// Assign an opaque identity to this actual native window lifetime. A
    /// reused platform WindowId or lens slot cannot reuse the old identity.
    pub(crate) fn native(window: &Arc<winit::window::Window>) -> Self {
        let identity = WINDOWS.with(|windows| {
            let Ok(mut windows) = windows.try_borrow_mut() else {
                gap();
                return 0;
            };
            windows.retain(|(window, _)| window.strong_count() > 0);
            if let Some((_, id)) = windows.iter().find(|(weak, _)| {
                weak.upgrade()
                    .is_some_and(|live| Arc::ptr_eq(&live, window))
            }) {
                return *id;
            }
            if windows.len() >= 64 {
                gap();
                return 0;
            }
            let id = NEXT_WINDOW
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                .unwrap_or_else(|_| {
                    gap();
                    0
                });
            windows.push((Arc::downgrade(window), id));
            id
        });
        Self::begin(identity)
    }

    pub fn begin(window: u64) -> Self {
        let frame = NEXT_FRAME
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .unwrap_or_else(|_| {
                gap();
                0
            });
        let state = Rc::new(RefCell::new(FrameState {
            producer: Producer::new().map_err(|_| gap()).ok(),
            frame,
            window,
            inputs: Vec::new(),
            omitted: 0,
            outputs: Vec::new(),
            submitted: false,
        }));
        let previous = ACTIVE.with(|active| active.replace(Some(state.clone())));
        Self { state, previous, staged: false }
    }

    /// Called only after `host.queue().present(frame)` returns. Wgpu queue
    /// submission is not a proof of a physical display or captured pixels.
    pub fn submitted(&self, surface_generation: u64) {
        submit(&self.state, surface_generation);
    }

    /// Stop collecting and hand the attempt to its staged native frame.
    pub(crate) fn stage(mut self) -> StagedFrame {
        self.staged = true;
        StagedFrame(self.state.clone())
    }
}

impl StagedFrame {
    /// Called only after the deferred `host.queue().present(frame)` returns.
    pub(crate) fn submitted(&self, surface_generation: u64) {
        submit(&self.0, surface_generation);
    }
}

impl Drop for StagedFrame {
    fn drop(&mut self) {
        refuse_unsubmitted(&self.0);
    }
}

fn submit(cell: &RefCell<FrameState>, surface_generation: u64) {
    let Ok(state) = cell.try_borrow() else {
        gap();
        return;
    };
    let Some(producer) = state.producer.clone() else {
        gap();
        return;
    };
    let details = DiagnosticDetails {
        frame_id: (state.frame != 0).then_some(state.frame),
        window_id: (state.window != 0).then_some(state.window),
        surface_generation: Some(surface_generation),
        reason: DiagnosticReason::Accepted,
        ..Default::default()
    };
    let causes = state
        .outputs
        .iter()
        .copied()
        .map(|parent| (CauseRelation::ScopeContains, parent))
        .collect::<Vec<_>>();
    drop(state);
    if producer
        .emit_with_causes(DiagnosticKind::HostFrameSubmitted, None, details, &causes)
        .is_err()
    {
        gap();
    }
    if let Ok(mut state) = cell.try_borrow_mut() {
        state.submitted = true;
    } else {
        gap();
    }
}

impl Drop for FrameScope {
    fn drop(&mut self) {
        if !self.staged {
            refuse_unsubmitted(&self.state);
        }
        ACTIVE.with(|active| {
            active.replace(self.previous.take());
        });
    }
}

fn refuse_unsubmitted(cell: &RefCell<FrameState>) {
    let snapshot = cell.try_borrow().ok().and_then(|state| {
        (!state.submitted).then(|| {
            (
                state.producer.clone(),
                state.frame,
                state.window,
                state.outputs.clone(),
            )
        })
    });
    if let Some((Some(producer), frame, window, outputs)) = snapshot {
        let causes = outputs
            .into_iter()
            .map(|p| (CauseRelation::ScopeContains, p))
            .collect::<Vec<_>>();
        if producer
            .emit_with_causes(
                DiagnosticKind::HostFrameUnavailable,
                None,
                DiagnosticDetails {
                    frame_id: (frame != 0).then_some(frame),
                    window_id: (window != 0).then_some(window),
                    reason: DiagnosticReason::Refused,
                    ..Default::default()
                },
                &causes,
            )
            .is_err()
        {
            gap();
        }
    }
}

fn gap() {
    COVERAGE_GAPS.with(|gaps| gaps.set(gaps.get().saturating_add(1)));
}

pub fn coverage_gaps() -> u64 {
    COVERAGE_GAPS.with(Cell::get)
}
pub(crate) fn mark_input_gap() {
    gap();
}

/// Transfer explicit revision references from a visible Settings owner into
/// this window's render attempt. Hidden and foreign-window owners are omitted.
pub fn include_mutations(records: &[MutationRecord]) {
    let Some(active) = ACTIVE.with(|slot| slot.try_borrow().ok().and_then(|v| v.clone())) else {
        return;
    };
    let Ok(mut state) = active.try_borrow_mut() else {
        gap();
        return;
    };
    for record in records.iter().filter(|record| record.visible) {
        if state.inputs.contains(&record.reference) {
            continue;
        }
        if state.inputs.len() < MAX_INPUTS {
            state.inputs.push(record.reference);
        } else {
            state.omitted = state.omitted.saturating_add(1);
            gap();
        }
    }
}

struct WorkState {
    frame: Rc<RefCell<FrameState>>,
    inputs: Vec<ProducerRef>,
    last_work: Option<ProducerRef>,
    last_kind: Option<DiagnosticKind>,
}

/// An actual DOM/layout call scope. Only callers that own the rendered DOM
/// enter it: an outer window frame, or a standalone Settings pane. Other UI
/// roots do not acquire Settings causes merely because they render nearby.
pub(crate) struct WorkScope {
    state: Option<Rc<RefCell<WorkState>>>,
    previous: Option<Rc<RefCell<WorkState>>>,
}

impl WorkScope {
    pub(crate) fn window() -> Self {
        Self::enter(None)
    }
    pub(crate) fn settings(records: &[MutationRecord]) -> Self {
        Self::enter(Some(
            records
                .iter()
                .filter(|r| r.visible)
                .map(|r| r.reference)
                .collect(),
        ))
    }
    fn enter(inputs: Option<Vec<ProducerRef>>) -> Self {
        let frame = ACTIVE.with(|slot| slot.try_borrow().ok().and_then(|v| v.clone()));
        let Some(frame) = frame else {
            return Self {
                state: None,
                previous: None,
            };
        };
        let inputs = inputs.unwrap_or_else(|| {
            frame
                .try_borrow()
                .map(|f| f.inputs.clone())
                .unwrap_or_default()
        });
        let state = Rc::new(RefCell::new(WorkState {
            frame,
            inputs,
            last_work: None,
            last_kind: None,
        }));
        let previous = WORK.with(|slot| slot.replace(Some(state.clone())));
        Self {
            state: Some(state),
            previous,
        }
    }
}

impl Drop for WorkScope {
    fn drop(&mut self) {
        let Some(state) = &self.state else {
            return;
        };
        if let Ok(state) = state.try_borrow()
            && let Some(last) = state.last_work
            && let Ok(mut frame) = state.frame.try_borrow_mut()
        {
            if frame.outputs.len() < MAX_INPUTS {
                frame.outputs.push(last);
            } else {
                gap();
            }
        }
        WORK.with(|slot| {
            slot.replace(self.previous.take());
        });
    }
}

/// Marks one completed call at its actual execution boundary. A retained
/// layout hit emits no style/layout work; paint is recorded only after paint.
pub(crate) fn completed(kind: DiagnosticKind) {
    let Some(active) = WORK.with(|slot| slot.try_borrow().ok().and_then(|v| v.clone())) else {
        return;
    };
    let Ok(state) = active.try_borrow() else {
        gap();
        return;
    };
    let Ok(frame) = state.frame.try_borrow() else {
        gap();
        return;
    };
    let Some(producer) = frame.producer.clone() else {
        gap();
        return;
    };
    let details = DiagnosticDetails {
        frame_id: (frame.frame != 0).then_some(frame.frame),
        window_id: (frame.window != 0).then_some(frame.window),
        notice_count: Some(frame.omitted as u64),
        reason: if kind == DiagnosticKind::RendererWorkFailed {
            DiagnosticReason::Refused
        } else {
            DiagnosticReason::Accepted
        },
        ..Default::default()
    };
    let mut causes = state
        .inputs
        .iter()
        .copied()
        .map(|parent| (CauseRelation::RenderedFrom, parent))
        .collect::<Vec<_>>();
    if let Some(previous) = state.last_work
        && matches!(
            (state.last_kind, kind),
            (
                Some(DiagnosticKind::RendererStyleCompleted),
                DiagnosticKind::RendererLayoutCompleted | DiagnosticKind::RendererWorkFailed
            ) | (
                Some(DiagnosticKind::RendererLayoutCompleted),
                DiagnosticKind::RendererPaintCompleted
            )
        )
    {
        // Keep the preceding work as an explicit edge, preserving the
        // style -> layout -> paint chain in addition to represented revisions.
        causes.insert(0, (CauseRelation::ProducedBy, previous));
    }
    drop(frame);
    drop(state);
    if let Ok(reference) = producer.emit_with_causes(kind, None, details, &causes) {
        if let Ok(mut state) = active.try_borrow_mut() {
            state.last_work = Some(reference);
            state.last_kind = Some(kind);
        } else {
            gap();
        }
    } else {
        gap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scopes_restore_parent_and_do_not_change_work_without_a_subscriber() {
        let first = FrameScope::begin(0);
        let outer = first.state.clone();
        {
            let second = FrameScope::begin(1);
            {
                let _work = WorkScope::window();
                completed(DiagnosticKind::RendererPaintCompleted);
            }
            second.submitted(7);
        }
        assert!(ACTIVE.with(|slot| Rc::ptr_eq(slot.borrow().as_ref().unwrap(), &outer)));
        {
            let _work = WorkScope::window();
            completed(DiagnosticKind::RendererPaintCompleted);
        }
        first.submitted(9);
        drop(first);
        assert!(ACTIVE.with(|slot| slot.borrow().is_none()));
    }
}
