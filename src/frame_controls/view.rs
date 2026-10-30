// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn view(s: &State) -> View {
    let mounts = s.mounts.clone();
    Box::new(cambium::workspace_view(
        &WorkspaceModel {
            workspace: &s.workspace,
            current: s.current,
            float_layer_visible: true,
        },
        |s: &mut State, event| s.events.push(event),
        move |tile| match mounts.get(&tile.id) {
            Some(mount) => Slot::View(Box::new(
                cambium::component(
                    mount.clone(),
                    |_| (),
                    |_, _, _| {},
                    |mount, _| Box::new(mount.view::<(), ()>()),
                    |_: &mut State, _: ()| {},
                )
                .probe_id(format!("pane:{}", tile.id.0)),
            )),
            None => Slot::Surface,
        },
    ))
}
