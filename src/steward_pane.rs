//! Steward's first live projection: durable download custody records.

use std::cell::RefCell;
use std::rc::Rc;

use cambium::{AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, el};
use genet_scripted_dom::ScriptedDom;

#[derive(Clone, Debug, PartialEq, Eq)]
struct DownloadRow {
    received_at_ms: u64,
    text: String,
    pending: bool,
}

fn rows_from(app: &crate::app::App) -> Vec<DownloadRow> {
    let facet = chartulary::FacetId::new(crate::content_classes::DOWNLOAD_FACET);
    let mut rows = app
        .graph_runtimes
        .graph()
        .nodes()
        .filter_map(|(_, node)| {
            let record = app.graph_runtimes.facets().get(&node.id, &facet)?;
            let status = record.get("status")?.as_str().unwrap_or("unknown");
            let received_at_ms = record
                .get("received_at_ms")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let bytes = record
                .get("byte_size")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let detail = record
                .get("destination_path")
                .or_else(|| record.get("error"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_else(|| node.url());
            let title = if node.title.trim().is_empty() {
                node.url()
            } else {
                node.title.trim()
            };
            Some(DownloadRow {
                received_at_ms,
                text: format!("{title} - {status} - {bytes} bytes - {detail}"),
                pending: status == "storing",
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| std::cmp::Reverse(row.received_at_ms));
    rows
}

struct StewardState {
    rows: Vec<DownloadRow>,
    viewport_w: f32,
    viewport_h: f32,
}

type StewardView = Box<dyn AnyView<StewardState, (), GenetCtx, GenetElement>>;
type StewardRunner =
    GenetAppRunner<StewardState, fn(&StewardState) -> StewardView, StewardView, ()>;

fn steward_view(state: &StewardState) -> StewardView {
    let rows = state
        .rows
        .iter()
        .map(|row| {
            let class = if row.pending {
                "list-row muted"
            } else {
                "list-row"
            };
            Box::new(el::<_, StewardState, ()>("div", row.text.clone()).attr("class", class))
                as StewardView
        })
        .collect::<Vec<StewardView>>();
    let title = if rows.is_empty() {
        "No downloads yet"
    } else {
        "Downloads"
    };
    Box::new(
        el::<_, StewardState, ()>(
            "div",
            (
                el::<_, StewardState, ()>("div", title).attr("class", "list-section-title"),
                el::<_, StewardState, ()>("div", rows),
            ),
        )
        .attr("class", "pane")
        .attr(
            "style",
            format!(
                "width: {}px; height: {}px;",
                state.viewport_w, state.viewport_h
            ),
        ),
    )
}

pub(crate) struct StewardPane {
    dom: DomHandle,
    runner: StewardRunner,
    scroll: crate::ui::PaneScroll,
    layout: crate::ui::RetainedLayout,
}

impl StewardPane {
    pub(crate) fn new() -> Self {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let runner = StewardRunner::new(
            dom.clone(),
            steward_view as fn(&StewardState) -> StewardView,
            StewardState {
                rows: Vec::new(),
                viewport_w: 0.0,
                viewport_h: 0.0,
            },
        );
        Self {
            dom,
            runner,
            scroll: crate::ui::PaneScroll::new(),
            layout: crate::ui::RetainedLayout::new(),
        }
    }

    pub(crate) fn sync(&mut self, app: &crate::app::App, pane_w: f32, pane_h: f32) {
        let rows = rows_from(app);
        self.runner.update(|state| {
            state.rows = rows;
            state.viewport_w = pane_w;
            state.viewport_h = pane_h;
        });
    }

    pub(crate) fn scene(&mut self, w: u32, h: u32) -> netrender::Scene {
        self.layout.scene_scrolled(
            &mut self.dom.borrow_mut(),
            crate::ui::CAMBIUM_SHEET,
            w,
            h,
            &mut self.scroll,
        )
    }

    pub(crate) fn scroll_by(&mut self, dx: f32, dy: f32) {
        self.scroll.nudge(dx, dy);
    }

    pub(crate) fn bars_visible(&mut self) -> bool {
        self.scroll.bars_visible()
    }

    pub(crate) fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.dom.borrow()
    }

    #[cfg(test)]
    fn resolve(&self, selector: &genet_probe::Selector) -> Option<(f32, f32)> {
        let dom = self.dom.borrow();
        let surfaces = [genet_probe::ProbeSurface {
            name: "steward",
            dom: &dom,
            rect: [0.0, 0.0, 640.0, 480.0],
            sheet: crate::ui::CAMBIUM_SHEET,
        }];
        genet_probe::resolve(&surfaces, selector).map(|hit| hit.point)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_download_projects_from_durable_graph_facets() {
        let mut app = crate::app::App::test_stub();
        let key = app
            .graph_runtimes
            .visit("gemini://capsule.test/archive.bin");
        let node = app.graph_runtimes.graph().get_node(key).unwrap().id;
        crate::content_classes::set_download_record(
            &mut app.graph_runtimes,
            node,
            crate::content_classes::DownloadFacetRecord {
                source_url: "gemini://capsule.test/archive.bin",
                received_at_ms: 42,
                byte_size: 12,
                status: "completed",
                media_type: Some("application/octet-stream"),
                content_disposition: None,
                destination_path: Some("C:\\Downloads\\archive.bin"),
                content_hash: Some(&"22".repeat(32)),
                error: None,
            },
        )
        .unwrap();

        let mut pane = StewardPane::new();
        pane.sync(&app, 640.0, 480.0);
        assert!(
            pane.resolve(
                &genet_probe::Selector::class("list-row")
                    .containing("archive.bin - completed - 12 bytes")
            )
            .is_some()
        );
    }
}
