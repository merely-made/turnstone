// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn caller_capture_retirement(window: u64) {
    for retirement in 0..4 {
        let mut f = CallerCaptureFixture::new(window, None);
        f.press(0, inker::MouseButton::Left);
        f.assert_a_custody();
        match retirement {
            0 => f.sync(false), // Genuine native focus loss.
            1 => {
                let placements = f
                    .plan()
                    .into_iter()
                    .filter(|s| s.id != f.surfaces[0].id)
                    .map(|surface| {
                        super::super::super::content_pool::HostPlacement::for_focus(
                            surface,
                            window,
                            true,
                            f.app.focus,
                            1.,
                        )
                    })
                    .collect();
                f.contents.sync_placements(
                    placements,
                    Arc::new(inker::SessionRegistry::new()),
                    Arc::new(inker::SurfaceEngineRegistry::new()),
                    std::time::Instant::now(),
                );
            },
            2 => {
                let placements = f
                    .plan()
                    .into_iter()
                    .map(|surface| {
                        let source_window = if surface.id == f.surfaces[0].id {
                            window + 1
                        } else {
                            window
                        };
                        super::super::super::content_pool::HostPlacement::for_focus(
                            surface,
                            source_window,
                            true,
                            f.app.focus,
                            1.,
                        )
                    })
                    .collect();
                f.contents.sync_placements(
                    placements,
                    Arc::new(inker::SessionRegistry::new()),
                    Arc::new(inker::SurfaceEngineRegistry::new()),
                    std::time::Instant::now(),
                );
            },
            _ => {
                f.contents.insert(f.nodes[0], hosted(&Default::default()));
            },
        }
        assert_eq!(
            captured_mouse(&f.contents.pool),
            None,
            "real retirement must still cancel custody"
        );
        assert_eq!(
            f.pointers[0]
                .lock()
                .unwrap()
                .iter()
                .map(|e| e.phase)
                .collect::<Vec<_>>(),
            [PointerPhase::Down, PointerPhase::Cancel]
        );
        assert_eq!(
            f.buttons[&window],
            PointerButtons::PRIMARY,
            "Cancel is not physical release"
        );
        let position = f.position(1);
        assert!(
            native_mouse_input(
                &mut f.contents.pool,
                &mut f.buttons,
                window,
                None,
                position,
                PointerPhase::Up,
                Some(inker::MouseButton::Left),
                Default::default()
            )
            .is_none()
        );
        assert_eq!(f.buttons[&window], PointerButtons::NONE);
        assert_eq!(f.pointers[0].lock().unwrap().len(), 2);
        let (source, effect) = f.press(1, inker::MouseButton::Middle);
        assert_eq!(source, ContentKey::Node(f.nodes[1]));
        assert!(effect.handled);
        assert_eq!(
            f.app.focus,
            FocusTarget::Content {
                node: f.nodes[1],
                appearance: f.surfaces[1].id
            }
        );
        assert_eq!(captured_mouse(&f.contents.pool), Some(f.surfaces[1].id));
        assert_eq!(f.pointers[1].lock().unwrap().len(), 1);
    }
}
