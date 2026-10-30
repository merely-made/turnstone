// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The turnstone shell: app state plus the window, present stack, and ports
/// that drive it.
pub struct Shell {
    pub(super) app: App,
    /// The shell-owned live value projection. Settings panes receive clones;
    /// the shell alone polls and applies the typed chrome snapshot.
    pub(super) live_settings: LiveSettingsHandle,
    /// Wakes the loop when the physics or fetch actor has news.
    pub(super) proxy: EventLoopProxy<crate::shell::ShellEvent>,
    /// Proxy callbacks admit one actor drain at the end of the native batch.
    pub(super) actor_wake_pending: bool,
    pub(super) native_frames: super::native_frames::NativeFrameQueue,
    pub(super) native_frame_wake: armillary::Wake,
    /// The fetch actor's command handle; dropping it ends the actor.
    pub(super) fetch_handle: armillary::ActorHandle<FetchCommand>,
    pub(super) network_offline: bool,
    pub(super) offline_attempts: Arc<std::sync::atomic::AtomicUsize>,
    /// Durable Gemini server pins. The protocol verifier shares this exact
    /// instance; the shell retains it for explicit change acceptance.
    pub(super) gemini_trust: Arc<crate::gemini_trust::GeminiTrustStore>,
    /// Completed fetches, drained after each notified native event batch.
    pub(super) fetch_rx: Receiver<FetchUpdate>,
    /// The session jar's durable home, flushed after each drain.
    pub(super) cookie_custody: crate::cookie_custody::CookieCustody,
    pub(super) nomadnet_handle: armillary::demand::DemandActor<FetchCommand, FetchUpdate>,
    pub(super) nomadnet_rx: armillary::demand::DemandUpdates<FetchCommand, FetchUpdate>,
    pub(super) micron_submission_handle:
        armillary::demand::DemandActor<nomadnet_fetch::MicronSubmissionCommand, Update>,
    pub(super) micron_submission_rx:
        armillary::demand::DemandUpdates<nomadnet_fetch::MicronSubmissionCommand, Update>,
    /// Serialized download custody writes, kept off the event-loop thread.
    pub(super) download_handle: armillary::ActorHandle<crate::download::DownloadCommand>,
    /// Completed custody writes, drained beside fetch answers.
    pub(super) download_rx: Receiver<Update>,
    /// The recycle-bin actor (the eidetic deleted-node bin at the session's
    /// bin dir); commands stage records / re-point on a session switch.
    pub(super) bin_handle: armillary::ActorHandle<crate::recycle::BinCommand>,
    /// The bin's answers (BinListed / BinFailed), drained beside the fetches.
    pub(super) bin_rx: Receiver<Update>,
    /// The trail-memory actor (browsing capture into the session's memory
    /// store; search wiring W1). Fed from the semantic-event drain; re-pointed
    /// and released on the same session edges as the bin.
    pub(super) trail_handle: Arc<armillary::ActorHandle<crate::trail_memory::TrailCommand>>,
    /// The trail's recall answers (RecallHits / RecallFailed), drained beside
    /// the bin's.
    pub(super) trail_rx: Receiver<Update>,
    pub(super) research_worker: crate::research_search::Worker,
    /// The retained-place worker. It owns every Gemot, Commons, chat, group,
    /// and redb handle for the active session.
    pub(super) place_handle: crate::place_service::PlacePort,
    pub(super) place_settings: crate::place::worker::PlaceWorkerSettings,
    pub(super) place_wake: armillary::Wake,
    pub(super) capture_library: crate::place::captured_collection::LocalCaptureLibrary,
    pub(super) custody_trail_ready: Option<Receiver<()>>,
    pub(super) pending_place_reopen:
        Option<(crate::panes::SessionId, u64, crate::place::PlaceBindingV1)>,
    /// Generation-tagged app-owned answers from the place worker.
    pub(super) place_rx: crate::place_service::PlaceUpdates,
    pub(super) custody_generation: Arc<std::sync::atomic::AtomicU64>,
    pub(super) custody_setup: Option<(
        crate::custody_binding::Setup,
        Receiver<crate::custody_binding::SetupEvent>,
    )>,
    pub(super) custody_expected_principal: Option<crate::identity::CustodyPrincipal>,
    pub(super) custody_binding: Option<PrivateBinding>,
    pub(super) retired_setups: Vec<crate::custody_binding::Setup>,
    pub(super) retired_watchers: Vec<crate::custody_binding::Watcher>,
    pub(super) custody_wake: Arc<dyn Fn() + Send + Sync>,
    /// Knot's publish source, held while the identity is pending: the
    /// publishing service speaks as the profile; unlock configures its owner.
    pub(super) pending_publish_source: Option<crate::publish_service::PublishSource>,
    /// Visits made while the identity is pending, filed under the root at
    /// unlock (at most [`PENDING_TRAIL_CAP`]).
    pub(super) pending_trail: Vec<(String, eidetic::TraceTransition, u64)>,
    /// The system clipboard, opened once at startup: the handle holds X11 and
    /// Wayland selection ownership for its lifetime. `Err` carries why it is
    /// unreachable, so a copy refuses out loud rather than panicking. It stays
    /// here, behind the effect seam; App and the place worker never see one.
    pub(super) clipboard: Result<Box<dyn genet_clipboard::TextClipboard>, String>,
    /// Last cursor position in physical px. winit's `MouseInput` carries no
    /// position, so the shell tracks it from `CursorMoved`.
    pub(super) cursor: (f32, f32),
    /// Live Ctrl state, for the omnibar summon chords (Ctrl+L / Ctrl+K).
    pub(super) ctrl: bool,
    /// Live Alt state, for the nav chords (Alt+Left / Alt+Right).
    pub(super) alt: bool,
    /// Live Shift state, for the tear-out modifier arms (Ctrl+Shift = fork).
    pub(super) shift: bool,
    pub(super) meta: bool,
    /// The taproot scenario driver (activated by `TURNSTONE_SCENARIO`): the
    /// generic one-step-per-frame loop every genet app shares, driving this
    /// Shell through its
    /// `Automatable`/`Driveable` impl — the one scenario loop turnstone runs.
    /// `shared_out_dir` stays
    /// on `self` (the scenario is taken out during a tick) so `capture` can reach
    /// it. `shared_done` guards writing the sentinel exactly once.
    pub(super) shared_scenario: Option<taproot::Scenario>,
    /// Exact action copied from the common catalog for scenario stale-source
    /// controls; delivery still lowers through the ordinary shell action path.
    pub(super) scenario_queued_action: Option<Action>,
    pub(super) pending_scenario_wait: Option<drive::PendingScenarioWait>,
    pub(super) held_contributed_click: Option<contributed_automation::HeldClick>,
    pub(super) contributed_click_request: u64,
    /// A bounded copy of semantic events after their ordinary consumers have
    /// received them. Automation drains this copy, so its assertions do not
    /// compete with trail memory for the app's one event stream.
    pub(super) observed_events: VecDeque<String>,
    pub(super) address_event_history: super::citation_event_history::AddressEventHistory,
    pub(super) diagnostic_observations: crate::diagnostic_observations::DiagnosticObservations,
    pub(super) shared_out_dir: std::path::PathBuf,
    /// A capture the next `render` fulfills from the very views it presents
    /// (never a re-rasterization — the receipt must be the presented frame).
    pub(super) pending_capture: Option<std::path::PathBuf>,
    pub(super) pending_capture_readback: Option<appearance_receipt::PendingCapture>,
    pub(super) pending_native_diagnostics: Vec<appearance_receipt::PendingCapture>,
    pub(super) main_receipt: appearance_receipt::MainReceipt,
    /// A capture the next LENS render fulfills (the scenario's capture-lens
    /// verb; targets the first live lens window).
    pub(super) pending_lens_capture: Option<std::path::PathBuf>,
    pub(super) pending_lens_capture_window: Option<WindowId>,
    pub(super) window: Option<Arc<Window>>,
    /// The OS AccessKit bridge for the primary window, when installed.
    pub(super) a11y_adapter: Option<genet_winit_host::AccessKitBridge>,
    /// Routes for the current host tree. Document admissions are captured
    /// separately at enqueue and revalidated against their live source.
    pub(super) a11y_routes: crate::a11y::A11yRoutes,
    /// Assistive actions queued by the platform thread, drained here.
    pub(super) a11y_actions: crate::shell::a11y_bridge::ActionQueue,
    /// Activation occurs on a platform thread; replay subtrees on the UI thread.
    pub(super) a11y_epoch: Option<u64>,
    pub(super) a11y_surface_layout: Vec<(uuid::Uuid, accesskit::Rect)>,
    pub(super) surface_a11y: crate::foreign_a11y::ForeignA11y,
    pub(super) host: Option<SurfaceHost>,
    pub(super) render_recovery: Option<Arc<genet_render_host::SharedRenderCore>>,
    pub(super) primary_recovery: Option<WindowRecovery>,
    pub(super) render_recovery_blocked: Option<String>,
    pub(super) width: u32,
    pub(super) height: u32,
    /// The content port (rung 4, session-engines plan phase 4): the session
    /// registry does the engine-id dispatch, and the live sessions — retained,
    /// non-Send handles — live here, keyed by the same node ids App's
    /// ContentStates tracks. Ports own handles; App holds data.
    /// Shared with every node's Pelt controller, which spawns from it.
    pub(super) content_engines: Arc<SessionRegistry<netrender::Scene>>,
    /// Pelt's surface registry for document controllers. A node's web surface
    /// is spawned from `surface_engines` below and held as the surface lane
    /// of its content.
    pub(super) pelt_surface_engines: Arc<inker::SurfaceEngineRegistry>,
    pub(super) content_sessions: NodeDocuments,
    pub(super) cue_frames: cue_content::CueFrames,
    /// Long-lived frame-streaming engines. The neutral inker registry chooses
    /// the producer; each node's content holds its non-Send live handle, and
    /// the shell keeps the imported frame cache.
    pub(super) surface_engines: inker::SurfaceEngineRegistry,
    /// Mailbox fallback cadence until producers expose an event-loop wake hook.
    pub(super) surface_poll_clock: surface_poll::SurfacePollClock,
    pub(super) document_clock: content_pool::DocumentClock,
    pub(super) primary_focused: bool,
    /// Last monitor-rate observation on the existing event/render cadence.
    pub(super) physics_display_rate_last_observed: Option<std::time::Instant>,
    /// Exact app request/query identity for progressive hosted find results.
    pub(super) surface_find_requests: std::collections::HashMap<uuid::Uuid, (u64, String)>,
    pub(super) page_captures: page_capture::CaptureCorrelation,
    #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
    pub(super) surface_frames:
        std::collections::HashMap<uuid::Uuid, Option<surface_frames::ImportedSurfaceFrame>>,
    #[cfg(all(feature = "scry", windows))]
    pub(super) scry_factory: Option<Arc<scry::TurnstoneScryFactory>>,
    #[cfg(all(
        feature = "servo",
        any(windows, target_os = "macos", target_os = "linux")
    ))]
    pub(super) servo_factory: Option<Arc<servo::TurnstoneServoFactory>>,
    #[cfg(all(feature = "scry", windows))]
    pub(super) scry_frame_importers:
        std::collections::HashMap<uuid::Uuid, surface_frames::ScryingFrameImporter>,
    /// A restored Weld-pinned node can request content before winit has created
    /// its device. Keep it requested until `resumed` instead of falsely
    /// reporting an unavailable engine.
    pub(super) pending_surface_spawns: Vec<(uuid::Uuid, String)>,
    /// One hub per visited holder, plus this profile's own root. Shared with
    /// the Knot authoring engine, which was boxed into the content registry
    /// long before any place was joined.
    pub(super) knot_visits: crate::knot_authoring::KnotVisits,
    /// Content spawns waiting on a dial to the mere that holds them, by
    /// holder root. Replayed through the ordinary spawn effect once the visit
    /// is open, exactly as a Weld-pinned node waits for its device.
    pub(super) pending_visit_spawns: Vec<([u8; 32], uuid::Uuid, String)>,
    /// Correlates a visit answer with the request that asked for it.
    pub(super) next_visit_request: u64,
    /// Place-held documents this profile was refused, kept so observation can
    /// say `refused` where there is no surface to ask.
    pub(super) knot_visit_refusals: Vec<crate::observe::KnotDocumentFacts>,
    /// Configured Knot destination for typed Inspector clips. The handle owns
    /// neither file authority nor vault keys; it only queues endpoint intents.
    pub(super) knot_clip: Option<crate::knot_authoring::KnotClipHandle>,
    pub(super) knot_notes_root: Option<std::path::PathBuf>,
    pub(super) clip_offers: HashMap<crate::panes::PaneId, research_notes::ClipOffer>,
    /// Mere's routing vocabulary over inker's engine rules: address -> engine id.
    pub(super) route_policy: inker::EngineRoutePolicy,
    /// Profile/origin user-agent policy, pending prompts, and the process-only
    /// credential provider. Graph data receives summaries, never secrets.
    pub(super) web_policy: crate::web_policy::WebPolicyService,
    /// Monotonic epoch for the sessions' pump clock.
    pub(super) epoch: std::time::Instant,
    /// In-flight fetch correlation: which node asked for each URL, noted
    /// before commanding the actor, reattached by the adapter on completion.
    pub(super) pending_fetches: browse::PendingFetches,
    /// The surface a pointer press landed on, held until release (rung 5 slice
    /// B). Pointer routing captures on press so a press-drag-release stays with
    /// one surface: the canvas needs paired `pointer_down`/`pointer_up`, and a
    /// content click must not leak its release to the canvas beneath.
    pub(super) pointer_capture: Option<crate::surface::SurfaceKind>,
    pub(super) contributed_pointer: Option<input::ContributedPointer>,
    /// DOM `buttons` state for the mouse pointer. Pointer Events carries the
    /// full post-event mask, including on move, so CEF can distinguish hover
    /// from a pressed drag.
    pub(super) surface_pointer_buttons: inker::PointerButtons,
    /// Winit touch ids are u64 and may be reused after a contact ends. Live
    /// contacts receive compact, collision-free DOM/CEF pointer ids and each
    /// retains its own content-surface capture until Up or Cancel.
    pub(super) touch_input: window_input::TouchInput,
    pub(super) input_window: u64,
    pub(super) window_focus: HashMap<u64, crate::surface::FocusTarget>,
    pub(super) window_buttons: HashMap<u64, inker::PointerButtons>,
    /// Files offered by the OS drag manager and the current web-surface target.
    /// Graph drops keep using the addressed-content import path.
    pub(super) host_file_drag: HostFileDrag,
    /// The frame-streaming content surface currently under the pointer. This
    /// scopes cursor-shape callbacks and restores the host default on leave.
    pub(super) hovered_surface: Option<uuid::Uuid>,
    /// Whether the last scroll key delivered to focused content actually moved
    /// the page (`Some(true/false)`), or `None` if no content scroll key has
    /// been delivered. A probe for the scenario runner: it lets a receipt
    /// assert both that a page scrolled AND that an idempotent end (PageUp at
    /// the top) is honestly a no-op, so the receipt proves real offset
    /// semantics rather than a method that always returns true.
    pub(super) content_scroll_moved: Option<bool>,
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
    /// Inspector also retains the followed object's viewer controls.
    /// The application-settings projection over the host provider. Retained
    /// like the other Cambium panes.
    /// Owner controls for the active retained Knot publishing service.
    /// The service owns carrier, vault read handle, tickets, and revocations;
    /// the pane only projects and commands it.
    pub(super) publish_service: Option<Arc<crate::publish_service::KnotPublishingService>>,
    /// Recipient controls for a private ticket. This service does not need a
    /// local Knot authoring host, only the profile root it derives from.
    pub(super) shared_knot_service:
        Option<Arc<crate::share_reader_service::KnotShareReaderService>>,
    pub(super) device_receipts_service:
        Option<Arc<crate::device_receipts_service::DeviceReceiptsService>>,
    /// The Overmap pane (O1): the switcher as a graph view, retained like the
    /// Gloss minimap it mirrors.
    /// Every retained per-pane Cambium renderer, keyed by `PaneId`.
    /// See [`renderers::PaneRenderers`] for why they live in one place.
    pub(super) renderers: renderers::PaneRenderers,
    pub(super) outer_frames: std::cell::RefCell<HashMap<usize, crate::outer_frame::OuterFrame>>,
    pub(super) outer_capture: Option<outer_workspace::OuterCapture>,
    pub(super) outer_focus: Option<frame_keys::HeaderFocus>,
    pub(super) titlebar_clicks: HashMap<usize, cambium_genet_winit_host::ClickCadence>,
    /// Runtime product surface factories. Concrete product state never enters
    /// the shell; admission yields the erased sessions retained above.
    pub(super) surface_providers: crate::contributed_surface::SurfaceProviderRegistry,
    /// Every mounted Redshank dock, by library identity. `!Send` like the
    /// other retained product state, so it lives here rather than in App;
    /// the app owns the model, the store and the one audio runtime.
    pub(super) redshank_docks: crate::redshank_episode_surface::RedshankDocks,
    /// Which pane the pointer is hovering (pane pointer-move routing): lets a
    /// move off a pane deliver its Leave so hover emphasis clears.
    pub(super) hovered_pane: Option<crate::panes::PaneId>,
    /// The chrome, as a cambium view over a FOREST of window-roots (one
    /// shared document, one projection per window): retained + diffed, row
    /// clicks live, lens windows carry the caption chip. Replaces the
    /// hand-built `ui::chrome_scene`.
    pub(super) chrome: crate::chrome_view::ChromeSurfaces,
    /// A workbench tab drag in flight, scoped to its source pane. Two
    /// Workbenches can be visible at once, so a member id alone cannot choose
    /// which Forme arrangement a release mutates.
    pub(super) wb_tab_drag: Option<(crate::panes::PaneId, uuid::Uuid)>,
    /// A workbench divider drag in flight: the pressed band plus the pane's
    /// window origin (the walk is pane-local; pointer deliveries are window
    /// coords).
    pub(super) wb_divider_drag: Option<(crate::workbench_tiling::WbDivider, (f32, f32))>,
    /// The divider drag in flight: the pressed seam's placement, held from
    /// press to release (like `pointer_capture`, which also points at it).
    /// Cursor moves turn into ratios through cambium's `Split::ratio_at` —
    /// the component owns the gesture math; the shell only feeds it points.
    pub(super) divider_drag: Option<crate::pane::DividerPlacement>,
    /// A LENS window's seam drag in flight: which lens (ordinal) plus the
    /// pressed seam's placement in that window's tiling. Moves lower
    /// `SetSplitRatio` aimed at the lens's space; release persists once.
    pub(super) lens_divider_drag: Option<(usize, crate::pane::DividerPlacement)>,
    /// Lens windows (rung 7, one-state-N-windows): the same graph through a
    /// window-owned camera. The primary window keeps the full pane/chrome
    /// experience; each lens renders the canvas with ITS `Viewport` installed
    /// around the pass and stashed back after — two windows on one graph hold
    /// distinct cameras over shared node positions (the canvas's install
    /// seam, exactly as the multi-window doctrine recorded).
    pub(super) lens_windows: std::collections::HashMap<WindowId, LensWindow>,
    /// Lens windows requested but not yet created (window creation needs the
    /// `ActiveEventLoop`, which effects don't carry; the event handlers drain
    /// this while one is in scope).
    pub(super) pending_windows: Vec<usize>,
    pub(super) theme_editor: Option<appearance_editor::EditorWindow>,
    pub(super) theme_editor_requested: bool,
    pub(super) close_after_theme_editor: bool,
    pub(super) alembic_workshop: Option<alembic_workshop::WorkshopWindow>,
    pub(super) alembic_workshop_requested: bool,
    pub(super) close_after_alembic_workshop: bool,
}
