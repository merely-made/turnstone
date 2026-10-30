// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ProducerFactory for TurnstoneScryFactory {
    fn build(
        &self,
        request: &SurfaceSpawnRequest,
    ) -> Result<Box<dyn WebSurfaceProducer>, SurfaceError> {
        Ok(Box::new(self.build_native(request)?))
    }
}
