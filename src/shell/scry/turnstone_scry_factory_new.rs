// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl TurnstoneScryFactory {
    /// Construct on the UI thread that will spawn and drive all producers.
    /// Device/backend and WebView2 runtime failures are returned by `build`.
    pub(in super::super) fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self {
            id: NEXT_FACTORY_ID.fetch_add(1, Ordering::Relaxed),
            owner: std::thread::current().id(),
            host: HostWgpuContext::new(device, queue),
            fences: Mutex::new(HashMap::new()),
        }
    }

    /// Fetch immediately after successful spawn, before accepting its frames.
    /// Each node has a distinct profile, and the shell owns at most one live
    /// producer per node. Retain the returned Arc in that node's importer for
    /// as long as any imported frame may be sampled, including after close.
    pub(in super::super) fn synchronizer_for_profile(
        &self,
        profile: &Path,
    ) -> Result<Arc<Dx12FenceSynchronizer>, String> {
        self.fences
            .lock()
            .map_err(|_| "Scry producer-fence lookup was poisoned".to_owned())?
            .get(profile)
            .and_then(Weak::upgrade)
            .ok_or_else(|| "Scry has no live producer fence for this profile".to_owned())
    }

    pub(super) fn composition_root(&self) -> Result<Arc<PlatformCompositionRoot>, SurfaceError> {
        require_owner_thread(self.owner)?;
        COMPOSITION_ROOTS.with(|roots| {
            let mut roots = roots.borrow_mut();
            roots.retain(|_, root| root.strong_count() != 0);
            if let Some(root) = roots.get(&self.id).and_then(Weak::upgrade) {
                return Ok(root);
            }
            // Capture is CreateFromVisual on each pane, independent of this
            // host window's size. The root owns its HWND for every attachment.
            let root = PlatformCompositionRoot::new_offscreen(PhysicalSize::new(1, 1))
                .map_err(|error| spawn_error("create capture-only composition root", error))?;
            roots.insert(self.id, Arc::downgrade(&root));
            Ok(root)
        })
    }
}
