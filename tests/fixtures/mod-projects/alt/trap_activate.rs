//! A document mod whose `activate` traps: the starter's `src/lib.rs` with this
//! `activate`. alt/trap.wasm is built from the starter project with this file as
//! `src/lib.rs`; a reload to it fails at activation.

wit_bindgen::generate!({
    path: "wit",
    world: "document-core",
});

struct Component;

impl Guest for Component {
    fn activate() {
        panic!("trap_activate: activate refuses");
    }

    fn deactivate() {}

    fn handle_event(_ev: Event) -> Result<ApplyBatch, TurnError> {
        Ok(ApplyBatch { expected_revision: 0, changes: Vec::new() })
    }
}

export!(Component);
