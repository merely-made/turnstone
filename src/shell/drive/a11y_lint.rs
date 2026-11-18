// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use registry::diagnostics::ux_lint::RegistrySink;
use uxtree::lint::Report;

pub(super) fn step(line: &str, inspect: impl FnOnce() -> Report) -> Option<Result<(), String>> {
    if line != "lint-a11y" {
        return None;
    }
    let report = inspect();
    report.send_to(&mut RegistrySink);
    let total = report.findings.len();
    if total == 0 {
        return Some(Ok(()));
    }
    let preview = report
        .findings
        .iter()
        .take(6)
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    Some(Err(format!(
        "{total} accessibility lint findings. {preview}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{Node, NodeId, Rect, Role, TreeId, TreeInfo, TreeUpdate};

    struct Fixture(TreeUpdate);

    impl taproot::Automatable for Fixture {
        fn with_surfaces<R>(&self, f: impl FnOnce(&[taproot::ProbeSurface<'_>]) -> R) -> R {
            f(&[])
        }
        fn snapshot(&self) -> taproot::ProbeSnapshot {
            taproot::ProbeSnapshot::default()
        }
        fn drain_events(&mut self) -> Vec<String> {
            Vec::new()
        }
        fn act(&mut self, _: &str) -> bool {
            false
        }
        fn press(&mut self, _: f32, _: f32) {}
        fn moved(&mut self, _: f32, _: f32) {}
        fn release(&mut self, _: f32, _: f32) {}
    }

    impl taproot::Driveable for Fixture {
        fn app_step(&mut self, line: &str) -> Result<(), String> {
            step(line, || {
                uxtree::lint::inspect(
                    TreeId::ROOT,
                    std::slice::from_ref(&self.0),
                    |_| true,
                    |_, _| false,
                )
            })
            .ok_or_else(|| format!("unexpected scenario step {line}"))?
        }
    }

    #[test]
    fn lint_a11y_scenario_passes_a_clean_tree_and_fails_a_planted_finding() {
        let (sender, _receiver) = registry::diagnostics::diagnostic_channel(Default::default());
        let _sink = registry::diagnostics::scoped_thread_sender(sender);
        let id = NodeId(1);
        let mut root = Node::new(Role::Window);
        root.set_bounds(Rect::new(0.0, 0.0, 10.0, 10.0));
        let tree = TreeUpdate {
            tree_id: TreeId::ROOT,
            tree: Some(TreeInfo::new(id)),
            nodes: vec![(id, root)],
            focus: id,
        };
        let run = |tree| {
            let mut scenario = taproot::Scenario::parse("lint-a11y").unwrap();
            let mut fixture = Fixture(tree);
            while scenario.tick(&mut fixture) != taproot::Progress::Done {}
            scenario.finish()
        };
        assert!(run(tree.clone()).ok);
        let mut broken = tree;
        broken.nodes[0].1.clear_bounds();
        let outcome = run(broken);
        assert!(!outcome.ok);
        assert!(
            outcome
                .log
                .iter()
                .any(|line| line.contains("MissingBounds"))
        );
    }
}
