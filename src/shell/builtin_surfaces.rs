// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The contributed surfaces the shell registers without any product wiring of their own:
//! each needs only the registry, plus the handles the shell shares with it.

use super::structured_source;
use crate::contributed_surface::SurfaceProviderRegistry;

/// Register the built-in providers. Viewer mods' providers come and go with the project list.
pub(super) fn register(
    registry: &mut SurfaceProviderRegistry,
    structured_source: &structured_source::Reader,
) {
    registry
        .register_provider(crate::knot_document_surface::KnotDocumentProvider::default())
        .expect("the built-in Knot document provider is unique");
    registry
        .register_provider(structured_source::Provider::new(structured_source.clone()))
        .expect("the source block reader provider is unique");
    #[cfg(feature = "sky")]
    registry
        .register_provider(crate::sky_surface::SkySurfaceProvider::default())
        .expect("the built-in Sky provider is unique");
    #[cfg(feature = "inference")]
    registry
        .register_provider(
            crate::distillery_installed_surface::DistilleryInstalledProvider::default(),
        )
        .expect("the built-in Distillery installed provider is unique");
}
