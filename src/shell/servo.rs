// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's host glue for upstream Servo views on its existing wgpu device.
//!
//! The engine itself (process root, `WebView` and delegate, input, cursor,
//! accessibility and navigation translation, frame import) lives in
//! wgpu-graft's `grafting-servo` and inker's `graft-engine`. This module owns
//! only what is Turnstone's: the `TURNSTONE_SERVO_*` process settings, the
//! profile naming handed in by the shell, and the Cue presentation below.
//! Hosts reach Servo only through `graft_engine::servo`, never by revision.

#[cfg(target_os = "macos")]
#[path = "servo_cue.rs"]
pub mod cue;
#[cfg(target_os = "linux")]
#[path = "servo_cue_linux.rs"]
pub mod cue;
#[path = "servo_cue_capabilities.rs"]
mod cue_capabilities;

use graft_engine::servo::{A11yFocusPolicy, ServoGraftFactory, ServoProducerConfig};
use inker::SurfaceError;
use std::{path::PathBuf, sync::Arc};

pub(crate) use graft_engine::servo::{FRAME_KIND, active_views, pump, shutdown};
#[cfg(target_os = "linux")]
pub(crate) use graft_engine::servo::GraftDmaBufFrame as ServoDmaBufFrame;
#[cfg(not(target_os = "linux"))]
pub(crate) use graft_engine::servo::GraftImportedFrame as ServoImportedFrame;

pub(crate) type TurnstoneServoFactory = ServoGraftFactory;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ServoHostOptions {
    pub profile_name: String,
    pub profile_dir: PathBuf,
}

impl ServoHostOptions {
    /// Build the process factory: the named profile plus the diagnostics the
    /// environment selects. Construct exactly once on the UI thread.
    pub(crate) fn into_factory(
        self,
        device: wgpu::Device,
        queue: wgpu::Queue,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<TurnstoneServoFactory, SurfaceError> {
        // Refuse unknown settings before touching upstream process globals.
        let mut config = ServoProducerConfig::new(self.profile_name, self.profile_dir);
        config.a11y_focus = requested_a11y_focus_policy()?;
        config.gpu_sync = requested_gpu_sync()?;
        ServoGraftFactory::new(config, device, queue, wake)
    }
}

fn spawn_error(detail: impl Into<String>) -> SurfaceError {
    SurfaceError::SpawnFailed(detail.into())
}

fn env_option(name: &str) -> Result<Option<String>, SurfaceError> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(_) => Err(spawn_error(format!("{name} must be Unicode"))),
    }
}

fn requested_a11y_focus_policy() -> Result<A11yFocusPolicy, SurfaceError> {
    let value = env_option("TURNSTONE_SERVO_A11Y_FOCUS")?;
    A11yFocusPolicy::parse(value.as_deref()).map_err(|_| {
        spawn_error("TURNSTONE_SERVO_A11Y_FOCUS must be upstream or document-root")
    })
}

fn requested_gpu_sync() -> Result<graft_engine::servo::grafting::DiagnosticGpuSync, SurfaceError> {
    let mode = env_option("TURNSTONE_SERVO_GPU_SYNC")?;
    let timeout = env_option("TURNSTONE_SERVO_GPU_SYNC_TIMEOUT_MS")?
        .map(|value| value.parse::<u64>())
        .transpose()
        .map_err(|_| spawn_error("Servo GPU diagnostic timeout must be positive milliseconds"))?
        .unwrap_or(5000);
    if timeout == 0 {
        return Err(spawn_error(
            "Servo GPU diagnostic timeout must be positive milliseconds",
        ));
    }
    graft_engine::servo::parse_gpu_sync(
        mode.as_deref().unwrap_or("existing"),
        std::time::Duration::from_millis(timeout),
    )
    .map_err(|_| {
        spawn_error(
            "TURNSTONE_SERVO_GPU_SYNC must be existing, producer, normalization or both",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{Action, Node, NodeId, Rect, Role, TreeId, TreeInfo, Uuid};
    use graft_engine::servo::servo::accesskit as servo_ak;

    /// Servo's AccessKit and the surface contract's differ in version; the
    /// serialized form must cross unchanged.
    #[test]
    fn servo_updates_convert_to_the_surface_contract_unchanged() {
        let mut root = servo_ak::Node::new(servo_ak::Role::RootWebArea);
        root.set_children(vec![servo_ak::NodeId(2)]);
        root.set_tree_id(servo_ak::TreeId(servo_ak::Uuid::from_u64_pair(0, 110)));
        let mut child = servo_ak::Node::new(servo_ak::Role::Button);
        child.set_label("supplier node");
        child.add_action(servo_ak::Action::Click);
        child.set_bounds(servo_ak::Rect::new(1.0, 2.0, 30.0, 40.0));
        let servo_update = servo_ak::TreeUpdate {
            nodes: vec![(servo_ak::NodeId(1), root), (servo_ak::NodeId(2), child)],
            tree: Some(servo_ak::Tree::new(servo_ak::NodeId(1))),
            tree_id: servo_ak::TreeId(servo_ak::Uuid::from_u64_pair(0, 109)),
            focus: servo_ak::NodeId(2),
        };

        let mut root = Node::new(Role::RootWebArea);
        root.set_children(vec![NodeId(2)]);
        root.set_tree_id(TreeId(Uuid::from_u64_pair(0, 110)));
        let mut child = Node::new(Role::Button);
        child.set_label("supplier node");
        child.add_action(Action::Click);
        child.set_bounds(Rect::new(1.0, 2.0, 30.0, 40.0));
        let expected = inker::SurfaceAccessibilityUpdate {
            nodes: vec![(NodeId(1), root), (NodeId(2), child)],
            tree: Some(TreeInfo::new(NodeId(1))),
            tree_id: TreeId(Uuid::from_u64_pair(0, 109)),
            focus: NodeId(2),
        };
        let value = serde_json::to_value(&servo_update).unwrap();
        assert_eq!(
            graft_engine::servo::surface_update_from_value(value).unwrap(),
            expected
        );
    }
}
