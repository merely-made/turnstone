// Bounded copies of address and reader-open events for scenario evidence.
// These observations never dispatch or change navigation.
use std::collections::VecDeque;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum OpenEvent {
    Address,
    Reader,
}

pub(super) fn event_address(event: &crate::observe::AppEvent) -> Option<(OpenEvent, &str)> {
    match event {
        crate::observe::AppEvent::AddressOpened(address) => Some((OpenEvent::Address, address)),
        crate::observe::AppEvent::WorkbenchTileOpened(address) => {
            Some((OpenEvent::Reader, address))
        },
        _ => None,
    }
}

#[derive(Default)]
pub(super) struct AddressEventHistory {
    total: u64,
    dropped: u64,
    overflow: bool,
    // None is a counted, redacted open. No arbitrary URI is retained.
    events: VecDeque<(u64, Option<crate::saved_sources::Target>)>,
}

impl AddressEventHistory {
    pub(super) fn record(&mut self, target: Option<crate::saved_sources::Target>) {
        let Some(next) = self.total.checked_add(1) else {
            self.overflow = true;
            return;
        };
        self.total = next;
        if self.events.len() == 128 {
            self.events.pop_front();
            match self.dropped.checked_add(1) {
                Some(value) => self.dropped = value,
                None => self.overflow = true,
            }
        }
        self.events.push_back((next, target));
    }

    pub(super) fn observation(
        &self,
        selected: &Option<Vec<crate::research_search::Member>>,
        admitted: impl Fn(&crate::saved_sources::Key) -> bool,
    ) -> serde_json::Value {
        self.observation_with_schema(selected, admitted, "turnstone.m5-address-events/v1")
    }

    pub(super) fn reader_observation(
        &self,
        selected: &Option<Vec<crate::research_search::Member>>,
        admitted: impl Fn(&crate::saved_sources::Key) -> bool,
    ) -> serde_json::Value {
        self.observation_with_schema(selected, admitted, "turnstone.m5-reader-open-events/v1")
    }

    fn observation_with_schema(
        &self,
        selected: &Option<Vec<crate::research_search::Member>>,
        admitted: impl Fn(&crate::saved_sources::Key) -> bool,
        schema: &str,
    ) -> serde_json::Value {
        let events: Vec<_> = self
            .events
            .iter()
            .map(|(sequence, target)| {
                let target = target.as_ref().filter(|value| {
                    admitted(&value.key)
                        && crate::research_scenario_privacy::capture_selected(selected, &value.key)
                });
                (*sequence, target)
            })
            .collect();
        serde_json::json!({"schema":schema,
            "total":self.total, "dropped":self.dropped, "overflow":self.overflow,
            "events":events})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_free_workbench_open_history_is_distinct_and_scope_bounded() {
        use crate::observe::AppEvent;
        let uri = "mere://saved-source/example".to_string();
        assert_eq!(
            event_address(&AppEvent::AddressOpened(uri.clone())),
            Some((OpenEvent::Address, uri.as_str()))
        );
        assert_eq!(
            event_address(&AppEvent::WorkbenchTileOpened(uri.clone())),
            Some((OpenEvent::Reader, uri.as_str()))
        );
        assert_eq!(event_address(&AppEvent::PaneClosed), None);
        let key = crate::saved_sources::Key {
            session: crate::panes::SessionId::new(),
            graph: crate::panes::GraphId::new(),
            node: uuid::Uuid::new_v4(),
            annotation_manifest: "sealed-v1:canary".into(),
        };
        let target = crate::saved_sources::Target {
            key: key.clone(),
            text_hash: "blake3:canary".into(),
            selector: None,
        };
        let mut h = AddressEventHistory::default();
        h.record(Some(target));
        let all = h.reader_observation(&None, |_| true);
        assert_eq!(all["schema"], "turnstone.m5-reader-open-events/v1");
        assert_eq!(all["total"], 1);
        assert!(all["events"][0][1].is_object());
        assert!(h.reader_observation(&Some(vec![]), |_| true)["events"][0][1].is_null());
        assert!(h.reader_observation(&None, |_| false)["events"][0][1].is_null());
        for _ in 0..128 {
            h.record(None);
        }
        assert_eq!(h.reader_observation(&None, |_| true)["dropped"], 1);
    }

    #[test]
    fn model_free_address_event_history_reports_bounded_loss() {
        let mut h = AddressEventHistory::default();
        let before = h.observation(&None, |_| true);
        h.record(None);
        let after = h.observation(&None, |_| true);
        assert_eq!(before["total"], 0);
        assert_eq!(after["total"], 1);
        assert!(after["events"][0][1].is_null());
        for _ in 0..128 {
            h.record(None);
        }
        assert_eq!(h.observation(&None, |_| true)["dropped"], 1);
        assert_eq!(
            h.observation(&None, |_| true)["events"]
                .as_array()
                .unwrap()
                .len(),
            128
        );
    }

    #[test]
    fn model_free_address_event_history_rechecks_current_scope_and_admission() {
        let key = crate::saved_sources::Key {
            session: crate::panes::SessionId::new(),
            graph: crate::panes::GraphId::new(),
            node: uuid::Uuid::new_v4(),
            annotation_manifest: "sealed-v1:blake3:canary".into(),
        };
        let target = crate::saved_sources::Target {
            key: key.clone(),
            text_hash: "blake3:canary".into(),
            selector: None,
        };
        let mut h = AddressEventHistory::default();
        h.record(Some(target));
        assert!(h.observation(&None, |_| false)["events"][0][1].is_null());
        assert!(h.observation(&Some(vec![]), |_| true)["events"][0][1].is_null());
        let selected = Some(vec![crate::research_search::Member::Capture(key)]);
        assert!(h.observation(&selected, |_| true)["events"][0][1].is_object());
    }
}
