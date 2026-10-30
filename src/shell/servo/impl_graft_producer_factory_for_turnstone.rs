// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl GraftProducerFactory for TurnstoneServoFactory {
    fn build(&self, request: &SurfaceSpawnRequest) -> Result<Box<dyn GraftSurface>, SurfaceError> {
        let root = self.root()?;
        if root.options != self.options {
            return Err(spawn_error(
                "Servo named process profile differs from the initialized root",
            ));
        }
        if root.a11y_focus_policy != self.a11y_focus_policy {
            return Err(spawn_error(
                "Servo accessibility process policy differs from its factory",
            ));
        }
        // The shell resolves Servo requests to the explicitly selected process
        // profile. Refuse accidentally forwarding a per-node binding.
        if PathBuf::from(&request.profile.user_data_dir) != self.options.profile_dir {
            return Err(spawn_error(
                "Servo view requested a different process profile",
            ));
        }
        if request.fence_handle.is_some() {
            return Err(spawn_error(
                "Servo import owns synchronization; raw spawn fences are unsupported",
            ));
        }
        let url = Url::parse(&request.url).map_err(|error| spawn_error(error.to_string()))?;
        #[cfg(target_os = "linux")]
        let interop = ServoDmaBufAdapter::new(
            &self.device,
            PhysicalSize::new(request.width.max(1), request.height.max(1)),
        )
        .map_err(|error| spawn_error(error.to_string()))?;
        #[cfg(not(target_os = "linux"))]
        let interop = ServoWgpuInteropAdapter::new_with_diagnostic_sync(
            self.device.clone(),
            self.queue.clone(),
            PhysicalSize::new(request.width.max(1), request.height.max(1)),
            requested_gpu_sync()?,
        )
        .map_err(|error| spawn_error(error.to_string()))?;
        let delegate = Rc::new(Delegate {
            events: RefCell::new(VecDeque::new()),
            cursors: RefCell::new(VecDeque::new()),
            accessibility_updates: RefCell::new(VecDeque::new()),
            accessibility_active: Cell::new(false),
            accessibility_focus_provenance: Cell::new(0),
            ready: Cell::new(false),
            animating: Cell::new(false),
            requested_url: RefCell::new(request.url.clone()),
            wake: root.wake.clone(),
        });
        let view = WebViewBuilder::new(&root.servo, interop.rendering_context())
            .url(url)
            .delegate(delegate.clone())
            .build();
        root.views.set(root.views.get() + 1);
        Ok(Box::new(TurnstoneServoSurface {
            view: Some(view),
            interop: Some(interop),
            delegate,
            root,
            frame_epoch: 0,
            device: self.device.clone(),
            queue: self.queue.clone(),
            pressed_keys: HashMap::new(),
        }))
    }
}
