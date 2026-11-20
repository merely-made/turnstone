// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use layout_dom_api::{LayoutDom, LocalName, Namespace};

use super::Shell;

type Fields = BTreeMap<String, String>;

pub(in crate::shell) struct PendingCoopWait {
    expected: Fields,
    deadline: Instant,
}

impl PendingCoopWait {
    pub(super) fn new(json: &str) -> Result<Self, String> {
        let expected: Fields = serde_json::from_str(json).map_err(|error| {
            format!("wait-coop expects a JSON object of string fields: {error}")
        })?;
        if expected.is_empty() {
            return Err("wait-coop requires at least one field".into());
        }
        Ok(Self {
            expected,
            deadline: Instant::now() + Duration::from_secs(30),
        })
    }

    pub(in crate::shell) fn poll(&self, observation: Option<&str>) -> Result<bool, String> {
        let current = observation
            .map(serde_json::from_str::<Fields>)
            .transpose()
            .map_err(|error| format!("invalid co-op observation: {error}"))?;
        let matches = current.as_ref().is_some_and(|fields| {
            self.expected.iter().all(|(key, expected)| {
                fields.get(key).is_some_and(|value| {
                    expected
                        .strip_prefix('~')
                        .map_or(value == expected, |part| value.contains(part))
                })
            })
        });
        if matches || Instant::now() < self.deadline {
            return Ok(matches);
        }
        let observed: Vec<_> = self
            .expected
            .keys()
            .map(|key| (key, current.as_ref().and_then(|fields| fields.get(key))))
            .collect();
        Err(format!(
            "wait-coop timed out. Expected {:?}; observed {observed:?}",
            self.expected
        ))
    }
}

impl Shell {
    pub(in crate::shell) fn take_capture_with_coop_reading(
        &mut self,
    ) -> Option<std::path::PathBuf> {
        let path = self.pending_capture.take()?;
        if let Some(observation) = self.coop_observation()
            && let Err(error) = std::fs::write(path.with_extension("coop.json"), observation)
        {
            self.main_receipt
                .errors
                .push(format!("write co-op capture reading: {error}"));
            return None;
        }
        Some(path)
    }

    pub(in crate::shell) fn coop_observation(&self) -> Option<String> {
        let pane = self.renderers.contributed.get(self.app.active_pane?)?;
        let dom = pane.dom_ref();
        let attr = LocalName::from("data-coop-observation");
        let mut nodes = vec![pane.session().root()];
        while let Some(node) = nodes.pop() {
            if let Some(value) = dom.attribute(node, &Namespace::default(), &attr)
                && !value.is_empty()
            {
                return Some(value.to_owned());
            }
            nodes.extend(dom.dom_children(node));
        }
        None
    }
}
