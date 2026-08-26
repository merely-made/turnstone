//! The desktop shell: winit window + the shared present stack, raw input
//! mapped onto the canvas's semantic methods (continuous gestures) and onto
//! [`Action`]s (app intents), the ports (fetch + physics actors), and the
//! effect runner. The only module that touches a platform API; everything it
//! learns flows back through the spine.

mod drive;
mod effects;
mod events;
mod gestures;
mod keys;
mod render;
mod renderers;
#[cfg(all(feature = "weld", windows))]
mod surface_frames;
#[cfg(all(feature = "weld", windows))]
mod weld;
use render::{capture_composed, decode_sprite};
mod lens;
use lens::LensWindow;
mod input;
use input::pointer_button;

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use fetch::{FetchCommand, FetchUpdate};
use genet_documents::{
    LocalFetcher, ReaderSessionEngine, SmolwebInlineMediaPolicy, SmolwebSessionEngine, SmolwebTheme,
};
use genet_winit_host::SurfaceHost;
use image::ImageEncoder;
use inker::{DocumentSession, SessionClick, SessionRegistry, SessionSpawnRequest};
use mere::canvas::WHEEL_PAN_SCALE;
use netrender::external_texture::ExternalTexturePlacement;
use netrender::{ColorLoad, NetrenderOptions};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use winit::window::{Window, WindowId};

pub(crate) mod a11y_bridge;

use crate::panes::PaneContent;
use crate::settings_pane::LiveSettingsHandle;
use crate::settings_provider::ApplicationSettingsProvider;

use crate::action::{Action, Effect, Update};
use crate::app::App;
use crate::surface::{Rect, SurfaceKind};
use crate::{browse, session};

use netrender::Scene;

/// Engine-native smolweb lanes Turnstone can route to through Inker's default
/// policy. Spartan shares Gemini's gemtext lane; feed is selected by response
/// media type, while the other ids correspond directly to protocol schemes.
const SMOLWEB_SESSION_ENGINE_IDS: &[&str] = &[
    inker::routing::ENGINE_NEMATIC_GEMTEXT,
    inker::routing::ENGINE_NEMATIC_GOPHER,
    inker::routing::ENGINE_NEMATIC_FINGER,
    inker::routing::ENGINE_NEMATIC_SCROLL,
    inker::routing::ENGINE_NEMATIC_NEX,
    inker::routing::ENGINE_NEMATIC_GUPPY,
    inker::routing::ENGINE_NEMATIC_TITAN,
    inker::routing::ENGINE_NEMATIC_FEED,
];

/// The content lanes that are always available in an ordinary Turnstone
/// build. Keeping their construction together makes route availability an
/// inspectable fact instead of an assumption split across shell setup.
fn standard_content_engines() -> SessionRegistry<Scene> {
    let mut engines = SessionRegistry::new();
    engines.register(Box::new(genet_documents::LiverySessionEngine::new(
        LocalFetcher,
    )));
    engines.register(Box::new(ReaderSessionEngine::new(SmolwebTheme::System)));
    for engine_id in SMOLWEB_SESSION_ENGINE_IDS {
        engines.register(Box::new(
            SmolwebSessionEngine::new(*engine_id, LocalFetcher, SmolwebTheme::default())
                .with_inline_media(smolweb_inline_media_policy()),
        ));
    }
    engines
}

/// Turnstone opts into linked gemtext images, while keeping the presentation
/// and resource budgets user-configurable at process launch.
fn smolweb_inline_media_policy() -> SmolwebInlineMediaPolicy {
    let mut policy = SmolwebInlineMediaPolicy::images();
    policy.enabled = std::env::var("TURNSTONE_SMOLWEB_INLINE_IMAGES")
        .ok()
        .map(|value| !matches!(value.to_ascii_lowercase().as_str(), "0" | "false" | "off"))
        .unwrap_or(true);
    if let Some(limit) = std::env::var("TURNSTONE_SMOLWEB_INLINE_IMAGE_LIMIT")
        .ok()
        .and_then(|value| value.parse().ok())
    {
        policy.max_images = limit;
    }
    if let Some(megabytes) = std::env::var("TURNSTONE_SMOLWEB_INLINE_IMAGE_MB")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
    {
        policy.max_encoded_bytes_per_image = megabytes.saturating_mul(1024 * 1024);
    }
    policy
}

/// Turnstone's ordinary HTML route uses the clean-room Livery lane. Mere's
/// shared policy keeps the legacy incumbent default for other hosts, so the
/// browser makes its product choice at its own composition boundary.
fn standard_route_policy() -> inker::EngineRoutePolicy {
    let mut policy = mere::routing::route_policy();
    for rule in &mut policy.rules {
        if rule.engine_id == inker::routing::ENGINE_GENET_WEB {
            rule.engine_id = inker::routing::ENGINE_GENET_LIVERY.to_string();
        }
    }
    policy
}

/// Resolve a session-authored link against the node that owns that content
/// surface before lowering it to `OpenAddress`. Session engines preserve the
/// authored spelling for inspection; the host owns the navigation context.
fn content_link_target(app: &App, node: uuid::Uuid, href: &str) -> String {
    app.graph_runtimes
        .graph()
        .get_node_by_id(node)
        .map(|(_, owner)| {
            url::Url::parse(owner.url())
                .and_then(|base| base.join(href))
                .map(|target| target.to_string())
                .unwrap_or_else(|_| genet_documents::resolve_href(owner.url(), href))
        })
        .unwrap_or_else(|| href.to_string())
}

/// A pane's placeholder display label from its `PaneContent`. Title-cased tag
/// (the tags are single lowercase words); slice D replaces the placeholder with
/// the pane's real content.
fn pane_display_label(content: &PaneContent) -> String {
    crate::panes::pane_definition(content.kind_id().as_str())
        .map(|definition| definition.display_name.to_string())
        .unwrap_or_else(|| content.tag().to_string())
}

/// The scenario, parsed from `TURNSTONE_SCENARIO` (a
/// path). A parse error yields a stillborn scenario whose first `finish` reports
/// the failure — the harness learns WHY instead of timing out. `None` when the
/// env var is unset (the turnstone driver, or no driver, runs instead).
fn shared_scenario_from_env() -> Option<genet_probe::Scenario> {
    let path = std::path::PathBuf::from(std::env::var_os("TURNSTONE_SCENARIO")?);
    let body = std::fs::read_to_string(&path).unwrap_or_default();
    // A parse error becomes a scenario that logs why and fails a step (an
    // assert on a field no snapshot has), so the run reports RESULT fail with the
    // reason rather than timing out — the same courtesy turnstone's own driver pays.
    Some(match genet_probe::Scenario::parse(&body) {
        Ok(sc) => sc,
        Err(err) => {
            let fallback = format!("log parse error: {err}\nassert snap __never__ == 1");
            genet_probe::Scenario::parse(&fallback).expect("fallback scenario parses")
        }
    })
}

/// Where a shared run writes its captures and sentinel: `TURNSTONE_CAPTURE_DIR`, or
/// the scenario file's own directory.
fn shared_out_dir_from_env() -> std::path::PathBuf {
    let dir = std::env::var_os("TURNSTONE_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("TURNSTONE_SCENARIO")
                .map(std::path::PathBuf::from)
                .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        })
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// One surface's scene, produced by render's mutable first pass and consumed by
/// its immutable rasterization pass. Splitting the two keeps a content session's
/// mutable borrow off the immutable `host` borrow.
struct PlannedScene {
    id: u64,
    kind: SurfaceKind,
    placement: ExternalTexturePlacement,
    dims: (u32, u32),
    scene: Scene,
    // Stored as the `Copy` clear color (netrender's `ColorLoad` derives nothing,
    // so it cannot be moved out of the collected vec); wrapped at the call.
    clear: wgpu::Color,
}

