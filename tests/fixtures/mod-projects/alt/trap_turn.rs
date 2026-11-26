//! A document mod whose `handle_event` traps: activation is fine, so it attaches,
//! and the first turn it is delivered traps. alt/trap_turn.wasm is built from the
//! starter project with this file as `src/lib.rs`.

wit_bindgen::generate!({
    path: "wit",
    world: "document-core",
});

struct Component;

impl Guest for Component {
    fn activate() {}

    fn deactivate() {}

    fn handle_event(_ev: Event) -> Result<ApplyBatch, TurnError> {
        panic!("trap_turn: handle_event refuses");
    }
}

export!(Component);
