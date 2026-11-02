// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ProjectionSource for TurnstoneEndpoint {
    type Error = ProjectionRefusal;

    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, Self::Error> {
        if request.session != self.session {
            return Err(ProjectionRefusal::WrongSession);
        }
        if request.version.major != ProtocolVersion::V1.major {
            return Err(ProjectionRefusal::UnsupportedProtocol);
        }
        if request.score.version != sceno::SCORE_VERSION {
            return Err(ProjectionRefusal::UnsupportedScore {
                received: request.score.version,
                supported: sceno::SCORE_VERSION,
            });
        }
        let Arrangement::Spiral(spiral) = request.score.arrangement else {
            return Err(ProjectionRefusal::UnsupportedArrangement);
        };

        let graph = self.app.graph_runtimes.graph();
        let cartography::CoveredScene { scene, coverage } = disclose_scene(
            graph,
            self.app.graph_runtimes.focused_key(),
            self.card_extent,
            spiral,
        );

        let revision = Revision(graph.revision().max(1));
        let scene = SceneSnapshot::from_dense(SceneEpoch(1), revision, scene)
            .map_err(|error| ProjectionRefusal::InvalidScene(format!("{error:?}")))?;
        self.resources.clear();
        let presentation = self
            .presentation(&scene)
            .map_err(ProjectionRefusal::Presentation)?;
        let snapshot = ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: self.session.clone(),
            scene,
            presentation,
            cache_policy: CachePolicy::default(),
        };
        self.snapshot = Some(snapshot.clone());
        self.snapshot_coverage = Some(coverage);
        Ok(snapshot)
    }
}
