// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
struct NoIo;
impl RetainedSourceOwner for NoIo {
    fn snapshot(
        &self,
        _: &str,
    ) -> Result<Vec<eidetic::models::recall::RetainedSourceText>, String> {
        unreachable!()
    }
    fn resolve(
        &self,
        _: &eidetic::models::recall::RecallSourceId,
        _: &str,
    ) -> Result<String, String> {
        unreachable!()
    }
}
#[test]
fn research_panes_share_one_publisher_and_reject_foreign_or_legacy_sources() {
    let bridge = crate::research::Bridge::default();
    let context = crate::research::Context {
        session: crate::panes::SessionId::new(),
        generation: uuid::Uuid::new_v4(),
        admissions: Vec::new(),
        clips: Vec::new(),
    };
    bridge.set_available(true);
    bridge.project(context.clone());
    let provider = AnswerProvider::new(Arc::new(NoIo), Arc::new(|| {}), bridge);
    let namespace = crate::research::namespace(context.session);
    let service = InferenceService::inert_for_surface_test(namespace.clone());
    provider.connections.0.borrow_mut().insert(
        format!("{namespace}/{}", context.generation),
        Arc::downgrade(&service),
    );
    let dom = || Rc::new(RefCell::new(genet_scripted_dom::ScriptedDom::new()));
    let first = provider
        .admit(&answer_source(*context.session.as_uuid()), dom())
        .unwrap();
    let second = provider
        .admit(&answer_source(*context.session.as_uuid()), dom())
        .unwrap();
    assert_eq!(
        Arc::strong_count(&service),
        3,
        "two admitted runners borrow exactly one publisher"
    );
    assert!(
        provider
            .admit(&answer_source(uuid::Uuid::new_v4()), dom())
            .is_err()
    );
    let legacy = PaneSource::Fixed(SourceRef::External {
        schema: SourceSchemaId::new(SOURCE_SCHEMA),
        payload: SerializedSource {
            version: 1,
            payload: serde_json::json!({ "session": context.session.as_uuid() }),
        },
    });
    assert!(
        provider.admit(&legacy, dom()).is_err(),
        "legacy PageText source cannot become a capture ask implicitly"
    );
    drop(first);
    drop(second);
    assert_eq!(Arc::strong_count(&service), 1);
    provider.bridge.set_available(false);
    assert!(
        provider
            .admit(&answer_source(*context.session.as_uuid()), dom())
            .is_err(),
        "locked custody must not admit a new answer connection"
    );
}
