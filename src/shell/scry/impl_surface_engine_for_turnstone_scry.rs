// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SurfaceEngine for TurnstoneScryEngine {
    fn engine_id(&self) -> &str {
        inker::routing::ENGINE_SCRYING_WEB
    }

    fn spawn(
        &self,
        request: &SurfaceSpawnRequest,
    ) -> Result<Box<dyn SurfaceProducer>, SurfaceError> {
        let producer = Rc::new(RefCell::new(self.factory.build_native(request)?));
        let fence = self
            .factory
            .synchronizer_for_profile(Path::new(&request.profile.user_data_dir))
            .map_err(|error| spawn_error("retain producer fence", error))?;
        let handle = fence.shared_handle().0 as usize as u64;
        Ok(Box::new(TurnstoneScryProducer {
            inner: ScryingProducer::new(
                Box::new(SharedNativeProducer::new(producer.clone())),
                Some(handle),
            ),
            producer,
            _fence: fence,
        }))
    }
}
