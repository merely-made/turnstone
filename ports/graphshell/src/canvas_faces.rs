// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Whether each node's face sits on its body, read back from the canvas: the
//! body where Livery drew it (by hit test), the face where the face layer
//! paints it. Both pages' `measure-faces` receipts use it (physics catalog
//! plan, the face offset ruled 2026-10-04).

use mere::canvas::Canvas;

/// One measurement over the nodes whose face lies wholly in the viewport.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FaceAlignment {
    pub zoom: f32,
    /// Nodes measured: face in view and body found.
    pub nodes: usize,
    /// Faces in view whose body was not found near its anchor.
    pub missing: usize,
    /// The largest distance from a face's centre to its body's, px.
    pub offset_max: f32,
    /// Face side over body side, the smallest and largest on either axis.
    pub ratio_min: f32,
    pub ratio_max: f32,
}

impl FaceAlignment {
    pub fn measure(canvas: &Canvas, viewport: (u32, u32)) -> Self {
        let (w, h) = (viewport.0 as f32, viewport.1 as f32);
        let mut out = Self {
            zoom: canvas.camera().zoom,
            ratio_min: f32::INFINITY,
            ratio_max: 0.0,
            ..Self::default()
        };
        for (key, _) in canvas.graph().nodes() {
            let Some(face) = canvas.node_face_rect(key) else {
                continue;
            };
            if face.0 < 0.0 || face.1 < 0.0 || face.2 > w || face.3 > h {
                continue;
            }
            let Some(body) = canvas.node_body_rect(key) else {
                out.missing += 1;
                continue;
            };
            let centre = |r: (f32, f32, f32, f32)| (0.5 * (r.0 + r.2), 0.5 * (r.1 + r.3));
            let (fx, fy) = centre(face);
            let (bx, by) = centre(body);
            out.nodes += 1;
            out.offset_max = out.offset_max.max((fx - bx).hypot(fy - by));
            for ratio in [
                (face.2 - face.0) / (body.2 - body.0),
                (face.3 - face.1) / (body.3 - body.1),
            ] {
                out.ratio_min = out.ratio_min.min(ratio);
                out.ratio_max = out.ratio_max.max(ratio);
            }
        }
        out
    }

    /// The receipt line for `measure-faces <label>`.
    pub fn line(&self, label: &str) -> String {
        format!(
            "faces {label}: zoom {:.3} nodes {} missing {} offset-max {:.4} px face/body {:.4}..{:.4}",
            self.zoom, self.nodes, self.missing, self.offset_max, self.ratio_min, self.ratio_max
        )
    }

    /// The snapshot fields a receipt asserts on. With nothing measured the
    /// bounds read `none`, so no numeric assert passes on an empty frame.
    pub fn fields(&self) -> [(&'static str, String); 5] {
        let value = |v: f32| {
            if self.nodes == 0 {
                "none".to_string()
            } else {
                format!("{v:.4}")
            }
        };
        [
            ("face-nodes", self.nodes.to_string()),
            ("face-missing", self.missing.to_string()),
            ("face-offset-max", value(self.offset_max)),
            ("face-ratio-min", value(self.ratio_min)),
            ("face-ratio-max", value(self.ratio_max)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas_controls::CanvasCommand;
    use mere::kernel::geometry::PortablePoint;

    #[test]
    fn every_face_in_view_sits_on_its_body() {
        let mut canvas = Canvas::with_sample_graph();
        canvas.resize(800, 600);
        canvas.set_physics_paused(true);
        // The twelve nodes on a 200 px grid about the viewport's centre, apart
        // enough that no caption covers a neighbour's body.
        let grid: Vec<_> = canvas
            .graph()
            .nodes()
            .enumerate()
            .map(|(i, (key, _))| {
                let (col, row) = ((i % 4) as f32, (i / 4) as f32);
                (
                    key,
                    PortablePoint::new(200.0 * col - 300.0, 200.0 * row - 200.0),
                )
            })
            .collect();
        canvas.land_paused_positions(&grid);
        canvas.set_camera(mere::canvas::CameraView {
            offset: (400.0, 300.0),
            zoom: 1.0,
        });
        for zoom in [1.0, 0.75, 0.5, 0.25, 0.1, 2.0] {
            CanvasCommand::SetZoom { zoom }.apply(&mut canvas, (800, 600));
            canvas.frame(800, 600);
            let faces = FaceAlignment::measure(&canvas, (800, 600));
            assert!(
                faces.nodes > 0 && faces.missing == 0,
                "{}",
                faces.line("test")
            );
            assert!(faces.offset_max <= 0.01, "{}", faces.line("test"));
            assert!(
                (faces.ratio_min - 0.72).abs() <= 0.001 && (faces.ratio_max - 0.72).abs() <= 0.001,
                "{}",
                faces.line("test")
            );
        }
        assert_eq!(FaceAlignment::default().fields()[2].1, "none");
    }
}
