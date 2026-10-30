// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SurfaceProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        provider: Box<dyn SurfaceProvider>,
    ) -> Result<(), SurfaceRegistrationError> {
        let declared = match &provider.descriptor().accepted_source {
            SurfaceSourceShape::One(kind) | SurfaceSourceShape::Many(kind) => {
                Some(kind.as_str().to_owned())
            },
            SurfaceSourceShape::None => None,
        };
        if declared.as_deref() != Some(provider.source_schema().as_str()) {
            return Err(SurfaceRegistrationError::SourceShapeMismatch {
                surface_id: provider.descriptor().surface_id.clone(),
                declared,
                schema: provider.source_schema().clone(),
            });
        }
        if self.providers.iter().any(|existing| {
            existing.pane_kind() == provider.pane_kind()
                || (existing.descriptor().provider_id == provider.descriptor().provider_id
                    && existing.descriptor().surface_id == provider.descriptor().surface_id)
        }) {
            return Err(SurfaceRegistrationError::Duplicate {
                pane_kind: provider.pane_kind().clone(),
                surface_id: provider.descriptor().surface_id.clone(),
            });
        }
        self.providers.push(provider);
        Ok(())
    }

    pub fn register_provider<P: SurfaceProvider + 'static>(
        &mut self,
        provider: P,
    ) -> Result<(), SurfaceRegistrationError> {
        self.register(Box::new(provider))
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    pub fn contains_pane_kind(&self, pane_kind: &PaneKindId) -> bool {
        self.providers
            .iter()
            .any(|provider| provider.pane_kind() == pane_kind)
    }

    pub fn find(
        &self,
        pane_kind: &PaneKindId,
        source: &PaneSource,
    ) -> Option<&dyn SurfaceProvider> {
        self.providers
            .iter()
            .find(|provider| {
                provider.pane_kind() == pane_kind
                    && source_matches(provider.source_schema(), source)
            })
            .map(|provider| provider.as_ref())
    }

    /// Admit a provider with a fresh host-created `ScriptedDom` handle.
    pub fn admit(
        &self,
        pane_kind: &PaneKindId,
        source: &PaneSource,
    ) -> Result<ContributedSurfacePane, SurfaceAdmissionError> {
        let provider = self
            .providers
            .iter()
            .find(|provider| provider.pane_kind() == pane_kind)
            .ok_or_else(|| SurfaceAdmissionError::ProviderNotFound {
                pane_kind: pane_kind.clone(),
                source_schema: source_schema(source).cloned(),
            })?;
        if !source_matches(provider.source_schema(), source) {
            return Err(SurfaceAdmissionError::InvalidSource {
                expected: provider.source_schema().clone(),
                actual: source_schema(source).cloned(),
            });
        }
        let dom = fresh_dom();
        let session = match provider.admit(source, dom.clone()) {
            Ok(session) => session,
            Err(SurfaceAdmissionError::Unavailable { reason }) => {
                unavailable_surface_session(provider.descriptor().clone(), dom, reason)
            },
            Err(error) => return Err(error),
        };
        Ok(ContributedSurfacePane::new(
            pane_kind.clone(),
            source.clone(),
            session,
            provider.stylesheet(),
        ))
    }
}
