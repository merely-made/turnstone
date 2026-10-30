// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use genet_render_host::{DeviceLossNotice, RecoveryStatus, SharedRenderCore};
use std::sync::atomic::{AtomicUsize, Ordering};

fn loss() -> DeviceLossNotice {
    DeviceLossNotice {
        generation: 908,
        reason: wgpu::DeviceLostReason::Unknown,
        message: "CPU-only recovery protocol control".into(),
    }
}
fn owner() -> (Arc<SharedRenderCore>, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let bindings = Arc::new(AtomicUsize::new(2));
    let counted = calls.clone();
    let alive = bindings.clone();
    let owner = SharedRenderCore::from_detached_loss(
        loss(),
        Arc::new(move || {
            assert_eq!(
                alive.load(Ordering::SeqCst),
                0,
                "factory precedes released host bindings"
            );
            counted.fetch_add(1, Ordering::SeqCst);
            Err("bounded failed factory, no GPU boot claimed".into())
        }),
    );
    (owner, calls, bindings)
}
fn window(owner: &SharedRenderCore) -> WindowRecovery {
    WindowRecovery::new(owner, Arc::new(|| {}))
}

#[test]
fn render_recovery_both_windows_release_before_one_sticky_factory() {
    let (owner, calls, bindings) = owner();
    let mut primary = window(&owner);
    let mut hidden_lens = window(&owner);
    primary.detach(&owner, 908, true, || {
        bindings.fetch_sub(1, Ordering::SeqCst);
    });
    assert!(matches!(
        owner.recover(),
        RecoveryStatus::Waiting {
            remaining_hosts: 1,
            ..
        }
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    primary.detach(&owner, 908, true, || {
        panic!("duplicate notice released the host twice")
    });
    hidden_lens.detach(&owner, 908, true, || {
        bindings.fetch_sub(1, Ordering::SeqCst);
    });
    let mut errors = Vec::new();
    assert!(!admit_recovered_core(owner.recover(), &mut errors));
    assert!(!admit_recovered_core(owner.recover(), &mut errors));
    assert_eq!(
        errors,
        ["device-loss recovery failed generation=908: bounded failed factory, no GPU boot claimed"]
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn render_recovery_closed_lens_leaves_barrier_and_stale_ack_does_not() {
    let (owner, calls, bindings) = owner();
    let mut primary = window(&owner);
    let lens = window(&owner);
    primary.detach(&owner, 907, true, || {});
    assert!(matches!(
        owner.recover(),
        RecoveryStatus::Waiting {
            remaining_hosts: 2,
            ..
        }
    ));
    // Actual close drops the subscription only after its host bindings.
    bindings.fetch_sub(1, Ordering::SeqCst);
    drop(lens);
    primary.detach(&owner, 908, true, || {
        bindings.fetch_sub(1, Ordering::SeqCst);
    });
    assert!(matches!(owner.recover(), RecoveryStatus::Failed { .. }));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn render_recovery_unsupported_binding_never_opens_replacement_barrier() {
    let (owner, calls, bindings) = owner();
    let mut primary = window(&owner);
    let mut lens = window(&owner);
    primary.detach(&owner, 908, false, || {
        bindings.fetch_sub(1, Ordering::SeqCst);
    });
    lens.detach(&owner, 908, true, || {
        bindings.fetch_sub(1, Ordering::SeqCst);
    });
    assert!(matches!(
        owner.recover(),
        RecoveryStatus::Waiting {
            remaining_hosts: 1,
            ..
        }
    ));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "unsupported consumer was falsely detached"
    );
}

#[test]
fn render_capture_terminal_takes_only_its_window_once() {
    let mut main = Some(std::path::PathBuf::from("primary.png"));
    let mut lens = Some(std::path::PathBuf::from("lens.png"));
    assert!(take_window_capture(3, Some(2), &mut main, &mut lens).is_none());
    assert!(main.is_some() && lens.is_some());
    assert_eq!(
        take_window_capture(2, Some(2), &mut main, &mut lens),
        Some(std::path::PathBuf::from("lens.png"))
    );
    assert!(take_window_capture(2, Some(2), &mut main, &mut lens).is_none());
    assert!(main.is_some());
    assert_eq!(
        take_window_capture(0, Some(2), &mut main, &mut lens),
        Some(std::path::PathBuf::from("primary.png"))
    );
    assert!(take_window_capture(0, Some(2), &mut main, &mut lens).is_none());
}

#[test]
fn render_capture_refusal_preserves_provider_and_installed_loss_identity() {
    match raster_capture_refusal(
        genet_render_host::RasterizeFailure::SourceError("unsupported border".into()),
        23,
        71,
    ) {
        cambium_rootstock::CaptureRefusal::SurfaceUnavailable { generation, error } => {
            assert_eq!(generation, 23);
            assert_eq!(error, "unsupported border");
        },
        other => panic!("source failure was not preserved: {other:?}"),
    }
    let error = Arc::new(netrender::VelloError::CurveQualityReadbackFailed);
    match raster_capture_refusal(
        genet_render_host::RasterizeFailure::ProviderError(error.clone()),
        23,
        71,
    ) {
        cambium_rootstock::CaptureRefusal::ProviderError {
            generation,
            source_key,
            error: actual,
        } => {
            assert_eq!(generation, 23);
            assert_eq!(source_key, Some(71));
            assert!(Arc::ptr_eq(&error, &actual));
        },
        other => panic!("provider failure was misclassified: {other:?}"),
    }
    match raster_capture_refusal(
        genet_render_host::RasterizeFailure::DeviceLost(loss()),
        23,
        71,
    ) {
        cambium_rootstock::CaptureRefusal::DeviceLost(actual) => assert_eq!(actual, loss()),
        other => panic!("installed loss was misclassified: {other:?}"),
    }
}