/// One compositor source in plan order. A retained scene is rasterized by
/// Turnstone; an imported surface has already supplied a host-device view.
enum PlannedLayer {
    Scene(PlannedScene),
    Imported(CompositeLayer),
}

/// A rasterized surface ready to compose: its view and where it lands in the
/// frame. The self-capture path composes the same list, so the receipt is the
/// presented frame.
struct CompositeLayer {
    kind: SurfaceKind,
    view: wgpu::TextureView,
    placement: ExternalTexturePlacement,
}

#[derive(Clone, Copy)]
struct ActiveSurfaceTouch {
    pointer_id: i32,
    node: uuid::Uuid,
    is_primary: bool,
}

#[derive(Default)]
struct HostFileDrag {
    files: Vec<std::path::PathBuf>,
    target: Option<uuid::Uuid>,
}

/// The turnstone shell: app state plus the window, present stack, and ports
/// that drive it.
pub struct Shell {
    app: App,
    /// The shell-owned live value projection. Settings panes receive clones;
    /// the shell alone polls and applies the typed chrome snapshot.
    live_settings: LiveSettingsHandle,
    /// Wakes the loop when the physics or fetch actor has news.
    proxy: EventLoopProxy<()>,
    /// The fetch actor's command handle; dropping it ends the actor.
    fetch_handle: armillary::ActorHandle<FetchCommand>,
    /// Durable Gemini server pins. The protocol verifier shares this exact
    /// instance; the shell retains it for explicit change acceptance.
    gemini_trust: Arc<crate::gemini_trust::GeminiTrustStore>,
    /// Completed fetches, drained in `user_event` on each wake.
    fetch_rx: Receiver<FetchUpdate>,
    /// Serialized download custody writes, kept off the event-loop thread.
    download_handle: armillary::ActorHandle<crate::download::DownloadCommand>,
    /// Completed custody writes, drained beside fetch answers.
    download_rx: Receiver<Update>,
    /// The recycle-bin actor (the eidetic deleted-node bin at the session's
    /// bin dir); commands stage records / re-point on a session switch.
    bin_handle: armillary::ActorHandle<crate::recycle::BinCommand>,
    /// The bin's answers (BinListed / BinFailed), drained beside the fetches.
    bin_rx: Receiver<Update>,
    /// The trail-memory actor (browsing capture into the session's memory
    /// store; search wiring W1). Fed from the semantic-event drain; re-pointed
    /// and released on the same session edges as the bin.
    trail_handle: armillary::ActorHandle<crate::trail_memory::TrailCommand>,
    /// The trail's recall answers (RecallHits / RecallFailed), drained beside
    /// the bin's.
    trail_rx: Receiver<Update>,
    /// The retained-place worker. It owns every Gemot, Commons, chat, group,
    /// and redb handle for the active session.
    place_handle: armillary::ActorHandle<crate::place::worker::PlaceWorkerCommand>,
    /// Generation-tagged app-owned answers from the place worker.
    place_rx: Receiver<Update>,
    /// Last cursor position in physical px. winit's `MouseInput` carries no
    /// position, so the shell tracks it from `CursorMoved`.
    cursor: (f32, f32),
    /// Live Ctrl state, for the omnibar summon chords (Ctrl+L / Ctrl+K).
    ctrl: bool,
    /// Live Alt state, for the nav chords (Alt+Left / Alt+Right).
    alt: bool,
    /// Live Shift state, for the tear-out modifier arms (Ctrl+Shift = fork).
    shift: bool,
    /// The genet-probe scenario driver (activated by `TURNSTONE_SCENARIO`): the
    /// generic one-step-per-frame loop every genet app shares, driving this
    /// Shell through its
    /// `Automatable`/`Driveable` impl — the one scenario loop turnstone runs.
    /// `shared_out_dir` stays
    /// on `self` (the scenario is taken out during a tick) so `capture` can reach
    /// it. `shared_done` guards writing the sentinel exactly once.
    shared_scenario: Option<genet_probe::Scenario>,
    /// A bounded copy of semantic events after their ordinary consumers have
    /// received them. Automation drains this copy, so its assertions do not
    /// compete with trail memory for the app's one event stream.
    observed_events: VecDeque<String>,
    shared_out_dir: std::path::PathBuf,
    /// A capture the next `render` fulfills from the very views it presents
    /// (never a re-rasterization — the receipt must be the presented frame).
    pending_capture: Option<std::path::PathBuf>,
    /// A capture the next LENS render fulfills (the scenario's capture-lens
    /// verb; targets the first live lens window).
    pending_lens_capture: Option<std::path::PathBuf>,
    window: Option<Arc<Window>>,
    /// The OS AccessKit bridge for the primary window, when installed.
    a11y_adapter: Option<accesskit_winit::Adapter>,
    /// The latest projected tree, shared with the platform activation thread.
    a11y_shared: crate::shell::a11y_bridge::SharedTree,
    /// Frames since the OS tree was last refreshed; a cadence, not a vsync
    /// obligation, because the projection walks the graph and freezes the
    /// disclosed scene, which is not a per-frame cost worth paying.
    a11y_frames_since_push: u32,
    /// Where an assistive action on each projected node lands, rebuilt with
    /// every pushed tree so the table and the tree can never disagree for
    /// longer than one cadence.
    a11y_routes: crate::a11y::A11yRoutes,
    /// Assistive actions queued by the platform thread, drained here.
    a11y_actions: crate::shell::a11y_bridge::ActionQueue,
    host: Option<SurfaceHost>,
    width: u32,
    height: u32,
    /// The content port (rung 4, session-engines plan phase 4): the session
    /// registry does the engine-id dispatch, and the live sessions — retained,
    /// non-Send handles — live here, keyed by the same node ids App's
    /// ContentStates tracks. Ports own handles; App holds data.
    content_engines: SessionRegistry<netrender::Scene>,
    content_sessions:
        std::collections::HashMap<uuid::Uuid, Box<dyn DocumentSession<netrender::Scene>>>,
    /// Long-lived frame-streaming engines, separate from the retained document
    /// sessions above. The neutral inker registry chooses the producer; the
    /// shell owns its non-Send live handle and its imported frame cache.
    surface_engines: inker::SurfaceEngineRegistry,
    surface_producers: std::collections::HashMap<uuid::Uuid, Box<dyn inker::SurfaceProducer>>,
    /// Exact app request/query identity for progressive hosted find results.
    surface_find_requests: std::collections::HashMap<uuid::Uuid, (u64, String)>,
    #[cfg(all(feature = "weld", windows))]
    surface_frames:
        std::collections::HashMap<uuid::Uuid, Option<surface_frames::ImportedSurfaceFrame>>,
    /// A restored Weld-pinned node can request content before winit has created
    /// its device. Keep it requested until `resumed` instead of falsely
    /// reporting an unavailable engine.
    pending_surface_spawns: Vec<(uuid::Uuid, String)>,
    /// Configured Knot destination for typed Inspector clips. The handle owns
    /// neither file authority nor vault keys; it only queues endpoint intents.
    knot_clip: Option<crate::knot_authoring::KnotClipHandle>,
    /// Mere's routing vocabulary over inker's engine rules: address -> engine id.
    route_policy: inker::EngineRoutePolicy,
    /// Profile/origin user-agent policy, pending prompts, and the process-only
    /// credential provider. Graph data receives summaries, never secrets.
    web_policy: crate::web_policy::WebPolicyService,
    /// Monotonic epoch for the sessions' pump clock.
    epoch: std::time::Instant,
    /// In-flight fetch correlation: which node asked for each URL, noted
    /// before commanding the actor, reattached by the adapter on completion.
    pending_fetches: browse::PendingFetches,
    /// The surface a pointer press landed on, held until release (rung 5 slice
    /// B). Pointer routing captures on press so a press-drag-release stays with
    /// one surface: the canvas needs paired `pointer_down`/`pointer_up`, and a
    /// content click must not leak its release to the canvas beneath.
    pointer_capture: Option<crate::surface::SurfaceKind>,
    /// DOM `buttons` state for the mouse pointer. Pointer Events carries the
    /// full post-event mask, including on move, so CEF can distinguish hover
    /// from a pressed drag.
    surface_pointer_buttons: inker::PointerButtons,
    /// Winit touch ids are u64 and may be reused after a contact ends. Live
    /// contacts receive compact, collision-free DOM/CEF pointer ids and each
    /// retains its own content-surface capture until Up or Cancel.
    active_surface_touches: HashMap<u64, ActiveSurfaceTouch>,
    next_surface_touch_id: i32,
    /// Files offered by the OS drag manager and the current web-surface target.
    /// Graph drops keep using the addressed-content import path.
    host_file_drag: HostFileDrag,
    /// The frame-streaming content surface currently under the pointer. This
    /// scopes cursor-shape callbacks and restores the host default on leave.
    hovered_surface: Option<uuid::Uuid>,
    /// Whether the last scroll key delivered to focused content actually moved
    /// the page (`Some(true/false)`), or `None` if no content scroll key has
    /// been delivered. A probe for the scenario runner: it lets a receipt
    /// assert both that a page scrolled AND that an idempotent end (PageUp at
    /// the top) is honestly a no-op, so the receipt proves real offset
    /// semantics rather than a method that always returns true.
    content_scroll_moved: Option<bool>,
    /// The Roster pane's cambium grid (rung 5 slice D): a retained
    /// `GenetAppRunner` whose state and DOM persist between the frame that draws
    /// it and the click that hits it. `!Send`, like the content sessions, so it
    /// lives here rather than in App.
    /// The Gloss pane (minimap): the first pane whose cambium view carries a
    /// custom-paint leaf, so it owns a leaf registry beside its runner.
    /// The Trail pane: the sectioned list's first consumer (the hand-DOM Trail
    /// retired). Retained like the others.
    /// The Inspector pane: detail sections over app truth (inert content;
    /// the detail_panel's own contract). Retained like the others.
    /// The Workbench pane (rung 5 slice E): platen's tiling walked into cells
    /// wearing cambium tab strips. Retained like the others.
    /// The Apparatus pane (the settings row): the focused node's viewer
    /// override on a cambium radio_group. Retained like the others.
    /// The application-settings projection over the host provider. Retained
    /// like the other Cambium panes.
    /// Owner controls for the active retained Knot publishing service.
    /// The service owns carrier, vault read handle, tickets, and revocations;
    /// the pane only projects and commands it.
    publish_service: Option<Arc<crate::publish_service::KnotPublishingService>>,
    /// Recipient controls for a private ticket. This service does not need a
    /// local Knot authoring host, only the profile root it derives from.
    shared_knot_service: Option<Arc<crate::share_reader_service::KnotShareReaderService>>,
    device_receipts_service: Option<Arc<crate::device_receipts_service::DeviceReceiptsService>>,
    /// The Overmap pane (O1): the switcher as a graph view, retained like the
    /// Gloss minimap it mirrors.
    /// Every retained per-pane Cambium renderer, keyed by `PaneId`.
    /// See [`renderers::PaneRenderers`] for why they live in one place.
    renderers: renderers::PaneRenderers,
    /// Runtime product surface factories. Concrete product state never enters
    /// the shell; admission yields the erased sessions retained above.
    surface_providers: crate::contributed_surface::SurfaceProviderRegistry,
    /// Which pane the pointer is hovering (pane pointer-move routing): lets a
    /// move off a pane deliver its Leave so hover emphasis clears.
    hovered_pane: Option<crate::panes::PaneId>,
    /// The chrome, as a cambium view over a FOREST of window-roots (one
    /// shared document, one projection per window): retained + diffed, row
    /// clicks live, lens windows carry the caption chip. Replaces the
    /// hand-built `ui::chrome_scene`.
    chrome: crate::chrome_view::ChromeSurfaces,
    /// A workbench tab drag in flight, scoped to its source pane. Two
    /// Workbenches can be visible at once, so a member id alone cannot choose
    /// which Forme arrangement a release mutates.
    wb_tab_drag: Option<(crate::panes::PaneId, uuid::Uuid)>,
    /// A workbench divider drag in flight: the pressed band plus the pane's
    /// window origin (the walk is pane-local; pointer deliveries are window
    /// coords).
    wb_divider_drag: Option<(crate::workbench_tiling::WbDivider, (f32, f32))>,
    /// The divider drag in flight: the pressed seam's placement, held from
    /// press to release (like `pointer_capture`, which also points at it).
    /// Cursor moves turn into ratios through cambium's `Split::ratio_at` —
    /// the component owns the gesture math; the shell only feeds it points.
    divider_drag: Option<crate::pane::DividerPlacement>,
    /// A LENS window's seam drag in flight: which lens (ordinal) plus the
    /// pressed seam's placement in that window's tiling. Moves lower
    /// `SetSplitRatio` aimed at the lens's space; release persists once.
    lens_divider_drag: Option<(usize, crate::pane::DividerPlacement)>,
    /// Lens windows (rung 7, one-state-N-windows): the same graph through a
    /// window-owned camera. The primary window keeps the full pane/chrome
    /// experience; each lens renders the canvas with ITS `Viewport` installed
    /// around the pass and stashed back after — two windows on one graph hold
    /// distinct cameras over shared node positions (the canvas's install
    /// seam, exactly as the multi-window doctrine recorded).
    lens_windows: std::collections::HashMap<WindowId, LensWindow>,
    /// Lens windows requested but not yet created (window creation needs the
    /// `ActiveEventLoop`, which effects don't carry; the event handlers drain
    /// this while one is in scope).
    pending_windows: Vec<usize>,
}

