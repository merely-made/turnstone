// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Public evidence-brief workshop through Graphshell's canonical MereHost.
//! The graph/workshop owner is constructed on one service thread. The native
//! event loop exchanges DTOs only; it performs no journal or destination I/O.

use alembic::{
    workshop_native::{native_hooks, native_init},
    workshop_surface::{SurfaceAction, SurfaceState},
};
use cambium_genet_winit_host::{CloseDisposition, HostOptions, run};
use graphshell::alembic_workshop::GraphshellWorkshop;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::{Arc, mpsc},
};

enum Command {
    Actions {
        serial: u64,
        actions: Vec<SurfaceAction>,
        selected: Vec<String>,
    },
    WorkerWake,
    Close,
    Shutdown,
}
enum Update {
    Surface { state: SurfaceState, handled: u64 },
    Closed(Result<(), String>),
}

fn owner_service(
    root: PathBuf,
    commands: mpsc::Receiver<Command>,
    sender: mpsc::Sender<Command>,
    updates: mpsc::Sender<Update>,
    wake: Arc<dyn Fn() + Send + Sync>,
) -> Result<(), String> {
    let mut state = SurfaceState::new();
    let mut handled = 0;
    let worker_wake = Arc::new(move || {
        let _ = sender.send(Command::WorkerWake);
    });
    let mut owner = match pollster::block_on(GraphshellWorkshop::open(&root, worker_wake)) {
        Ok(owner) => Some(owner),
        Err(error) => {
            state.error = Some(error);
            state.busy = true;
            state.status = "Workshop owner unavailable.".into();
            None
        },
    };
    if let Some(owner) = &owner {
        if let Err(error) = pollster::block_on(owner.refresh_surface(&mut state)) {
            state.error = Some(error);
        }
        state.status = "Public example workshop. Load the retained example to begin; the reviewer identity is synthetic.".into();
    }
    let _ = updates.send(Update::Surface {
        state: state.clone(),
        handled,
    });
    wake();
    while let Ok(command) = commands.recv() {
        match command {
            Command::Close => {
                let result = owner.as_mut().map_or(Ok(()), GraphshellWorkshop::close);
                let complete = result.is_ok();
                let _ = updates.send(Update::Closed(result));
                wake();
                if complete {
                    return Ok(());
                }
            },
            Command::Shutdown => {
                // The event loop is already gone, so a failed save cannot wait
                // for another UI retry. Driver Drop still observes worker join.
                return owner.as_mut().map_or(Ok(()), GraphshellWorkshop::close);
            },
            Command::Actions {
                serial,
                actions,
                selected,
            } => {
                handled = serial;
                if owner.is_none() {
                    state.take_actions();
                    state.busy = true;
                    state.error = Some(
                        "The workshop owner is unavailable; close this window before retrying."
                            .into(),
                    );
                    let _ = updates.send(Update::Surface {
                        state: state.clone(),
                        handled,
                    });
                    wake();
                    continue;
                }
                for choice in &mut state.choices {
                    choice.selected = selected.contains(&choice.key);
                }
                for action in actions {
                    state.request(action);
                }
                if let Some(owner) = owner.as_mut() {
                    owner.service(&mut state);
                }
                let _ = updates.send(Update::Surface {
                    state: state.clone(),
                    handled,
                });
                wake();
            },
            Command::WorkerWake => {
                if let Some(owner) = owner.as_mut() {
                    owner.service(&mut state);
                }
                let _ = updates.send(Update::Surface {
                    state: state.clone(),
                    handled,
                });
                wake();
            },
        }
    }
    owner.as_mut().map_or(Ok(()), GraphshellWorkshop::close)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let first = args.next().ok_or("usage: graphshell-alembic <public-workshop-directory> | --probe <generate|accept|reconcile|inspect> <directory>")?;
    if first == "--probe" {
        let stage = args.next().ok_or("missing probe stage")?;
        let root = args
            .next()
            .map(PathBuf::from)
            .ok_or("missing probe directory")?;
        return probe(root, stage.to_str().ok_or("invalid probe stage")?);
    }
    let root = PathBuf::from(first);
    let (commands, receive_commands) = mpsc::channel();
    let (updates, receive_updates) = mpsc::channel();
    let thread = Rc::new(RefCell::new(None));
    let closing = Rc::new(Cell::new(false));
    let sent = Rc::new(Cell::new(0u64));
    let mut hooks = native_hooks();
    let dispatch = commands.clone();
    let sent_dispatch = sent.clone();
    hooks.after_dispatch = Box::new(move |ctx| {
        ctx.runner.update(|state| {
            let actions = state.take_actions();
            if !actions.is_empty() {
                let selected = state.selected();
                let Some(serial) = sent_dispatch.get().checked_add(1) else {
                    state.error = Some("The window request limit was reached; reopen it.".into());
                    return;
                };
                if dispatch
                    .send(Command::Actions {
                        serial,
                        actions,
                        selected,
                    })
                    .is_err()
                {
                    state.error = Some("The workshop owner stopped.".into());
                } else {
                    sent_dispatch.set(serial);
                    state.busy = true;
                }
            }
        });
    });
    let close_pending = closing.clone();
    hooks.after_wake = Box::new(move |ctx| {
        while let Ok(update) = receive_updates.try_recv() {
            match update {
                Update::Surface { state, handled } => {
                    // Ignore an earlier response while a newer request awaits
                    // its owner; otherwise it could re-enable stale controls.
                    if handled >= sent.get() {
                        ctx.runner
                            .update(|current| current.apply_owner_update(state));
                    }
                },
                Update::Closed(Ok(())) => {
                    *ctx.close = true;
                },
                Update::Closed(Err(error)) => {
                    close_pending.set(false);
                    ctx.runner.update(|state| state.error = Some(error));
                },
            }
        }
    });
    let close = commands.clone();
    hooks.close_request = Box::new(move |ctx, _| {
        if !closing.replace(true) && close.send(Command::Close).is_err() {
            ctx.runner.update(|state| {
                state.error =
                    Some("The workshop owner stopped before close was acknowledged.".into())
            });
            return CloseDisposition::Exit;
        }
        CloseDisposition::KeepVisible
    });
    let service_thread = thread.clone();
    let worker_commands = commands.clone();
    let result = run(
        HostOptions {
            title: "Graphshell · Alembic".into(),
            initial_logical_size: (980., 900.),
            ..HostOptions::default()
        },
        move |_, window_commands, wake| {
            let wake = wake.callback();
            *service_thread.borrow_mut() = Some(std::thread::spawn(move || {
                owner_service(root, receive_commands, worker_commands, updates, wake)
            }));
            let mut state = SurfaceState::new();
            state.busy = true;
            state.status = "Opening workshop owner…".into();
            native_init(state, window_commands)
        },
        hooks,
    );
    // Covers an event-loop exit that bypasses the ordinary acknowledged close.
    let _ = commands.send(Command::Shutdown);
    if let Some(thread) = thread.borrow_mut().take() {
        thread
            .join()
            .map_err(|_| "workshop owner thread failed")??;
    }
    result?;
    Ok(())
}

