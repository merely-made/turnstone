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

#[derive(Clone, Default)]
enum OwnerMode {
    #[default]
    Public,
    #[cfg(feature = "alembic-workshop-private")]
    Private { endpoint: String },
}
type CustodyObserver = Arc<dyn Fn() -> bool + Send + Sync>;

fn open_owner(
    root: &std::path::Path,
    wake: Arc<dyn Fn() + Send + Sync>,
    mode: &OwnerMode,
) -> Result<(GraphshellWorkshop, Option<CustodyObserver>), String> {
    match mode {
        OwnerMode::Public => {
            pollster::block_on(GraphshellWorkshop::open(root, wake)).map(|owner| (owner, None))
        },
        #[cfg(feature = "alembic-workshop-private")]
        OwnerMode::Private { endpoint } => {
            use graphshell_endpoint::door::{
                custody_identity::CustodyIdentity, identity::VaultLockView,
            };
            let identity = Arc::new(
                CustodyIdentity::connect_at_timeout(
                    endpoint,
                    "graphshell",
                    std::time::Duration::from_secs(2),
                )
                .map_err(|e| e.to_string())?,
            );
            let status = identity.connected_status();
            if status.lock != VaultLockView::Unlocked
                || !status
                    .custody_epoch
                    .as_ref()
                    .is_some_and(|epoch| epoch.valid())
            {
                return Err("An authoritative unlocked custody binding is required".into());
            }
            let human = status
                .persona_public_key
                .ok_or("Custody disclosed no principal")?;
            let profile = status
                .profile
                .as_ref()
                .ok_or("Custody disclosed no profile")?
                .0
                .clone();
            let watched = identity.clone();
            let protection = Arc::new(athanor::workshop_seal::EideticProtection::new(
                identity.clone(),
                move || watched.custody_epoch(),
            ));
            let watched = identity.clone();
            let observer: CustodyObserver = Arc::new(move || {
                watched
                    .status_timeout(std::time::Duration::from_millis(500))
                    .is_ok()
                    && watched.custody_epoch().is_some()
            });
            if !observer() {
                return Err("Private custody binding changed while opening".into());
            }
            let owner = pollster::block_on(GraphshellWorkshop::open_sealed(
                root, wake, protection, human, profile,
            ))?;
            Ok((owner, Some(observer)))
        },
    }
}

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
    mode: OwnerMode,
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
    let (mut owner, custody) = match open_owner(&root, worker_wake, &mode) {
        Ok((owner, custody)) => (Some(owner), custody),
        Err(error) => {
            state.error = Some(error);
            state.busy = true;
            state.status = "Workshop owner unavailable.".into();
            (None, None)
        },
    };
    if let Some(owner) = &owner {
        if let Err(error) = pollster::block_on(owner.refresh_surface(&mut state)) {
            state.error = Some(error);
        }
        state.status = if custody.is_some() {
            "Private retained-source workshop. Select two to four admitted saved passages.".into()
        } else {
            "Public example workshop. Load the retained example to begin; the reviewer identity is synthetic.".into()
        };
    }
    let _ = updates.send(Update::Surface {
        state: state.clone(),
        handled,
    });
    wake();
    loop {
        // This service thread owns I/O and joining; the native event loop only
        // applies DTOs. An idle private window still observes lock/loss.
        if let Some(observe) = &custody {
            let _ = observe(); // Fresh status invalidates the SDK epoch on loss.
            if owner
                .as_mut()
                .is_some_and(|owner| owner.observe_custody(&mut state))
            {
                let _ = updates.send(Update::Surface {
                    state: state.clone(),
                    handled,
                });
                wake();
            }
        }
        let command = match commands.recv_timeout(std::time::Duration::from_millis(250)) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
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
    #[cfg(feature = "alembic-workshop-private")]
    if first == "--private-probe" {
        let stage = args.next().ok_or("missing private stage")?;
        let root = PathBuf::from(args.next().ok_or("missing private directory")?);
        let endpoint = args
            .next()
            .ok_or("missing explicit fixture broker endpoint")?
            .into_string()
            .map_err(|_| "invalid endpoint")?;
        return private_probe(root, stage.to_str().ok_or("invalid stage")?, endpoint);
    }
    let (root, mode) = if first == "--private-workshop" {
        #[cfg(feature = "alembic-workshop-private")]
        {
            (
                PathBuf::from(args.next().ok_or("missing private directory")?),
                OwnerMode::Private {
                    endpoint: args
                        .next()
                        .ok_or("missing explicit fixture broker endpoint")?
                        .into_string()
                        .map_err(|_| "invalid endpoint")?,
                },
            )
        }
        #[cfg(not(feature = "alembic-workshop-private"))]
        {
            return Err("Private receiving is not enabled in this binary".into());
        }
    } else {
        (PathBuf::from(first), OwnerMode::Public)
    };
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
                owner_service(root, mode, receive_commands, worker_commands, updates, wake)
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
            owner_service(
                root,
                OwnerMode::Public,
                receive,
                worker_send,
                updates,
                Arc::new(|| {}),
            )
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

/// A process receiving receipt exposes only hashes and state. Private passages
/// and derived note text never go to the normal diagnostic/log stream.
#[cfg(feature = "alembic-workshop-private")]
fn private_probe(
    root: PathBuf,
    stage: &str,
    endpoint: String,
) -> Result<(), Box<dyn std::error::Error>> {
    use alembic::workshop::{Control, Review};
    use std::time::{Duration, Instant};
    let (mut owner, custody) =
        open_owner(&root, Arc::new(|| {}), &OwnerMode::Private { endpoint })?;
    let mut state = SurfaceState::new();
    pollster::block_on(owner.refresh_surface(&mut state))?;
    if stage == "seed" {
        for (i, text) in [
            "PRIVATE-SOURCE-GRAPHSHELL-7fa alpha",
            "PRIVATE-SOURCE-GRAPHSHELL-7fa café 🦉",
            "PRIVATE-SOURCE-GRAPHSHELL-7fa omega",
        ]
        .iter()
        .enumerate()
        {
            pollster::block_on(owner.retain_private_source(
                uuid::Uuid::from_u128(i as u128 + 1),
                format!("Private fixture {}", i + 1),
                (*text).into(),
                0,
                text.len() as u64,
            ))?;
        }
        state.choices.clear();
        pollster::block_on(owner.refresh_surface(&mut state))?;
    } else if stage == "generate" {
        for choice in &mut state.choices {
            choice.selected = true;
        }
        state.request(SurfaceAction::Run);
        owner.service(&mut state);
        let deadline = Instant::now() + Duration::from_secs(30);
        while owner.busy() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            owner.service(&mut state);
        }
        if owner.busy() {
            return Err("Private worker did not complete".into());
        }
    } else if stage == "accept" || stage == "reconcile" || stage == "dissolve" {
        state.request(SurfaceAction::Control(match stage {
            "accept" => Control::Accept,
            "reconcile" => Control::Reconcile,
            _ => Control::Dissolve,
        }));
        owner.service(&mut state);
    } else if stage == "inspect" {
        let view = state.view.as_ref().ok_or("No private view")?;
        let proposed = view.proposal.as_ref().ok_or("No retained proposal")?;
        let mut actions = proposed
            .sources
            .iter()
            .map(|source| SurfaceAction::OpenSource {
                proposal: proposed.digest,
                source: source.source.clone(),
            })
            .collect::<Vec<_>>();
        if let Some(Review::Added { receipt, .. }) = &view.review {
            actions.push(SurfaceAction::OpenResult(receipt.resource.clone()));
        }
        for action in actions {
            state.request(action);
        }
        owner.service(&mut state);
    } else {
        return Err("Unknown private probe stage".into());
    }
    if let Some(error) = state.error.take() {
        return Err(error.into());
    }
    if !custody.as_ref().is_some_and(|observe| observe()) {
        owner.observe_custody(&mut state);
        return Err("Private custody changed".into());
    }
    let view = state.view.as_ref();
    let proposed = view.and_then(|v| v.proposal.as_ref());
    let receipt = view
        .and_then(|v| v.review.as_ref())
        .and_then(|review| match review {
            Review::Added { receipt, .. } => Some(receipt),
            _ => None,
        });
    let summary = serde_json::json!({
        "schema": 1, "owner": owner.owner(), "nodes": owner.graph().node_count(), "choices": state.choices.len(),
        "phase": view.map(|v| &v.phase), "proposal_digest": proposed.map(|p| p.digest),
        "source_versions": proposed.map(|p| p.sources.iter().map(|p| &p.source.version).collect::<Vec<_>>()),
        "all_private": proposed.map(|p| p.sources.iter().all(|p| !p.public)),
        "note_digest": proposed.map(|p| blake3::hash(p.djot.as_bytes()).to_hex().to_string()),
        "reading_digest": state.reading.as_ref().map(|r| blake3::hash(r.text.as_bytes()).to_hex().to_string()),
        "receipt": receipt,
    });
    owner.close()?;
    println!("{}", serde_json::to_string(&summary)?);
    Ok(())
}
