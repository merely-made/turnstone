// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn raster_capture_refusal(
    failure: genet_render_host::RasterizeFailure,
    generation: u64,
    source_key: u64,
) -> cambium_rootstock::CaptureRefusal {
    match failure {
        genet_render_host::RasterizeFailure::DeviceLost(notice) => {
            cambium_rootstock::CaptureRefusal::DeviceLost(notice)
        },
        genet_render_host::RasterizeFailure::SourceError(error) => {
            cambium_rootstock::CaptureRefusal::SurfaceUnavailable { generation, error }
        },
        genet_render_host::RasterizeFailure::ProviderError(error) => {
            cambium_rootstock::CaptureRefusal::ProviderError {
                generation,
                source_key: Some(source_key),
                error,
            }
        },
    }
}
