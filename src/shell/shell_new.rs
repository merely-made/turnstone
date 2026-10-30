// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub fn new(proxy: EventLoopProxy<()>, address: Option<String>) -> Self {
        let client_limits = crate::client_limits::BrowserClientLimits::from_environment()
            .expect("Turnstone client limit configuration is invalid");
        let (mut app, boot_effects) = App::boot(address.as_deref());
        match crate::chrome_layout::ChromeLayout::load(&app.data_root) {
            Ok(layout) => {
                let mut chrome = app.shell_chrome_config().clone();
                chrome.layout = layout;
                app.set_shell_chrome_config(chrome);
            },
            Err(error) => tracing::warn!(%error, "chrome placement could not be loaded"),
        }
        let initial_provider = match ApplicationSettingsProvider::load(&app.data_root) {
            Ok(provider) => provider,
            Err(error) => {
                tracing::warn!(%error, "application settings could not be loaded at shell startup");
                ApplicationSettingsProvider::from_failed_load(&app.data_root, error.to_string())
            },
        };
        let live_settings = LiveSettingsHandle::new(initial_provider.settings());
        live_settings.publish_provider(&initial_provider);
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
        let (nomadnet_handle, nomadnet_rx) = nomadnet_fetch::spawn(Arc::clone(&fetch_wake));
        let (micron_submission_handle, micron_submission_rx) =
            nomadnet_fetch::spawn_submissions(Arc::clone(&fetch_wake));
        // The session jar is durable again: loaded here, flushed on each drain.
        let cookie_custody = crate::cookie_custody::CookieCustody::open(&app.data_root)
            .expect("Turnstone could not open its cookie store");
        // The process session's stores: the page actor and every other fetch
        // in this shell share them, so an episode's ranged reads carry the
        // same cookies as the pages (ranged fetch plan, lane T1). Keying the
        // set by persona is lane T2.
        let stores = fetch::session_stores().clone();
        let network_offline = crate::offline::enabled();
        let offline_attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (fetch_handle, fetch_rx) = if network_offline {
            crate::offline::spawn(fetch_wake, offline_attempts.clone())
        } else {
            fetch::spawn_fetcher_with_policy(fetch_wake, stores.clone(), client_limits.fetch)
        };
        let blocking_fetch: Arc<dyn fetch::Fetch> = if network_offline {
            Arc::new(crate::offline::NoNetwork(offline_attempts.clone()))
        } else {
            Arc::new(
                fetch::NetFetch::new(&stores).expect("Turnstone could not build its fetch handle"),
            )
        };

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
        let (bin_handle, bin_rx) = crate::recycle::spawn_bin_for_session(
            bin_wake,
            (!app.session_load_refused()).then(|| crate::recycle::bin_dir(&app.session_dir())),
        );

        // The trail-memory actor over THIS session's memory store (search
        // wiring W1 + W2): browsing capture in, recall answers out, behind
        // the same wake shape.
        let trail_proxy = proxy.clone();
        let trail_wake: armillary::Wake = Arc::new(move || {
            let _ = trail_proxy.send_event(());
        });
        let capture_library = crate::place::captured_collection::LocalCaptureLibrary::default();
        let (trail_handle, trail_rx) = crate::trail_memory::spawn_trail_for_session(
            trail_wake,
            (!app.session_load_refused())
                .then(|| crate::trail_memory::memory_dir(&app.session_dir())),
            capture_library.clone(),
            app.source_capture_generation,
        );
        let trail_handle = Arc::new(trail_handle);

        let place_proxy = proxy.clone();
        let place_wake: armillary::Wake = Arc::new(move || {
            let _ = place_proxy.send_event(());
        });
        let place_settings = crate::place::worker::PlaceWorkerSettings {
            capture_library: capture_library.clone(),
            ..crate::place::worker::PlaceWorkerSettings::default()
        };
        // Identity binding runs off the UI thread and returns only a bounded,
        // epoch-bound route. The place worker waits until that route is ready.
        let (place_handle, place_rx) =
            crate::place_service::configure(place_wake.clone(), place_settings.clone());
        if let Some(root) = &app.identity {
            place_handle.command(crate::place::worker::PlaceWorkerCommand::AdoptIdentity(
                root.clone(),
            ));
        }
        let custody_generation = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let custody_proxy = proxy.clone();
        let custody_wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = custody_proxy.send_event(());
        });
        let custody_setup = match crate::custody_binding::Setup::connect(
            custody_generation.clone(),
            custody_wake.clone(),
        ) {
            Ok((setup, events)) => Some((setup, events)),
            Err(error) => {
                tracing::warn!(%error, "profile custody connection remains pending");
                None
            },
        };
        let mut pending_publish_source = None;

        // The content port's ordinary lanes: both Genet static renderers plus
        // the engine-native smolweb family. Route policy selects one by
        // address/media type without app dispatch.
        let mut content_engines = content_engines_with_client_limits(client_limits);
        let knot_proxy = proxy.clone();
        let knot_wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = knot_proxy.send_event(());
        });
        let mut knot_clip = None;
        let mut knot_notes_root = None;
        let mut publish_service = None;
        let shared_knot_service = app
            .identity
            .clone()
            .filter(|_| !network_offline)
            .and_then(start_share_reader);
        let device_receipts_service =
            match crate::device_receipts_service::DeviceReceiptsService::start() {
                Ok(service) => Some(Arc::new(service)),
                Err(error) => {
                    tracing::warn!(%error, "device receipts reader is unavailable");
                    None
                },
            };
        let knot_visits = crate::knot_authoring::KnotVisits::default();
        #[cfg(feature = "authoring")]
        match crate::knot_authoring::KnotAuthoringEngine::for_profile(
            knot_wake,
            knot_visits.clone(),
            &app.data_root,
        ) {
            Ok(Some(mut engine)) => {
                knot_clip = engine.clip_handle();
                knot_notes_root = engine.notes_root();
                if let Some(source) = engine.take_publish_source() {
                    match app.identity.clone() {
                        Some(root) if !network_offline => {
                            publish_service = start_publishing(source, root)
                        },
                        Some(_) => {},
                        None => pending_publish_source = Some(source),
                    }
                }
                content_engines.register(Box::new(engine));
            },
            Ok(None) => {},
            Err(error) => tracing::warn!(%error, "Knot authoring is unavailable"),
        }

        let clipboard = genet_clipboard::SystemClipboard::new()
            .map(|clipboard| Box::new(clipboard) as Box<dyn genet_clipboard::TextClipboard>)
            .map_err(|error| {
                tracing::warn!(%error, "the system clipboard is unavailable");
                error.to_string()
            });

        let mut surface_providers = crate::contributed_surface::SurfaceProviderRegistry::new();
        surface_providers
            .register_provider(crate::knot_document_surface::KnotDocumentProvider::default())
            .expect("the built-in Knot document provider is unique");
        #[cfg(feature = "sky")]
        surface_providers
            .register_provider(crate::sky_surface::SkySurfaceProvider::default())
            .expect("the built-in Sky provider is unique");
        #[cfg(feature = "inference")]
        surface_providers
            .register_provider(
                crate::distillery_installed_surface::DistilleryInstalledProvider::default(),
            )
            .expect("the built-in Distillery installed provider is unique");
        // Redshank's own compact dock, admitted rather than copied. The shell
        // holds the dock handles; the app holds the one listening authority,
        // which is where the device is opened.
        app.sync_research_context();
        let answers_proxy = proxy.clone();
        let answers_wake: armillary::Wake = Arc::new(move || {
            let _ = answers_proxy.send_event(());
        });
        #[cfg(feature = "inference")]
        surface_providers
            .register_provider(crate::inference_surface::AnswerProvider::new(
                Arc::new(crate::trail_memory::CaptureInferenceOwner {
                    trail: trail_handle.clone(),
                    bridge: app.research.clone(),
                }),
                answers_wake,
                app.research.clone(),
            ))
            .expect("the answer provider is unique");
        let redshank_docks = crate::redshank_episode_surface::RedshankDocks::new();
        surface_providers
            .register_provider(
                crate::redshank_episode_surface::RedshankEpisodeProvider::new(
                    redshank_docks.clone(),
                ),
            )
            .expect("the Redshank episode provider is unique");
        if app.session_load_refused() {
            app.redshank = crate::redshank_host::RedshankHost::detached(
                crate::redshank_host::Output::Device,
                blocking_fetch,
            );
        } else {
            app.redshank = crate::redshank_host::RedshankHost::open(
                &app.session_dir(),
                crate::redshank_host::Output::Device,
                blocking_fetch,
            );
        }
        let redshank_proxy = proxy.clone();
        app.redshank.set_wake(Arc::new(move || {
            let _ = redshank_proxy.send_event(());
        }));
        let mut shell = Self {
            app,
            live_settings,
            proxy,
            actor_wake_pending: false,
            fetch_handle,
            network_offline,
            offline_attempts,
            gemini_trust,
            fetch_rx,
            cookie_custody,
            nomadnet_handle,
            nomadnet_rx,
            micron_submission_handle,
            micron_submission_rx,
            download_handle,
            download_rx,
            bin_handle,
            bin_rx,
            trail_handle,
            trail_rx,
            place_handle,
            place_settings,
            place_wake,
            capture_library,
            custody_trail_ready: None,
            pending_place_reopen: None,
            place_rx,
            custody_generation,
            custody_setup,
            custody_expected_principal: None,
            custody_binding: None,
            retired_setups: Vec::new(),
            retired_watchers: Vec::new(),
            custody_wake,
            pending_publish_source,
            pending_trail: Vec::new(),
            clipboard,
            cursor: (0.0, 0.0),
            ctrl: false,
            alt: false,
            shift: false,
            meta: false,
            shared_scenario: shared_scenario_from_env(),
            scenario_queued_action: None,
            pending_scenario_wait: None,
            held_contributed_click: None,
            contributed_click_request: 0,
            observed_events: VecDeque::with_capacity(128),
            diagnostic_observations:
                crate::diagnostic_observations::DiagnosticObservations::from_env(),
            shared_out_dir: shared_out_dir_from_env(),
            pending_capture: None,
            pending_capture_readback: None,
            pending_native_diagnostics: Vec::new(),
            main_receipt: Default::default(),
            pending_lens_capture: None,
            pending_lens_capture_window: None,
            window: None,
            a11y_adapter: None,
            a11y_routes: crate::a11y::A11yRoutes::new(),
            a11y_actions: std::sync::Arc::new(std::sync::Mutex::new(Default::default())),
            a11y_epoch: None,
            a11y_surface_layout: Vec::new(),
            surface_a11y: crate::foreign_a11y::ForeignA11y::new(),
            host: None,
            render_recovery: None,
            primary_recovery: None,
            render_recovery_blocked: None,
            width: 1024,
            height: 600,
            content_engines: Arc::new(content_engines),
            pelt_surface_engines: Arc::new(inker::SurfaceEngineRegistry::new()),
            content_sessions: NodeDocuments::default(),
            cue_frames: Default::default(),
            surface_engines: inker::SurfaceEngineRegistry::new(),
            surface_poll_clock: surface_poll::SurfacePollClock::new(
                surface_poll_interval_from_env(),
            ),
            document_clock: content_pool::DocumentClock::new(surface_poll_interval_from_env()),
            primary_focused: true,
            physics_display_rate_last_observed: None,
            surface_find_requests: std::collections::HashMap::new(),
            page_captures: page_capture::CaptureCorrelation::default(),
            #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
            surface_frames: std::collections::HashMap::new(),
            #[cfg(all(feature = "scry", windows))]
            scry_factory: None,
            #[cfg(all(
                feature = "servo",
                any(windows, target_os = "macos", target_os = "linux")
            ))]
            servo_factory: None,
            #[cfg(all(feature = "scry", windows))]
            scry_frame_importers: std::collections::HashMap::new(),
            pending_surface_spawns: Vec::new(),
            knot_visits,
            pending_visit_spawns: Vec::new(),
            next_visit_request: 1,
            knot_visit_refusals: Vec::new(),
            knot_clip,
            knot_notes_root,
            clip_offers: HashMap::new(),
            route_policy: standard_route_policy(),
            web_policy,
            epoch: std::time::Instant::now(),
            pending_fetches: browse::PendingFetches::default(),
            pointer_capture: None,
            contributed_pointer: None,
            surface_pointer_buttons: inker::PointerButtons::NONE,
            touch_input: window_input::TouchInput::default(),
            input_window: 0,
            window_focus: HashMap::new(),
            window_buttons: HashMap::new(),
            host_file_drag: HostFileDrag::default(),
            hovered_surface: None,
            content_scroll_moved: None,
            publish_service,
            shared_knot_service,
            device_receipts_service,
            renderers: Default::default(),
            outer_frames: Default::default(),
            outer_capture: None,
            outer_focus: None,
            titlebar_clicks: HashMap::new(),
            surface_providers,
            redshank_docks,
            hovered_pane: None,
            chrome: crate::chrome_view::ChromeSurfaces::new(),
            wb_tab_drag: None,
            wb_divider_drag: None,
            divider_drag: None,
            lens_divider_drag: None,
            lens_windows: std::collections::HashMap::new(),
            pending_windows: Vec::new(),
            theme_editor: None,
            theme_editor_requested: false,
            close_after_theme_editor: false,
            alembic_workshop: None,
            alembic_workshop_requested: false,
            close_after_alembic_workshop: false,
        };
        shell.publish_engine_inventory();
        shell.run_effects(boot_effects);
        shell
    }

    pub(super) fn has_live_content(&self, node: &uuid::Uuid) -> bool {
        self.content_sessions.contains_key(node)
    }

    pub(super) fn has_independent_reader_appearance(&self, node: &uuid::Uuid) -> bool {
        self.content_sessions.session(node).is_some_and(|session| {
            session
                .as_any_ref()
                .is::<mere_document_lanes::ReaderDocumentSession>()
        })
    }

    pub(super) fn clear_surface_content(&mut self) {
        self.cancel_content_pointer();
        self.cue_frames.clear();
        self.content_sessions.retain_appearances(|_, _| false);
        self.clear_surface_semantics();
        self.content_sessions
            .retain(|_, content| content.lane() != pelt_core::PeltLane::Surface);
        self.surface_find_requests.clear();
        self.page_captures.clear_surfaces();
        #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
        self.surface_frames.clear();
        #[cfg(all(feature = "scry", windows))]
        self.scry_frame_importers.clear();
    }

    pub(super) fn clear_reader_appearances(&mut self, node: uuid::Uuid) {
        self.content_sessions.clear_appearances(node);
    }
}
