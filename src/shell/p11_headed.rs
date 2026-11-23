// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A public fixture supplier through Turnstone's live window and Windows UIA.

use super::*;
use accesskit::{
    Action as A11yAction, ActionRequest, Node, NodeId, Rect, Role, TreeId, TreeInfo, TreeUpdate,
};
use inker::{SurfaceError, SurfaceFrame, SurfaceProducer};
use pelt_core::{PeltContent, PeltRoute, PeltRouteSource, PeltRouteState};
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    io::Write,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use winit::{
    event_loop::EventLoop,
    event_loop::pump_events::{EventLoopExtPumpEvents, PumpStatus},
    platform::windows::EventLoopBuilderExtWindows,
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
};

const EXPECTED_LABEL: &str = "P11 Fixture Nested Target";
const SIBLING_LABEL: &str = "P11 Fixture Nested Sibling";
const BUTTON: NodeId = NodeId(91);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn fixture_root() -> TreeId {
    TreeId(accesskit::Uuid::from_u64_pair(0xA11F, 1))
}

fn target_tree() -> TreeId {
    TreeId(accesskit::Uuid::from_u64_pair(0xA11F, 2))
}

fn sibling_tree() -> TreeId {
    TreeId(accesskit::Uuid::from_u64_pair(0xA11F, 3))
}

fn fixture_forest() -> VecDeque<TreeUpdate> {
    let root_id = NodeId(1);
    let mut root = Node::new(Role::Document);
    root.set_label("P11 Fixture Supplier");
    root.set_bounds(Rect::new(0.0, 0.0, 400.0, 200.0));
    root.set_children(vec![NodeId(2), NodeId(3)]);
    let root_update = TreeUpdate {
        tree_id: fixture_root(),
        tree: Some(TreeInfo::new(root_id)),
        focus: root_id,
        nodes: vec![
            (root_id, root),
            (
                NodeId(2),
                uxtree::graft::graft_node(Role::Group, target_tree(), (0.0, 0.0), (160.0, 80.0)),
            ),
            (
                NodeId(3),
                uxtree::graft::graft_node(Role::Group, sibling_tree(), (170.0, 0.0), (160.0, 80.0)),
            ),
        ],
    };
    fn child(tree_id: TreeId, label: &str) -> TreeUpdate {
        let root_id = NodeId(1);
        let mut panel = Node::new(Role::Group);
        panel.set_bounds(Rect::new(0.0, 0.0, 150.0, 60.0));
        panel.set_children(vec![BUTTON]);
        let mut button = Node::new(Role::Button);
        button.set_label(label);
        button.set_bounds(Rect::new(4.0, 4.0, 125.0, 40.0));
        button.add_action(A11yAction::Click);
        TreeUpdate {
            tree_id,
            tree: Some(TreeInfo::new(root_id)),
            focus: root_id,
            nodes: vec![(root_id, panel), (BUTTON, button)],
        }
    }
    [
        root_update,
        child(target_tree(), EXPECTED_LABEL),
        child(sibling_tree(), SIBLING_LABEL),
    ]
    .into()
}

#[derive(Default)]
struct SupplierTrace {
    actions: Mutex<Vec<ActionRequest>>,
}

struct FixtureSupplier {
    pending: VecDeque<TreeUpdate>,
    trace: Arc<SupplierTrace>,
}

impl SurfaceProducer for FixtureSupplier {
    fn resize(&mut self, _: u32, _: u32) -> Result<(), SurfaceError> {
        Ok(())
    }
    fn set_offset(&mut self, _: i32, _: i32) -> Result<(), SurfaceError> {
        Ok(())
    }
    fn acquire_frame(&mut self) -> Result<Option<SurfaceFrame>, SurfaceError> {
        Ok(None)
    }
    fn send_mouse_input(&mut self, _: inker::MouseEvent) -> Result<(), SurfaceError> {
        Ok(())
    }
    fn send_pointer_input(&mut self, _: inker::PointerEvent) -> Result<(), SurfaceError> {
        Ok(())
    }
    fn send_keyboard_input(&mut self, _: inker::KeyboardEvent) -> Result<(), SurfaceError> {
        Ok(())
    }
    fn move_focus(&mut self, _: inker::FocusReason) -> Result<(), SurfaceError> {
        Ok(())
    }
    fn poll_cursor_shape(&mut self) -> Option<inker::CursorShape> {
        None
    }
    fn apply_settings(&mut self, _: &inker::SurfaceSettings) -> Result<(), SurfaceError> {
        Ok(())
    }