impl Shell {
    pub fn new(proxy: EventLoopProxy<()>, address: Option<String>) -> Self {
        let (mut app, boot_effects) = App::boot(address.as_deref());
        let initial_settings = match ApplicationSettingsProvider::load(&app.data_root) {
            Ok(provider) => provider.settings().clone(),
            Err(error) => {
                tracing::warn!(%error, "application settings could not be loaded at shell startup");
                pandect::ApplicationSettings::default()
            }
        };
        let live_settings = LiveSettingsHandle::new(&initial_settings);
        app.apply_chrome_settings_snapshot(&live_settings.snapshot());
        let policy_path = crate::web_policy::default_policy_path(&app.data_root);
        let policy_registry = crate::web_policy::PermissionRegistry::load(
            "default",
            crate::web_policy::ProfileStorage::Persistent(policy_path),
        )
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "persistent web policy is unavailable; using an isolated in-memory profile");
            crate::web_policy::PermissionRegistry::private("default-fallback")
        });
        let request_timeout = std::env::var("TURNSTONE_WEB_REQUEST_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .map(std::time::Duration::from_millis)
            .unwrap_or_else(|| std::time::Duration::from_secs(30));
        let web_policy = crate::web_policy::WebPolicyService::new(policy_registry, request_timeout);

        // Gemini must never fall through errand's permissive trust default.
        // Corrupt trust state fails startup rather than silently forgetting
        // pins and treating a changed certificate as first contact.
        let gemini_trust = Arc::new(
            crate::gemini_trust::GeminiTrustStore::load(&app.data_root)
                .expect("Turnstone could not open its durable Gemini trust store"),
        );
        fetch::install_smolweb_tofu(gemini_trust.clone());

        // The fetch actor on its own armillary thread, waking this loop like
        // the physics actor does.
        let fetch_proxy = proxy.clone();
        let fetch_wake: armillary::Wake = Arc::new(move || {
            let _ = fetch_proxy.send_event(());
        });
        let (fetch_handle, fetch_rx) = fetch::spawn_fetcher(fetch_wake);

        let download_proxy = proxy.clone();
        let download_wake: armillary::Wake = Arc::new(move || {
            let _ = download_proxy.send_event(());
        });
        let (download_handle, download_rx) = crate::download::spawn_downloads(download_wake);

        // The recycle-bin actor over THIS session's bin store, waking the
        // loop the same way; it answers its spawn with the initial list.
        let bin_proxy = proxy.clone();
        let bin_wake: armillary::Wake = Arc::new(move || {
            let _ = bin_proxy.send_event(());
        });
        let (bin_handle, bin_rx) =
            crate::recycle::spawn_bin(bin_wake, crate::recycle::bin_dir(&app.session_dir()));

        // The trail-memory actor over THIS session's memory store (search
        // wiring W1 + W2): browsing capture in, recall answers out, behind
        // the same wake shape.
        let trail_proxy = proxy.clone();
        let trail_wake: armillary::Wake = Arc::new(move || {
            let _ = trail_proxy.send_event(());
        });
        let (trail_handle, trail_rx) = crate::trail_memory::spawn_trail(
            trail_wake,
            crate::trail_memory::memory_dir(&app.session_dir()),
        );

        let place_proxy = proxy.clone();
        let place_wake: armillary::Wake = Arc::new(move || {
            let _ = place_proxy.send_event(());
        });
        let (place_handle, place_rx) = crate::place::worker::spawn_place_worker(
            place_wake,
            app.identity.clone(),
            crate::place::worker::PlaceWorkerSettings::default(),
        );

        // The content port's ordinary lanes: both Genet static renderers plus
        // the engine-native smolweb family. Route policy selects one by
        // address/media type without app dispatch.
        let mut content_engines = standard_content_engines();
        let knot_proxy = proxy.clone();
        let knot_wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = knot_proxy.send_event(());
        });
        let mut knot_clip = None;
        let mut publish_service = None;
        let shared_knot_service = match crate::share_reader_service::KnotShareReaderService::start(
            app.identity.clone(),
        ) {
            Ok(service) => Some(Arc::new(service)),
            Err(error) => {
                tracing::warn!(%error, "Knot share reader is unavailable");
                None
            }
        };
        let device_receipts_service =
            match crate::device_receipts_service::DeviceReceiptsService::start() {
                Ok(service) => Some(Arc::new(service)),
                Err(error) => {
                    tracing::warn!(%error, "device receipts reader is unavailable");
                    None
                }
            };
        match crate::knot_authoring::KnotAuthoringEngine::from_env(knot_wake) {
            Ok(Some(mut engine)) => {
                knot_clip = engine.clip_handle();
                if let Some(source) = engine.take_publish_source() {
                    match crate::publish_service::KnotPublishingService::start(
                        source,
                        app.identity.clone(),
                    ) {
                        Ok(service) => publish_service = Some(Arc::new(service)),
                        Err(error) => tracing::warn!(%error, "Knot publishing is unavailable"),
                    }
                }
                content_engines.register(Box::new(engine));
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "Knot authoring is unavailable"),
        }

        let mut surface_providers = crate::contributed_surface::SurfaceProviderRegistry::new();
        surface_providers
            .register_provider(crate::knot_document_surface::KnotDocumentProvider::default())
            .expect("the built-in Knot document provider is unique");
        let mut shell = Self {
            app,
            live_settings,
            proxy,
            fetch_handle,
            gemini_trust,
            fetch_rx,
            download_handle,
            download_rx,
            bin_handle,
            bin_rx,
            trail_handle,
            trail_rx,
            place_handle,
            place_rx,
            cursor: (0.0, 0.0),
            ctrl: false,
            alt: false,
            shift: false,
            shared_scenario: shared_scenario_from_env(),
            observed_events: VecDeque::with_capacity(128),
            shared_out_dir: shared_out_dir_from_env(),
            pending_capture: None,
            pending_lens_capture: None,
            window: None,
            a11y_adapter: None,
            a11y_shared: std::sync::Arc::new(std::sync::Mutex::new(None)),
            a11y_frames_since_push: u32::MAX / 2,
            a11y_routes: crate::a11y::A11yRoutes::new(),
            a11y_actions: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            host: None,
            width: 1024,
            height: 600,
            content_engines,
            content_sessions: std::collections::HashMap::new(),
            surface_engines: inker::SurfaceEngineRegistry::new(),
            surface_producers: std::collections::HashMap::new(),
            surface_find_requests: std::collections::HashMap::new(),
            #[cfg(all(feature = "weld", windows))]
            surface_frames: std::collections::HashMap::new(),
            pending_surface_spawns: Vec::new(),
            knot_clip,
            route_policy: standard_route_policy(),
            web_policy,
            epoch: std::time::Instant::now(),
            pending_fetches: browse::PendingFetches::default(),
            pointer_capture: None,
            surface_pointer_buttons: inker::PointerButtons::NONE,
            active_surface_touches: HashMap::new(),
            next_surface_touch_id: 2,
            host_file_drag: HostFileDrag::default(),
            hovered_surface: None,
            content_scroll_moved: None,
            publish_service,
            shared_knot_service,
            device_receipts_service,
            renderers: Default::default(),
            surface_providers,
            hovered_pane: None,
            chrome: crate::chrome_view::ChromeSurfaces::new(),
            wb_tab_drag: None,
            wb_divider_drag: None,
            divider_drag: None,
            lens_divider_drag: None,
            lens_windows: std::collections::HashMap::new(),
            pending_windows: Vec::new(),
        };
        shell.run_effects(boot_effects);
        shell
    }

    fn has_live_content(&self, node: &uuid::Uuid) -> bool {
        self.content_sessions.contains_key(node) || self.surface_producers.contains_key(node)
    }

    fn clear_surface_content(&mut self) {
        self.surface_producers.clear();
        self.surface_find_requests.clear();
        #[cfg(all(feature = "weld", windows))]
        self.surface_frames.clear();
    }

    #[cfg(all(feature = "weld", windows))]
    fn ensure_weld_engine(&mut self) -> Result<(), String> {
        if self
            .surface_engines
            .contains(inker::routing::ENGINE_WELD_CHROMIUM)
        {
            return Ok(());
        }
        let host = self
            .host
            .as_ref()
            .ok_or_else(|| "the Turnstone wgpu host is not ready".to_string())?;
        let cef_path = std::env::var_os("TURNSTONE_CEF_PATH")
            .or_else(|| std::env::var_os("CEF_PATH"))
            .map(std::path::PathBuf::from)
            .ok_or_else(|| {
                "set TURNSTONE_CEF_PATH (or CEF_PATH) before selecting weld.chromium".to_string()
            })?;
        let cache_root = self.app.data_root.join("weld").join("cef-cache");
        let runtime = weld::initialize_runtime(cef_path, &cache_root)?;
        let factory =
            weld::TurnstoneWeldFactory::new(runtime, host.device().clone(), host.queue().clone());
        self.surface_engines
            .register(Box::new(weld_engine::WeldEngine::new(Arc::new(factory))));
        Ok(())
    }

    #[cfg(not(all(feature = "weld", windows)))]
    fn ensure_weld_engine(&mut self) -> Result<(), String> {
        Err("weld.chromium is available only in a Windows build with `--features weld`".into())
    }

    /// Poll the value projection after a settings pane persists a write. The
    /// shell owns the redraw boundary and the `ShellChromeConfig`; the pane
    /// remains a retained form with no renderer back-channel.
    fn poll_live_settings(&mut self) -> bool {
        self.app
            .apply_chrome_settings_snapshot(&self.live_settings.snapshot())
    }

    /// Lower one app intent through the spine and run what falls out. Syncs
    /// the window's IME enablement to controlled chrome fields on transitions
    /// (a platform call, so it lives here, not in `update`).
    fn act(&mut self, action: Action) {
        let accepted_text = self.accepts_controlled_text();
        let closing = matches!(&action, Action::CloseActivePane)
            .then_some(self.app.active_pane)
            .flatten();
        let effects = self.app.update(action);
        if let Some(pane) = closing
            && self.app.space_of(pane).is_none()
        {
            self.evict_pane_renderer(pane);
        }
        let accepts_text = self.accepts_controlled_text();
        if accepts_text != accepted_text
            && let Some(window) = self.window.as_ref()
        {
            window.set_ime_allowed(accepts_text);
        }
        self.run_effects(effects);
    }

    fn accepts_controlled_text(&self) -> bool {
        self.app.omnibar.open
            || self.app.document_find.open
            || self.app.user_agent_decision.accepts_text()
    }

    fn sync_ime_allowed(&self) {
        if let Some(window) = self.window.as_ref() {
            window.set_ime_allowed(self.accepts_controlled_text());
        }
    }

    fn evict_pane_renderer(&mut self, pane: crate::panes::PaneId) {
        self.renderers.evict(pane);
        if self.hovered_pane == Some(pane) {
            self.hovered_pane = None;
        }
    }

    /// The current surface plan, from app truth plus the window size. The one
    /// place render and input agree on which surfaces exist and where, so a
    /// pointer always hits exactly what the last frame drew. The base layer is
    /// the frisket pane tree (rung 5 slice C): the Orrery leaf is the canvas,
    /// every other leaf a pane. Content insets over the canvas; chrome sits on
    /// top.
    fn surface_plan(&self) -> Vec<crate::surface::Surface> {
        let area = Rect::full(self.width.max(1), self.height.max(1));
        // A5 promotes a space to its `SpaceBlueprint` only when it enters the
        // float layer. Its placements then drive this exact compositor path;
        // legacy Frisket still supplies the payload and retained renderer
        // lookup during the A8 persistence migration.
        let (pane_rects, divider_rects, float_rects): (
            Vec<(crate::panes::PaneId, Rect)>,
            Vec<(u32, Rect)>,
            Vec<(crate::panes::PaneId, Rect)>,
        ) = match self.app.blueprint_space(crate::action::SpaceRef::Primary) {
            Some(blueprint) => {
                let placements = crate::panes::place_space(blueprint, area, self.app.maximized);
                (
                    placements
                        .panes
                        .into_iter()
                        .map(|pane| (pane.id, pane.rect))
                        .collect(),
                    placements
                        .dividers
                        .into_iter()
                        .map(|divider| (divider.index, divider.rect))
                        .collect(),
                    placements
                        .floats
                        .into_iter()
                        .map(|pane| (pane.id, pane.rect))
                        .collect(),
                )
            }
            None => {
                let placements =
                    crate::pane::place_panes(&self.app.frisket, area, self.app.maximized);
                (
                    placements
                        .panes
                        .into_iter()
                        .map(|pane| (pane.id, pane.rect))
                        .collect(),
                    placements
                        .dividers
                        .into_iter()
                        .map(|divider| (divider.index, divider.rect))
                        .collect(),
                    Vec::new(),
                )
            }
        };
        let mut graph_rects = Vec::new();
        let mut base: Vec<(SurfaceKind, Rect)> = pane_rects
            .iter()
            .filter_map(|(id, rect)| {
                self.app
                    .pane_content(*id)
                    .map(|content| (id, rect, content))
            })
            .map(|(id, rect, content)| {
                if matches!(content, PaneContent::Orrery) {
                    graph_rects.push((*id, *rect));
                    (SurfaceKind::Graph(*id), *rect)
                } else if let PaneContent::Tile(m) = content
                    && self.has_live_content(&m)
                {
                    // A pinned Tile pane with a live session IS a content
                    // surface at the pane's rect — same keyed path as an
                    // inset or workbench tile, so input routes for free.
                    (SurfaceKind::Content(*m), *rect)
                } else {
                    (SurfaceKind::Pane(*id), *rect)
                }
            })
            .collect();
        // Each seam is its own thin surface, so it paints (an empty scene over
        // the seam clear colour) and takes the divider drag.
        base.extend(
            divider_rects
                .iter()
                .map(|(index, rect)| (SurfaceKind::Divider(*index), *rect)),
        );
        // Workbench tiles (rung 5 slice E): the Workbench pane's cells, walked
        // at the pane's WINDOW rect, compose each visible (active) tile with a
        // live session as its own content surface at the cell's body rect —
        // the same keyed path the focused inset uses, so tile input routing
        // (wheel, clicks, focus) arrives through the existing Content arms.
        let workbench_panes: Vec<_> = pane_rects
            .iter()
            .filter(|(id, _)| matches!(self.app.pane_content(*id), Some(PaneContent::Workbench)))
            .map(|(id, rect)| (*id, *rect))
            .collect();
        let tiles: Vec<(uuid::Uuid, Rect)> = workbench_panes
            .iter()
            .flat_map(|(pane, rect)| {
                let geom = self
                    .app
                    .workbench_for_pane(*pane)
                    .and_then(|workbench| workbench.to_arrangement().1);
                crate::workbench_tiling::place_workbench(geom.as_ref(), *rect)
                    .cells
                    .iter()
                    .filter_map(|c| {
                        let m = c.active_member()?;
                        self.content_sessions
                            .contains_key(&m)
                            .then(|| (m, c.body()))
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        // Content overlays the canvas pane (when it is shown); a live node's
        // document insets within the graph, not over a maximized pane. A node
        // showing as a workbench tile is not ALSO inset over the canvas: one
        // session, one surface, or the two frame at fighting sizes. That rule
        // holds ACROSS windows too — when the workbench pane tore out to a
        // lens, its tiles render THERE (the lens's plan walks them), so the
        // same membership excludes the inset here.
        let wb_in_lens = workbench_panes.is_empty()
            && self.app.lenses.iter().flatten().any(|space| {
                space
                    .iter_leaves()
                    .any(|(_, c, _)| matches!(c, PaneContent::Workbench))
            });
        let tiled_in_lens = |id: &uuid::Uuid| {
            wb_in_lens && {
                self.app
                    .lenses
                    .iter()
                    .enumerate()
                    .filter_map(|(ordinal, space)| {
                        space.as_ref().and_then(|space| {
                            space
                                .iter_leaves()
                                .find(|(_, content, _)| matches!(content, PaneContent::Workbench))
                                .and_then(|(pane, _, _)| self.app.workbench_for_pane(pane))
                                .and_then(|workbench| workbench.to_arrangement().1)
                                .map(|geometry| (ordinal, geometry))
                        })
                    })
                    .any(|(_, geom)| {
                        crate::workbench_tiling::place_workbench(Some(&geom), area)
                            .cells
                            .iter()
                            .any(|c| c.active_member() == Some(*id))
                    })
            }
        };
        // A pinned Tile pane claims its member wherever its space shows.
        let tile_paned = |id: &uuid::Uuid| {
            self.app
                .frisket
                .iter_leaves()
                .chain(
                    self.app
                        .lenses
                        .iter()
                        .flatten()
                        .flat_map(|s| s.iter_leaves()),
                )
                .any(|(_, c, _)| matches!(c, PaneContent::Tile(m) if *m == *id))
        };
        let focused_graph = self
            .app
            .focused_graph_pane()
            .or_else(|| graph_rects.first().map(|(pane, _)| *pane));
        let content = focused_graph.and_then(|pane| {
            let cr = graph_rects
                .iter()
                .find(|(candidate, _)| *candidate == pane)
                .map(|(_, rect)| *rect)?;
            self.app
                .graph_pane_focused_member(pane)
                .filter(|id| self.has_live_content(id))
                .filter(|id| !tiles.iter().any(|(t, _)| t == id))
                .filter(|id| !tiled_in_lens(id))
                .filter(|id| !tile_paned(id))
                .map(|node| (node, crate::surface::content_rect(cr)))
        });
        let caption = focused_graph
            .and_then(|pane| self.app.graph_for_pane(pane))
            .and_then(|graph| self.app.graph_runtimes.canvas(graph))
            .and_then(crate::app::focused_caption);
        let chrome = (self.app.user_agent_decision.is_open()
            || self.app.document_find.open
            || self.app.shell_chrome_config().projects_shellbar() && caption.is_some()
            || self.app.omnibar.open && self.app.shell_chrome_config().projects_omnibar())
        .then_some(area);
        let mut surfaces = crate::surface::assemble(&base, &tiles, content, None);
        surfaces.extend(float_rects.into_iter().filter_map(|(id, rect)| {
            let content = self.app.pane_content(id)?;
            let kind = match content {
                PaneContent::Orrery => SurfaceKind::Graph(id),
                PaneContent::Tile(member) if self.has_live_content(member) => {
                    SurfaceKind::Content(*member)
                }
                _ => SurfaceKind::Pane(id),
            };
            Some(crate::surface::Surface {
                id: crate::surface::SurfaceId::for_kind(kind),
                kind,
                rect,
            })
        }));
        if let Some(rect) = chrome {
            surfaces.push(crate::surface::Surface {
                id: crate::surface::SurfaceId::CHROME,
                kind: SurfaceKind::Chrome,
                rect,
            });
        }
        surfaces
    }

    /// A pane's `PaneContent`, looked up from the frisket tree by id.
    fn pane_content(&self, id: crate::panes::PaneId) -> Option<PaneContent> {
        self.app.pane_content(id).cloned()
    }

    /// A pane's display label, looked up from the frisket tree by id.
    fn pane_label(&self, id: crate::panes::PaneId) -> String {
        self.pane_content(id)
            .map(|content| pane_display_label(&content))
            .unwrap_or_default()
    }

    /// Click the list-pane (Trail/Roster) row whose text contains `substr`
    /// (scenario `click-row`). The shell owns the pane rects and rows, so it
    /// resolves the row's window position and delivers a real click through the
    /// shared pointer path — a receipt names a row by text, not pixels.

    /// A pane click's resulting Actions, by kind — the cambium round trip
    /// (hit-test the runner's DOM, dispatch, convert what bubbles) packaged
    /// for any window. Lens windows drive this; the primary press arm carries
    /// its own copy of these round trips today (collapsing it here is a
    /// follow-on simplification). Side-mirrors happen here (roster_tab, the
    /// gloss Expand focus, Trail's not-yet-wired Recover note); durable
    /// intents come back as Actions for the caller to lower.
    fn pane_click_actions(
        &mut self,
        pane_id: crate::panes::PaneId,
        content: &PaneContent,
        local: (f32, f32),
        dims: (u32, u32),
    ) -> Vec<Action> {
        let (lx, ly) = local;
        let (rw, rh) = dims;
        let mut out = Vec::new();
        match content {
            PaneContent::Trail => {
                if let Some(pane) = self.renderers.trail.get_mut(&pane_id) {
                    for action in pane.click(lx, ly, rw, rh) {
                        match action {
                            crate::trail_pane::TrailPaneAction::Navigate(url) => {
                                out.push(Action::OpenAddress(url))
                            }
                            crate::trail_pane::TrailPaneAction::Recover(id) => {
                                match id.parse::<uuid::Uuid>() {
                                    Ok(id) => out.push(Action::RecoverDeletedNode(id)),
                                    Err(_) => {
                                        self.app.note(crate::observe::AppEvent::InteractionMissed {
                                            what: "recover",
                                            target: id.clone(),
                                        })
                                    }
                                }
                            }
                            crate::trail_pane::TrailPaneAction::RecoverSession(id) => {
                                if let Ok(id) = id.parse::<uuid::Uuid>() {
                                    out.push(Action::RecoverSession(
                                        crate::panes::SessionId::from_uuid(id),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            PaneContent::Roster => {
                if let Some(grid) = self.renderers.roster.get_mut(&pane_id) {
                    let actions = grid.click(lx, ly, rw, rh);
                    self.app.roster_tab = grid.selected_tab().0;
                    for action in actions {
                        match action {
                            crate::cambium_pane::RosterAction::Navigate(url) => {
                                out.push(Action::OpenAddress(url))
                            }
                        }
                    }
                }
            }
            PaneContent::Gloss(_) => {
                if let Some(pane) = self.renderers.gloss.get_mut(&pane_id) {
                    for intent in pane.click(lx, ly, rw, rh) {
                        match intent {
                            crate::swatch_pane::SwatchIntent::Activate(
                                crate::swatch_pane::SwatchActivate::Open(url),
                            ) => out.push(Action::OpenAddress(url)),
                            crate::swatch_pane::SwatchIntent::Activate(
                                crate::swatch_pane::SwatchActivate::Switch(id),
                            ) => out.push(Action::SwitchSession(id)),
                            // A composed Removed row: recover by ORIGINAL id.
                            crate::swatch_pane::SwatchIntent::Activate(
                                crate::swatch_pane::SwatchActivate::Recover(id),
                            ) => out.push(Action::RecoverDeletedNode(id)),
                            crate::swatch_pane::SwatchIntent::Expand => {
                                self.app.focus = crate::surface::FocusTarget::Graph(
                                    self.app.default_graph_pane(),
                                );
                            }
                        }
                    }
                }
            }
            PaneContent::Apparatus => {
                if let Some(pane) = self.renderers.apparatus.get_mut(&pane_id) {
                    for intent in pane.click(lx, ly, rw, rh) {
                        match intent {
                            crate::apparatus_pane::ApparatusIntent::SetViewer(viewer) => {
                                if let Some(member) = self.app.graph_runtimes.focused_member() {
                                    out.push(Action::SetViewerOverride { member, viewer });
                                }
                            }
                        }
                    }
                }
            }
            PaneContent::Registered(kind) if kind.as_str() == crate::panes::kind::SETTINGS => {
                if let Some(pane) = self.renderers.settings.get_mut(&pane_id) {
                    pane.click(lx, ly, rw, rh);
                }
            }
            PaneContent::Registered(kind) if kind.as_str() == crate::panes::kind::PUBLISHING => {
                if let Some(pane) = self.renderers.publish.get_mut(&pane_id) {
                    pane.click(lx, ly, rw, rh);
                }
            }
            PaneContent::Registered(kind) if kind.as_str() == crate::panes::kind::SHARED_KNOT => {
                if let Some(pane) = self.renderers.shared_knot.get_mut(&pane_id) {
                    pane.click(lx, ly, rw, rh);
                }
            }
            PaneContent::Registered(kind)
                if kind.as_str() == crate::panes::kind::DEVICE_RECEIPTS =>
            {
                if let Some(pane) = self.renderers.device_receipts.get_mut(&pane_id) {
                    pane.click(lx, ly, rw, rh);
                }
            }
            PaneContent::Registered(kind)
                if kind.as_str() == crate::panes::kind::FROZEN_PROJECTION =>
            {
                if let Some(pane) = self.renderers.frozen_projection.get_mut(&pane_id) {
                    pane.click(lx, ly, rw, rh);
                }
            }
            PaneContent::Inspector => {
                if let Some(pane) = self.renderers.inspector.get_mut(&pane_id)
                    && pane.click(lx, ly, rw, rh).into_iter().any(|intent| {
                        matches!(intent, crate::inspector_pane::InspectorIntent::ClipToKnot)
                    })
                {
                    self.clip_focused_document_to_knot();
                }
            }
            _ => {}
        }
        out
    }

    fn projected_a11y_tree(
        &self,
    ) -> (
        uxtree::UxTree,
        crate::a11y::A11yRoutes,
        Option<accesskit::NodeId>,
    ) {
        let pane_rects = self
            .surface_plan()
            .into_iter()
            .filter_map(|surface| match surface.kind {
                crate::surface::SurfaceKind::Pane(pane) => Some((pane, surface.rect)),
                _ => None,
            })
            .collect();
        let focused_pane = match self.app.focus {
            crate::surface::FocusTarget::Pane(pane) => Some(pane),
            _ => None,
        };
        let contributed = crate::contributed_a11y::project(
            &self.renderers.contributed,
            &pane_rects,
            focused_pane,
        );
        let contributed_focus = contributed.focus;
        let (tree, mut routes) =
            crate::a11y::project_app_with_routes_and_contributions(&self.app, contributed.trees);
        routes.extend(contributed.routes.into_iter().map(|(id, route)| {
            (
                id,
                crate::a11y::A11yRoute::Contributed {
                    pane: route.pane,
                    node: route.node,
                },
            )
        }));
        let focus = contributed_focus
            .filter(|focus| tree.nodes.iter().any(|(candidate, _)| candidate == focus));
        (tree, routes, focus)
    }

    /// Refresh the OS accessibility tree on a frame cadence.
    ///
    /// Thirty frames is about half a second at sixty; the projection walks
    /// the graph and re-freezes the disclosed scene, so per-frame would be
    /// paying a solve for readers that poll far slower than that.
    fn push_a11y_tree(&mut self) {
        const CADENCE_FRAMES: u32 = 30;
        if self.a11y_adapter.is_none() {
            return;
        }
        self.a11y_frames_since_push = self.a11y_frames_since_push.saturating_add(1);
        if self.a11y_frames_since_push < CADENCE_FRAMES {
            return;
        }
        self.a11y_frames_since_push = 0;
        let (tree, routes, focus) = self.projected_a11y_tree();
        self.a11y_routes = routes;
        let mut update = tree.to_tree_update(focus);
        // Narrator refuses to walk past a node without a bounding rectangle:
        // UIA treats boundless elements as off-screen, so the tree served
        // without these read as "three buttons, the omnibar, then nothing".
        // Contributed retained DOM nodes already carry exact pane-translated
        // fragment bounds. App-authored structural nodes do not yet, so only
        // those boundless nodes claim the window's extent: coarse geometry,
        // correct names and structure, without overwriting precise surfaces.
        let bounds = accesskit::Rect {
            x0: 0.0,
            y0: 0.0,
            x1: self.width.max(1) as f64,
            y1: self.height.max(1) as f64,
        };
        for (_, node) in &mut update.nodes {
            if node.bounds().is_none() {
                node.set_bounds(bounds);
            }
        }
        *self.a11y_shared.lock().expect("a11y tree slot poisoned") = Some(update.clone());
        self.a11y_adapter
            .as_mut()
            .expect("adapter presence checked above")
            .update_if_active(|| update);
    }

    /// Drain assistive actions queued by the platform thread and lower each
    /// through the route table. Runs on the main thread, woken by the same
    /// proxy the actors use.
    fn drain_a11y_actions(&mut self) {
        let requests: Vec<accesskit::ActionRequest> = std::mem::take(
            &mut *self
                .a11y_actions
                .lock()
                .expect("a11y action queue poisoned"),
        );
        for request in requests {
            if !matches!(
                request.action,
                accesskit::Action::Click | accesskit::Action::Focus
            ) {
                continue;
            }
            let route = self.a11y_routes.get(&request.target_node).cloned();
            if let Some(crate::a11y::A11yRoute::Contributed { pane, node }) = route.as_ref() {
                let (pane, node) = (*pane, *node);
                let landed = if let Some(surface) = self.renderers.contributed.get_mut(pane) {
                    surface.accessibility_action(request.action, node);
                    true
                } else {
                    false
                };
                if !landed {
                    self.app.note(crate::observe::AppEvent::InteractionMissed {
                        what: "a11y-action",
                        target: format!("contributed pane {}", pane.0),
                    });
                } else if request.action == accesskit::Action::Focus {
                    self.app.focus = crate::surface::FocusTarget::Pane(pane);
                    self.app.active_pane = Some(pane);
                    self.app.raise_floating_pane(pane);
                }
                self.request_redraw();
                continue;
            }
            // App-only routes are activation routes. A reader moving focus
            // across them must not perform their default action.
            if request.action != accesskit::Action::Click {
                continue;
            }
            let effects =
                crate::a11y::apply_route(&mut self.app, route.as_ref(), request.target_node);
            self.run_effects(effects);
            self.request_redraw();
        }
    }

    fn clip_focused_document_to_knot(&mut self) {
        let Some(handle) = self.knot_clip.clone() else {
            return;
        };
        let Some(member) = self.app.graph_runtimes.focused_member() else {
            return;
        };
        let Some(clip) = self
            .content_sessions
            .get(&member)
            .and_then(|session| session.clip())
        else {
            return;
        };
        if let Err(error) = handle.insert(clip) {
            tracing::warn!(%error, "Knot clip could not be queued");
        }
    }

    /// Run a scenario `script` step through the Piccolo control lane and lower
    /// its Actions through the same `act` spine a keypress takes — the
    /// automation runner of the "one description, two runners" pair. Without
    /// the `piccolo` feature the step is an honest, attributable failure
    /// rather than a silent skip.
    #[cfg(feature = "piccolo")]
    fn run_scenario_script(&mut self, source: &str) {
        match crate::script::run_control(&self.app, source, 5000) {
            Ok(actions) => {
                for action in actions {
                    self.act(action);
                }
            }
            Err(err) => {
                tracing::warn!(%err, "scenario script failed");
                self.app.note(crate::observe::AppEvent::InteractionMissed {
                    what: "script",
                    target: err,
                });
            }
        }
    }

    #[cfg(not(feature = "piccolo"))]
    fn run_scenario_script(&mut self, _source: &str) {
        tracing::warn!("scenario `script` step needs the `piccolo` feature; skipped");
        self.app.note(crate::observe::AppEvent::InteractionMissed {
            what: "script",
            target: "piccolo feature off".to_string(),
        });
    }

    /// Advance the self-drive scenario one step after each rendered frame.
    /// Steps lower to Actions through the same spine as a keypress; a Done
    /// tick writes the sentinel and exits WITHOUT saving the session (a
    /// scenario never mutates the profile it ran against).
    /// Write the shared driver's outcome in turnstone's `scenario.done` format
    /// (first line `RESULT ok`/`RESULT fail`, then the log), so the same headed
    /// harness that waits on the turnstone driver reads a shared run identically.
    fn write_shared_done(&self, outcome: &genet_probe::Outcome) {
        let result = if outcome.ok { "ok" } else { "fail" };
        let mut body = format!("RESULT {result}\n");
        for line in &outcome.log {
            body.push_str(line);
            body.push('\n');
        }
        let _ = std::fs::write(self.shared_out_dir.join("scenario.done"), body);
    }

    fn scenario_pump(&mut self, event_loop: &ActiveEventLoop) {
        // The shared genet-probe driver, when active, takes the frame: take the
        // scenario out (so `tick(self)` can borrow the Shell mutably), tick it,
        // put it back — or, on Done, write the `scenario.done` sentinel in
        // turnstone's format and exit. Mutually exclusive with the turnstone driver.
        if let Some(mut shared) = self.shared_scenario.take() {
            use genet_probe::Progress;
            match shared.tick(self) {
                Progress::Done => {
                    let outcome = shared.finish();
                    self.write_shared_done(&outcome);
                    event_loop.exit();
                }
                Progress::Running => {
                    self.request_redraw();
                    self.shared_scenario = Some(shared);
                }
            }
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gate 1 of the smolweb browser gap analysis: Turnstone's default route
    /// for a Gemini address must name a registered retained-session engine,
    /// and that engine must lower real gemtext into paint plus a clickable
    /// navigation target. The body is supplied at the spawn seam so the
    /// receipt is deterministic and does not depend on a public capsule.
    #[test]
    fn gemini_route_lands_in_a_live_nematic_session() {
        let engines = standard_content_engines();
        let decision = standard_route_policy().route(&inker::EngineRouteRequest {
            workspace_id: inker::WorkspaceRouteId::new("turnstone-test"),
            view: None,
            node: None,
            address: "gemini://capsule.test/".to_string(),
            content_type: None,
            pinned_engine: None,
        });

        assert_eq!(decision.engine_id, inker::routing::ENGINE_NEMATIC_GEMTEXT);
        assert!(engines.contains(&decision.engine_id));

        let request = SessionSpawnRequest::new("gemini://capsule.test/")
            .with_body("# Turnstone Capsule\n\n=> /next Follow the next link\n")
            .with_viewport(640, 480);
        let mut session = engines
            .spawn(&decision.engine_id, &request)
            .expect("the registered Gemini lane spawns");
        let scene = session.frame(640, 480);
        assert!(
            scene
                .ops
                .iter()
                .any(|op| matches!(op, netrender::SceneOp::GlyphRun(_))),
            "gemtext text reaches the retained scene"
        );
        let report = session.inspect().expect("Nematic exposes structure");
        assert_eq!(report.title.as_deref(), Some("Turnstone Capsule"));
        assert_eq!(report.links, vec!["/next"]);

        let link = session.links().into_iter().next().expect("laid-out link");
        let [x, y, width, height] = link.rect;
        let SessionClick::Navigate(href) = session.click_at(x + width / 2.0, y + height / 2.0)
        else {
            panic!("the laid-out gemtext link must navigate");
        };

        let mut app = App::test_stub();
        let _ = app.update(Action::OpenAddress(
            "gemini://capsule.test/start".to_string(),
        ));
        let node = app
            .graph_runtimes
            .focused_member()
            .expect("opened capsule node");
        assert_eq!(
            content_link_target(&app, node, &href),
            "gemini://capsule.test/next"
        );
    }

    #[test]
    fn automatic_html_routes_to_the_registered_livery_session() {
        let engines = standard_content_engines();
        let decision = standard_route_policy().route(&inker::EngineRouteRequest {
            workspace_id: inker::WorkspaceRouteId::new("turnstone-test"),
            view: None,
            node: None,
            address: "https://example.test/".to_string(),
            content_type: Some("text/html".to_string()),
            pinned_engine: None,
        });

        assert_eq!(decision.engine_id, inker::routing::ENGINE_GENET_LIVERY);
        assert!(engines.contains(&decision.engine_id));
        assert!(!engines.contains(inker::routing::ENGINE_GENET_WEB));
    }

    #[test]
    fn reader_pin_uses_held_html_and_switching_back_restores_original_rendering() {
        let engines = standard_content_engines();
        assert!(engines.contains(inker::routing::ENGINE_GENET_READER));
        let source = "<html><head><title>Original Page</title></head><body><nav>Site chrome</nav>\
            <main><h1>Reader Heading</h1><p>This substantial article paragraph proves the shared \
            reader lane while retaining the original source bytes.</p></main></body></html>";
        let retained = source.to_string();
        let reader_decision = standard_route_policy().route(&inker::EngineRouteRequest {
            workspace_id: inker::WorkspaceRouteId::new("turnstone-test"),
            view: None,
            node: None,
            address: "https://example.test/story".to_string(),
            content_type: Some("text/html".to_string()),
            pinned_engine: Some(inker::routing::ENGINE_GENET_READER.to_string()),
        });
        assert_eq!(
            reader_decision.engine_id,
            inker::routing::ENGINE_GENET_READER
        );
        let request = SessionSpawnRequest::new("https://example.test/story")
            .with_body(source)
            .with_content_type("text/html")
            .with_viewport(640, 480);
        let reader = engines
            .spawn(&reader_decision.engine_id, &request)
            .expect("reader spawns from held bytes");
        let reader_report = reader.inspect().expect("reader inspection");
        assert_eq!(reader_report.title.as_deref(), Some("Reader Heading"));
        assert!(reader_report.lineage.is_some());

        let original_decision = standard_route_policy().route(&inker::EngineRouteRequest {
            workspace_id: inker::WorkspaceRouteId::new("turnstone-test"),
            view: None,
            node: None,
            address: "https://example.test/story".to_string(),
            content_type: Some("text/html".to_string()),
            pinned_engine: Some(inker::routing::ENGINE_GENET_LIVERY.to_string()),
        });
        let original = engines
            .spawn(&original_decision.engine_id, &request)
            .expect("original renderer respawns from the same held bytes");
        assert_eq!(
            original.inspect().and_then(|report| report.title),
            Some("Original Page".to_string())
        );
        assert_eq!(
            retained, source,
            "reader extraction does not mutate held source"
        );
    }

    /// The one-state-N-windows invariant (rung 7): two windows on one graph
    /// hold DISTINCT cameras over shared positions. Install/stash through the
    /// canvas's viewport seam keeps a pan in one lens out of the other.
    #[test]
    fn lens_viewports_stay_distinct() {
        let mut canvas = mere::canvas::Canvas::with_sample_graph();
        canvas.resize(800, 600);
        let a = canvas.viewport();
        // Drive "window B": install, pan, stash.
        canvas.set_viewport(a);
        canvas.wheel(0.0, 240.0);
        let b = canvas.viewport();
        // Restore "window A".
        canvas.set_viewport(a);
        assert_ne!(a, b, "B's wheel moved B's viewport (inertia counts)");
        assert_eq!(canvas.viewport(), a, "A's viewport is untouched");
    }

    /// The drop decode: a real PNG round-trips to a data-URI; a non-image
    /// file declines (and so becomes a node instead of a sprite).
    #[test]
    fn dropped_files_classify_by_decodability() {
        let dir = std::env::temp_dir().join(format!("turnstone-drop-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let png_path = dir.join("drop.png");
        image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255]))
            .save(&png_path)
            .unwrap();
        let (uri, hull) = decode_sprite(&png_path).expect("a png decodes");
        assert!(uri.starts_with("data:image/png;base64,"));
        assert!(hull.len() >= 3, "an opaque png traces a collider hull");
        let txt_path = dir.join("drop.txt");
        std::fs::write(&txt_path, "not an image").unwrap();
        assert!(decode_sprite(&txt_path).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
