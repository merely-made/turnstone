// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl TurnstoneServoFactory {
    pub(crate) fn profile_name(&self) -> &str {
        &self.options.profile_name
    }

    pub(crate) fn profile_dir(&self) -> &std::path::Path {
        &self.options.profile_dir
    }

    pub(crate) fn a11y_focus_policy(&self) -> &'static str {
        self.a11y_focus_policy.name()
    }

    /// Construct exactly once on the UI thread. The configurable named profile
    /// is shared by this process's Servo views, independently of node profiles.
    pub(crate) fn new(
        options: ServoHostOptions,
        device: wgpu::Device,
        queue: wgpu::Queue,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, SurfaceError> {
        #[cfg(target_os = "linux")]
        {
            let host = grafting::HostWgpuContext::new(device.clone(), queue.clone());
            // Inspect the enabled native extensions before process-global
            // Servo initialization. Recovery cannot silently lose native import.
            let foreign =
                unsafe { device.as_hal::<wgpu::wgc::api::Vulkan>() }.is_some_and(|device| {
                    device
                        .enabled_device_extensions()
                        .contains(&c"VK_EXT_queue_family_foreign")
                });
            if host.backend != grafting::InteropBackend::Vulkan || !host.dmabuf_support || !foreign
            {
                return Err(spawn_error(
                    "Servo DMA-BUF requires a Vulkan host provisioned for foreign native import",
                ));
            }
        }
        // Refuse unknown settings before touching upstream process globals.
        // Factory reuse and every later view retain this one selection.
        let a11y_focus_policy = requested_a11y_focus_policy()?;
        if options.profile_name.trim().is_empty() || options.profile_dir.as_os_str().is_empty() {
            return Err(spawn_error(
                "Servo needs an explicitly named profile directory",
            ));
        }
        let owner = std::thread::current().id();
        if PROCESS_OWNER.get().is_some_and(|actual| *actual != owner) {
            return Err(spawn_error(
                "Servo must be constructed on its original UI thread",
            ));
        }
        ROOT.with(|slot| {
            if slot.borrow().is_some() || INITIALIZED.with(Cell::get) {
                return Err(spawn_error(
                    "Servo process root is already initialized; reuse its factory",
                ));
            }
            std::fs::create_dir_all(&options.profile_dir)
                .map_err(|error| spawn_error(error.to_string()))?;
            let opts = Opts {
                config_dir: Some(options.profile_dir.clone()),
                ..Opts::default()
            };
            // Record before upstream initialization: its global options cannot
            // safely be retried, even if construction unwinds.
            if PROCESS_OWNER.set(owner).is_err() {
                require_owner(*PROCESS_OWNER.get().expect("Servo owner was assigned"))?;
            }
            INITIALIZED.with(|initialized| initialized.set(true));
            if rustls::crypto::CryptoProvider::get_default().is_none() {
                // An existing process provider wins, including an install
                // racing this one. Servo's net initialization needs a default.
                let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
            }
            let servo = ServoBuilder::default()
                .opts(opts)
                .preferences(Preferences {
                    accessibility_enabled: true,
                    ..Preferences::default()
                })
                .event_loop_waker(Box::new(Waker(wake.clone())))
                .build();
            *slot.borrow_mut() = Some(Rc::new(ProcessRoot {
                servo,
                options: options.clone(),
                views: Cell::new(0),
                wake,
                a11y_focus_policy,
            }));
            Ok(Self {
                owner,
                options,
                device,
                queue,
                a11y_focus_policy,
            })
        })
    }

    pub(super) fn root(&self) -> Result<Rc<ProcessRoot>, SurfaceError> {
        require_owner(self.owner)?;
        ROOT.with(|slot| slot.borrow().clone())
            .ok_or_else(|| spawn_error("Servo process root is shut down"))
    }
}