    fn set_accessibility_active(&mut self, active: bool) -> Result<Option<TreeId>, SurfaceError> {
        self.pending = if active {
            fixture_forest()
        } else {
            VecDeque::new()
        };
        Ok(active.then_some(fixture_root()))
    }

    fn poll_accessibility_update(&mut self) -> Option<TreeUpdate> {
        self.pending.pop_front()
    }

    fn send_accessibility_action(&mut self, request: ActionRequest) -> Result<(), SurfaceError> {
        self.trace.actions.lock().unwrap().push(request);
        Ok(())
    }
}

fn mount_fixture(shell: &mut Shell, node: uuid::Uuid) -> Arc<SupplierTrace> {
    let trace = Arc::new(SupplierTrace::default());
    shell.content_sessions.insert(
        node,
        PeltContent::from_surface(
            Box::new(FixtureSupplier {
                pending: VecDeque::new(),
                trace: trace.clone(),
            }),
            (400, 200),
            PeltRoute {
                decision: inker::routing::EngineRouteDecision {
                    engine_id: "p11.headed.fixture".into(),
                    surface_contract: inker::routing::SurfaceContract {
                        target: inker::routing::SurfaceTargetId::new("p11-headed-fixture"),
                        mode: inker::routing::SurfaceContractMode::CompositedTexture,
                    },
                },
                source: PeltRouteSource::Automatic,
                state: PeltRouteState::Surface,
            },
        ),
    );
    shell.activate_surface_accessibility(node, false);
    shell.push_a11y_tree();
    trace
}

struct Observer(Child);

impl Drop for Observer {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn start_observer(output: &Path, window: &dyn winit::window::Window, broken: bool) -> Observer {
    let raw = window.window_handle().expect("window handle").as_raw();
    let RawWindowHandle::Win32(raw) = raw else {
        panic!("expected HWND")
    };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/p11-nested-uia.ps1");
    let stdout = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("uia-stdout.log"))
        .unwrap();
    let stderr = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("uia-stderr.log"))
        .unwrap();
    let mut command = Command::new("powershell.exe");
    command
        .args(["-NoProfile", "-NonInteractive", "-File"])
        .arg(script)
        .arg("-OwnerPid")
        .arg(std::process::id().to_string())
        .arg("-Hwnd")
        .arg(raw.hwnd.get().to_string())
        .arg("-OutputDirectory")
        .arg(output)
        .arg("-TargetLabel")
        .arg(if broken {
            SIBLING_LABEL
        } else {
            EXPECTED_LABEL
        })
        .arg("-SiblingLabel")
        .arg(if broken {
            EXPECTED_LABEL
        } else {
            SIBLING_LABEL
        })
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .creation_flags(CREATE_NO_WINDOW);
    Observer(command.spawn().expect("start owned Windows UIA observer"))
}

fn pump_until(
    event_loop: &mut EventLoop,
    shell: &mut Shell,
    mut ready: impl FnMut(&Shell) -> bool,
    stage: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(40);
    while !ready(shell) {
        assert!(Instant::now() < deadline, "timed out: {stage}");
        assert!(
            shell.main_receipt.errors.is_empty(),
            "{:?}",
            shell.main_receipt.errors
        );
        shell.request_redraw();
        assert!(matches!(
            event_loop.pump_app_events(Some(Duration::from_millis(25)), &mut *shell),
            PumpStatus::Continue
        ));
    }
}

fn write_receipt(path: &Path, value: &serde_json::Value) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    serde_json::to_writer_pretty(&mut file, value).unwrap();
    writeln!(file).unwrap();
}

