# Scenario fixtures

Packs the scenarios and tests install as participants.

- [browser_servo](browser_servo/README.md) contains the real upstream Servo
  two-view input/navigation and orientation fixture for
  [browser_servo_windows.scn](../browser_servo_windows.scn). Its host receipt
  additionally checks the configurable shared process profile, actual host-device
  textures and zero active views/cached frames. Source grammar alone does not
  qualify this engine.

- `trail_keeper.lua` — a rung-1 control script (the piccolo lane).
- `app_core_guest.wasm` — an `app-core` component (the envelope lane), built
  from mere's `crates/script/app-host/guest`. Rebuild it with:

  ```
  cd <mere>/crates/script/app-host/guest
  cargo build --target wasm32-wasip2 --release
  cp target/wasm32-wasip2/release/app_core_guest.wasm <turnstone>/scenarios/fixtures/
  ```

  It is committed so the wasm lane's receipts run on any checkout without a
  wasm toolchain. mere's own `app-host` tests build it from source, so a WIT
  change that breaks it fails there first.