/// Fresh-process persistence diagnostics drive the same owner requests as the
/// native controls. They establish storage behavior, not headed UI evidence.
fn probe(root: PathBuf, stage: &str) -> Result<(), Box<dyn std::error::Error>> {
    use alembic::workshop::{Control, Review};
    use std::time::{Duration, Instant};
    let mut owner = pollster::block_on(GraphshellWorkshop::open(&root, Arc::new(|| {})))?;
    let mut state = SurfaceState::new();
    pollster::block_on(owner.refresh_surface(&mut state))?;
    match stage {
        "generate" => {
            state.request(SurfaceAction::LoadExample);
            owner.service(&mut state);
            if let Some(error) = state.error.take() {
                return Err(error.into());
            }
            state.request(SurfaceAction::Run);
            owner.service(&mut state);
            let deadline = Instant::now() + Duration::from_secs(5);
            while owner.busy() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
                owner.service(&mut state);
            }
            if owner.busy() {
                return Err("workshop worker did not complete".into());
            }
        },
        "accept" => {
            state.request(SurfaceAction::Control(Control::Accept));
            owner.service(&mut state);
        },
        "reconcile" => {
            state.request(SurfaceAction::Control(Control::Reconcile));
            owner.service(&mut state);
        },
        "inspect" => {
            if let Some(Review::Added { receipt, .. }) =
                state.view.as_ref().and_then(|v| v.review.as_ref())
            {
                state.request(SurfaceAction::OpenResult(receipt.resource.clone()));
                owner.service(&mut state);
            }
        },
        _ => return Err("unknown probe stage".into()),
    }
    if let Some(error) = state.error {
        return Err(error.into());
    }
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "owner": owner.owner(), "view": state.view,
            "reading": state.reading.map(|r| r.text), "nodes": owner.graph().nodes().count(),
        }))?
    );
    owner.close()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn workshop_native_failed_owner_disables_actions_and_shutdown_joins() {
        let directory = tempfile::tempdir().unwrap();
        let held = pollster::block_on(GraphshellWorkshop::open(directory.path(), Arc::new(|| {})))
            .unwrap();
        let root = directory.path().to_owned();
        let (send, receive) = mpsc::channel();
        let worker_send = send.clone();
        let (updates, responses) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            owner_service(root, receive, worker_send, updates, Arc::new(|| {}))
        });
        let Update::Surface { state, handled } =
            responses.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
            panic!("expected owner response")
        };
        assert_eq!(handled, 0);
        assert!(state.busy && state.error.is_some());
        send.send(Command::Actions {
            serial: 1,
            actions: vec![SurfaceAction::LoadExample],
            selected: vec![],
        })
        .unwrap();
        let Update::Surface { state, handled } =
            responses.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
            panic!("expected refused owner response")
        };
        assert_eq!(handled, 1);
        assert!(state.busy && state.error.as_ref().unwrap().contains("unavailable"));
        send.send(Command::Shutdown).unwrap();
        assert!(thread.join().unwrap().is_ok());
        assert_eq!(held.graph().nodes().count(), 0);
    }
}