#[test]
#[ignore = "opens an owned Windows UIA window; needs isolated TURNSTONE_ROOT and TURNSTONE_CAPTURE_DIR"]
fn nested_foreign_supplier_accepts_native_uia_invoke() {
    let profile = PathBuf::from(std::env::var_os("TURNSTONE_ROOT").expect("isolated profile"));
    let output =
        PathBuf::from(std::env::var_os("TURNSTONE_CAPTURE_DIR").expect("capture directory"));
    assert!(
        profile.is_absolute() && !profile.exists(),
        "use a fresh profile"
    );
    assert!(
        output.is_absolute() && output.is_dir(),
        "create a fresh capture directory"
    );
    assert!(
        !output.join("supplier-action.json").exists(),
        "receipt already exists"
    );
    assert_eq!(std::env::var("TURNSTONE_OFFLINE").as_deref(), Ok("1"));
    let broken = std::env::var("TURNSTONE_P11_BROKEN_TARGET").as_deref() == Ok("1");

    let mut event_loop = EventLoop::builder().with_any_thread(true).build().unwrap();
    let mut shell = Shell::new(event_loop.create_proxy(), None);
    if let Some((setup, _)) = shell.custody_setup.take() {
        setup.request_stop();
    }
    pump_until(
        &mut event_loop,
        &mut shell,
        |s| s.main_receipt.presented > 0,
        "first native frame",
    );
    taproot::Driveable::app_step(&mut shell, "key escape").unwrap();
    shell
        .app
        .update(Action::OpenAddress("mere://p11-nested-fixture".into()));
    let node = shell
        .app
        .graph_runtimes
        .focused_member()
        .expect("fixture graph member");
    shell.app.update(Action::OpenInWorkbench);
    let trace = mount_fixture(&mut shell, node);
    assert!(
        shell.surface_a11y.ready(node),
        "nested fixture forest was not admitted"
    );
    assert!(
        shell
            .foreign_a11y_placements()
            .iter()
            .any(|(owner, _)| *owner == node),
        "fixture surface is absent from the native window plan"
    );

    let mut observer = start_observer(
        &output,
        shell.window.as_deref().expect("native window"),
        broken,
    );
    pump_until(
        &mut event_loop,
        &mut shell,
        |_| output.join("uia-invoked.json").exists() || observer.0.try_wait().unwrap().is_some(),
        "native UIA Invoke",
    );
    assert!(
        output.join("uia-invoked.json").is_file(),
        "native UIA observer failed before invoking; see uia-stderr.log"
    );
    pump_until(
        &mut event_loop,
        &mut shell,
        |_| !trace.actions.lock().unwrap().is_empty(),
        "supplier receives Invoke",
    );
    let recorded = trace.actions.lock().unwrap();
    let original = ActionRequest {
        action: A11yAction::Click,
        target_tree: target_tree(),
        target_node: BUTTON,
        data: None,
    };
    write_receipt(
        &output.join("supplier-action.json"),
        &serde_json::json!({
            "source_tree": fixture_root().0.to_string(),
            "expected_target_tree": target_tree().0.to_string(),
            "sibling_tree": sibling_tree().0.to_string(),
            "shared_local_node": BUTTON.0,
            "grafted_tree_count": 3,
            "received": recorded.iter().map(|request| serde_json::json!({
                "target_tree": request.target_tree.0.to_string(),
                "target_node": request.target_node.0,
                "action": format!("{:?}", request.action),
                "data": format!("{:?}", request.data),
            })).collect::<Vec<_>>(),
        }),
    );
    assert_eq!(
        recorded.as_slice(),
        &[original],
        "nested supplier received the wrong routed action"
    );
    drop(recorded);

    let chrome_path = "turnstone/chrome";
    let chrome_hash = uxtree::node_id_for_path(chrome_path);
    let publisher = shell.a11y_adapter.as_ref().expect("native publisher");
    let (_, chrome_native) = publisher
        .native_address((TreeId::ROOT, chrome_hash))
        .expect("known chrome source was not served to the native adapter");
    let leaked = publisher.has_published_native_id(chrome_hash);
    write_receipt(
        &output.join("chrome-native-address.json"),
        &serde_json::json!({
            "known_source_path": chrome_path,
            "source_node_id": chrome_hash.0.to_string(),
            "native_node_id": chrome_native.0.to_string(),
            "frame_contains_source_hash": leaked,
        }),
    );
    assert!(
        !leaked,
        "published native frame exposed the Turnstone chrome path hash"
    );

    shell.content_sessions.remove(&node);
    shell.retire_surface_semantics(node);
    shell.push_a11y_tree();
    assert!(!shell.surface_a11y.ready(node));
    File::create_new(output.join("supplier-retired.ready")).unwrap();
    let mut observer_status = None;
    pump_until(
        &mut event_loop,
        &mut shell,
        |_| {
            observer_status = observer.0.try_wait().unwrap();
            observer_status.is_some()
        },
        "UIA target withdrawal",
    );
    assert!(
        observer_status.unwrap().success(),
        "observer refused UIA withdrawal"
    );
    assert!(output.join("uia-withdrawn.json").is_file());
    assert_eq!(trace.actions.lock().unwrap().len(), 1);
}
