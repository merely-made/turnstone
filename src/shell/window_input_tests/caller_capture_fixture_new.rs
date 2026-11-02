// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl CallerCaptureFixture {
    pub(super) fn new(window: u64, document: Option<bool>) -> Self {
        use crate::action::{Action, WbAxis};
        let mut app = crate::app::App::test_stub();
        app.viewport = (1200., 900.);
        let mut nodes = Vec::new();
        for address in ["https://upper.test/index", "https://lower.test/index"] {
            app.update(Action::OpenAddress(address.into()));
            nodes.push(app.graph_runtimes.focused_member().unwrap());
            app.update(Action::OpenInWorkbench);
        }
        app.update(Action::WorkbenchSplitBeside {
            dragged: nodes[1],
            target: nodes[0],
            axis: WbAxis::Row,
            after: true,
        });
        if window != 0 {
            app.update(Action::TearOutActivePane);
        }
        let nodes = [nodes[0], nodes[1]];
        let pointers = [
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(Vec::new())),
        ];
        let inputs = [
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(Vec::new())),
        ];
        let mut contents = super::super::super::content_pool::NodeDocuments::default();
        for (index, node) in nodes.iter().enumerate() {
            let content = if let Some(submit) = document {
                let address = if index == 0 {
                    "https://upper.test/index"
                } else {
                    "https://lower.test/index"
                };
                let controller = pelt_core::PeltController::from_session_shared(
                    Arc::new(inker::SessionRegistry::new()),
                    Arc::new(inker::SurfaceEngineRegistry::new()),
                    pelt_core::PeltControllerConfig::new("probe", address, (100, 100))
                        .with_host_history(),
                    super::super::super::ShellClock(std::time::Instant::now()),
                    Box::new(OverlapDocument {
                        inputs: inputs[index].clone(),
                        submit,
                    }),
                );
                PeltContent::from_controller(controller, route())
            } else {
                PeltContent::from_surface(
                    Box::new(Page {
                        calls: Default::default(),
                        pointers: pointers[index].clone(),
                        acquisitions: Default::default(),
                        title: None,
                        fail_offset: None,
                    }),
                    (100, 100),
                    route(),
                )
            };
            contents.insert(*node, content);
        }
        let plan = if window == 0 {
            super::super::super::primary_content_plan(&app, &contents, 1200, 900)
        } else {
            super::super::super::lens::lens_content_plan(&app, &contents, 0, 1200, 900)
        };
        let surfaces = nodes.map(|node| {
            *plan
                .iter()
                .find(|s| {
                    s.kind == SurfaceKind::Content(node)
                        && crate::surface::hit_test(
                            &plan,
                            app.focus,
                            s.rect.x + s.rect.w / 2.,
                            s.rect.y + s.rect.h / 2.,
                        )
                        .is_some_and(|hit| hit.id == s.id)
                })
                .expect("actual portable plan exposes both separate content appearances")
        });
        assert_ne!(surfaces[0].id, surfaces[1].id);
        assert_ne!(surfaces[0].rect, surfaces[1].rect);
        let mut fixture = Self {
            app,
            contents,
            window,
            nodes,
            surfaces,
            pointers,
            inputs,
            buttons: HashMap::new(),
            scale: 1.,
        };
        fixture.sync(true);
        fixture
    }

    pub(super) fn plan(&self) -> Vec<Surface> {
        if self.window == 0 {
            super::super::super::primary_content_plan(&self.app, &self.contents, 1200, 900)
        } else {
            super::super::super::lens::lens_content_plan(&self.app, &self.contents, 0, 1200, 900)
        }
    }

    pub(super) fn sync(&mut self, active: bool) {
        let placements = self
            .plan()
            .into_iter()
            .map(|surface| {
                super::super::super::content_pool::HostPlacement::for_focus(
                    surface,
                    self.window,
                    active,
                    self.app.focus,
                    self.scale,
                )
            })
            .collect();
        self.contents.sync_placements(
            placements,
            Arc::new(inker::SessionRegistry::new()),
            Arc::new(inker::SurfaceEngineRegistry::new()),
            std::time::Instant::now(),
        );
    }

    pub(super) fn position(&self, index: usize) -> PhysicalPosition {
        let r = self.surfaces[index].rect;
        PhysicalPosition {
            x: r.x + r.w / 2.,
            y: r.y + r.h / 2.,
        }
    }

    pub(super) fn press(
        &mut self,
        index: usize,
        button: inker::MouseButton,
    ) -> (ContentKey, PeltHostEffect) {
        let position = self.position(index);
        assert!(
            native_mouse_input(
                &mut self.contents.pool,
                &mut self.buttons,
                self.window,
                None,
                position,
                PointerPhase::Down,
                Some(button),
                Default::default()
            )
            .is_none()
        );
        let plan = self.plan();
        let hit = crate::surface::hit_test(&plan, self.app.focus, position.x, position.y).unwrap();
        assert_eq!(hit.id, self.surfaces[index].id);
        let target = content_press_target(
            &mut self.app.focus,
            &self.contents.pool,
            self.window,
            position,
            hit,
        );
        // Primary content_input(Focus(true)) syncs first. Lens content_mouse
        // syncs directly. Both use the real shared placement construction.
        if self.window == 0 {
            self.sync(true);
            self.contents.pool.input(
                &target.id,
                PeltInput::Session(inker::SessionInput::Focus(true)),
            );
        }
        self.sync(true);
        let placement = self.contents.pool.placement(&target.id).unwrap();
        let (x, y) = (
            placement.rect.x * placement.scale_factor,
            placement.rect.y * placement.scale_factor,
        );
        content_mouse_input(
            &mut self.contents.pool,
            &self.buttons,
            self.window,
            Some(target.id),
            PhysicalPosition {
                x: target.local.0 + x,
                y: target.local.1 + y,
            },
            PointerPhase::Down,
            Some(button),
            Default::default(),
        )
        .unwrap()
    }

    pub(super) fn pointer(
        &mut self,
        phase: PointerPhase,
        button: Option<inker::MouseButton>,
    ) -> (ContentKey, PeltHostEffect) {
        self.sync(true);
        let position = self.position(1);
        native_mouse_input(
            &mut self.contents.pool,
            &mut self.buttons,
            self.window,
            Some(self.surfaces[1].id),
            position,
            phase,
            button,
            Default::default(),
        )
        .unwrap()
    }

    pub(super) fn assert_a_custody(&self) {
        assert_eq!(
            captured_mouse(&self.contents.pool),
            Some(self.surfaces[0].id),
            "admitted second Down must retain the first appearance capture through caller focus and sync"
        );
        assert_eq!(
            self.app.focus,
            FocusTarget::Content {
                node: self.nodes[0],
                appearance: self.surfaces[0].id
            }
        );
        assert!(
            self.contents
                .pool
                .placement(&self.surfaces[0].id)
                .unwrap()
                .focused
        );
        assert!(
            !self
                .contents
                .pool
                .placement(&self.surfaces[1].id)
                .unwrap()
                .focused
        );
    }
}
