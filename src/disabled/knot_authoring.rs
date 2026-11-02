// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Capability boundary for builds without the authoring engine.
pub(crate) use crate::authoring_address::*;
use std::sync::{Arc, Mutex};
pub const ENGINE_ID: &str = "knot.authoring";
#[derive(Clone, Copy)]
pub enum KnotClipHandle {}
impl KnotClipHandle {
    pub fn same_admission(&self, _: &Self) -> bool {
        match *self {}
    }
    pub fn same_endpoint(&self, _: &Self) -> bool {
        match *self {}
    }
    pub fn keep_status_from(&mut self, _: &Self) {
        match *self {}
    }
    pub fn insert(&self, _: inker::DocumentClip) -> Result<(), String> {
        match *self {}
    }
    pub fn target(&self) -> &str {
        match *self {}
    }
    pub fn status(&self) -> KnotClipStatus {
        match *self {}
    }
}
pub enum KnotClipStatus {
    Ready,
    Sending,
    Saved,
    Stale,
    Rejected(String),
}
impl KnotClipStatus {
    pub fn label(&self) -> String {
        "Authoring is not included in this build".into()
    }
}
pub enum KnotDocumentSession {}
impl KnotDocumentSession {
    pub fn address(&self) -> &str {
        match *self {}
    }
    pub fn holder(&self) -> Option<String> {
        match *self {}
    }
    pub fn hold_status(&self) -> &str {
        match *self {}
    }
    pub fn derived_digest(&self) -> String {
        match *self {}
    }
    pub fn retained_selection(&self) -> Option<crate::producer_retention::KnotSelection> {
        match *self {}
    }
    pub fn dom_ref(&self) -> std::cell::Ref<'_, genet_scripted_dom::ScriptedDom> {
        match *self {}
    }
    pub fn selector_point(
        &self,
        _: &taproot::Selector,
    ) -> Result<Option<(f32, f32)>, &'static str> {
        match *self {}
    }
    pub fn selected_retained_export(
        &mut self,
        _: &crate::producer_retention::KnotSelection,
        _: usize,
    ) -> Result<(serde_json::Value, Vec<u8>), String> {
        match *self {}
    }
    pub fn research_clip_handle(&self) -> Result<KnotClipHandle, String> {
        match *self {}
    }
}
pub(crate) enum KnotHub {}
pub(crate) type KnotVisits = Arc<Mutex<KnotVisitRegistry>>;
#[derive(Default)]
pub(crate) struct KnotVisitRegistry {
    local_root: Option<[u8; 32]>,
}
impl KnotVisitRegistry {
    pub fn set_local_root(&mut self, root: Option<[u8; 32]>) {
        self.local_root = root;
    }
    pub fn is_local(&self, root: &[u8; 32]) -> bool {
        self.local_root.as_ref() == Some(root)
    }
    pub fn holder(&self, _: &[u8; 32]) -> Option<Arc<KnotHub>> {
        None
    }
    pub fn insert_holder(&mut self, _: [u8; 32], hub: Arc<KnotHub>) {
        match *hub {}
    }
    pub fn clear(&mut self) {
        self.local_root = None;
    }
}
pub(crate) fn visit_holder(
    dial: crate::place::lanes::HolderDial,
    _: armillary::Wake,
) -> Result<Arc<KnotHub>, String> {
    match dial {}
}

pub(crate) const KNOT_SHEET: &str = "";
