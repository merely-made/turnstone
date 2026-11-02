// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl TurnstoneScryFactory {
    pub(super) fn build_native(
        &self,
        request: &SurfaceSpawnRequest,
    ) -> Result<PlatformWebSurfaceProducer, SurfaceError> {
        require_owner_thread(self.owner)?;
        let profile = profile_path(&request.profile.user_data_dir)?;
        if request.fence_handle.is_some() {
            return Err(SurfaceError::SpawnFailed(
                "Scry capture uses a dedicated producer fence; a raw spawn fence cannot replace its synchronizer".into(),
            ));
        }
        let fence = Arc::new(
            Dx12FenceSynchronizer::new(&self.host)
                .map_err(|error| spawn_error("create producer DX12 fence", error))?,
        );
        std::fs::create_dir_all(&profile).map_err(|error| {
            SurfaceError::SpawnFailed(format!(
                "could not create Scry profile {}: {error}",
                profile.display()
            ))
        })?;
        let root = self.composition_root()?;
        let config = PlatformWebSurfaceConfig::new(
            PhysicalSize::new(request.width.max(1), request.height.max(1)),
            &profile,
        )
        .with_dx12_fence_synchronizer(fence.clone());
        // SAFETY: root owns the live offscreen HWND, every attached producer
        // retains it, and this factory rejects construction off its UI thread.
        let producer = unsafe { PlatformWebSurfaceProducer::new_attached(&root, config) }
            .map_err(|error| spawn_error("construct WebView2 capture producer", error))?;
        // Start the requested URL promptly. Completion belongs to the ordered
        // event stream, rather than a blocking navigation wait in spawn.
        producer
            .load_url(&request.url)
            .map_err(|error| spawn_error("start initial navigation", error))?;
        let mut fences = self.fences.lock().map_err(|_| {
            SurfaceError::SpawnFailed("Scry producer-fence lookup was poisoned".into())
        })?;
        fences.retain(|_, fence| fence.strong_count() != 0);
        fences.insert(profile, Arc::downgrade(&fence));
        Ok(producer)
    }
}
