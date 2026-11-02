// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[derive(Clone)]
pub(super) struct DocumentBinding {
    pub(super) target: InstanceId,
    pub(super) save_action: AdvertisedAction,
    pub(super) clip_action: AdvertisedAction,
    pub(super) resolve_action: Option<AdvertisedAction>,
    pub(super) run_action: Option<AdvertisedAction>,
    pub(super) editable: EditableTextV1,
}
