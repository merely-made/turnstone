// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
pub(super) fn comparison_identity(target: &crate::research::Target) -> String {
    let (kind, graph) = match target {
        crate::research::Target::Capture(t) => ("capture", t.key.graph),
        crate::research::Target::Clip(t) => ("clip", t.clip.graph),
    };
    format!(
        "{kind} · session {} · graph {} · node {} · manifest {} · text hash {}",
        target.session().as_uuid(),
        graph.as_uuid(),
        target.node(),
        target.identity(),
        target.text_hash()
    )
}

/// Consume each side at most once, including skipped text before this page.
pub(super) fn comparison_previews<'a>(
    mut chars: impl Iterator<Item = char>,
    spans: impl Iterator<Item = &'a eidetic_search::compare::ComparisonSpan>,
    left: bool,
    max_chars: usize,
) -> Vec<Option<String>> {
    let mut position = 0usize;
    let mut exhausted = false;
    spans
        .map(|span| {
            let selector = if left {
                span.old.as_ref()
            } else {
                span.new.as_ref()
            }?;
            if selector.start < position || selector.end < selector.start || exhausted {
                return None;
            }
            while position < selector.start {
                if chars.next().is_none() {
                    exhausted = true;
                    return None;
                }
                position += 1;
            }
            let mut preview = String::new();
            let count = selector.end.saturating_sub(selector.start).min(max_chars);
            for _ in 0..count {
                let Some(ch) = chars.next() else {
                    exhausted = true;
                    break;
                };
                preview.push(ch);
                position += 1;
            }
            Some(preview)
        })
        .collect()
}

#[cfg(test)]
mod comparison_preview_tests {
    use super::comparison_previews;
    use eidetic::models::recall::PassageSelector;
    use eidetic_search::compare::{ComparisonSpan, SpanKind};
    use std::cell::Cell;

    #[test]
    fn model_free_comparison_preview_traverses_long_prefix_once() {
        let mut text = "x".repeat(3_000_000);
        text.push('\n');
        for _ in 0..50 {
            text.push_str("🐦 line\n");
        }
        let mut spans = vec![ComparisonSpan {
            kind: SpanKind::Delete,
            old: Some(PassageSelector {
                start: 0,
                end: 3_000_001,
            }),
            new: None,
        }];
        for i in 0..50 {
            spans.push(ComparisonSpan {
                kind: SpanKind::Delete,
                old: Some(PassageSelector {
                    start: 3_000_001 + i * 7,
                    end: 3_000_001 + (i + 1) * 7,
                }),
                new: None,
            });
        }
        let traversed = Cell::new(0);
        let chars = text.chars().inspect(|_| traversed.set(traversed.get() + 1));
        let previews = comparison_previews(chars, spans.iter(), true, 4);
        assert_eq!(previews.len(), 51);
        assert_eq!(previews[50].as_deref(), Some("🐦 li"));
        assert!(traversed.get() <= text.chars().count());
    }
}
