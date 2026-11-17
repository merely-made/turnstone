// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::{RemoteFetcher, Scene, SessionRegistry, ShellEvent};
use genet_documents::{LocalFetcher, ScriptWake, ScriptedSessionEngine};
use genet_render_host::webgl::WebGlHost;
use std::sync::{Arc, Mutex};
use winit::event_loop::EventLoopProxy;

#[derive(Clone, Copy)]
enum ScriptedEngine {
    Bole,
    #[cfg(target_pointer_width = "64")]
    Vano,
}

#[derive(Clone)]
pub(super) struct ScriptedWebGl {
    engine: ScriptedEngine,
    host: Arc<Mutex<Option<WebGlHost>>>,
}

impl ScriptedWebGl {
    pub(super) fn from_environment() -> Option<Self> {
        let engine = match std::env::var("TURNSTONE_GENET_ENGINE").as_deref() {
            Ok("bole") => ScriptedEngine::Bole,
            #[cfg(target_pointer_width = "64")]
            Ok("vano") => ScriptedEngine::Vano,
            #[cfg(not(target_pointer_width = "64"))]
            Ok("vano") => panic!("Turnstone's Vano engine requires a 64-bit target"),
            _ => return None,
        };
        Some(Self {
            engine,
            host: Default::default(),
        })
    }

    pub(super) fn engine_id(&self) -> &'static str {
        match self.engine {
            ScriptedEngine::Bole => document_session_api::engine_ids::ENGINE_GENET_SCRIPTED,
            #[cfg(target_pointer_width = "64")]
            ScriptedEngine::Vano => document_session_api::engine_ids::ENGINE_GENET_SCRIPTED_NOVA,
        }
    }

    pub(super) fn set_device(&self, handles: &netrender::WgpuHandles) -> Result<(), &'static str> {
        let mut current = self.host.lock().expect("WebGL host lock");
        if current
            .as_ref()
            .is_none_or(|host| !host.matches_device(&handles.device, &handles.queue))
        {
            if current
                .as_ref()
                .is_some_and(|host| host.live_contexts() != 0)
            {
                return Err("Turnstone's WebGL contexts still use the previous render device");
            }
            *current = Some(WebGlHost::new(
                handles.device.clone(),
                handles.queue.clone(),
            ));
        }
        Ok(())
    }

    pub(super) fn host(&self) -> Option<WebGlHost> {
        self.host.lock().expect("WebGL host lock").clone()
    }

    pub(super) fn detach_if_idle(&self) -> bool {
        let mut current = self.host.lock().expect("WebGL host lock");
        if current
            .as_ref()
            .is_some_and(|host| host.live_contexts() != 0)
        {
            return false;
        }
        *current = None;
        true
    }

    pub(super) fn register(
        &self,
        engines: &mut SessionRegistry<Scene>,
        remote: RemoteFetcher,
        proxy: EventLoopProxy<ShellEvent>,
    ) {
        let wake = ScriptWake::new();
        wake.set_callback(move |_| {
            let _ = proxy.send_event(ShellEvent::ActorWake);
        });
        let device = self.clone();
        let options = move |_: &str| {
            if device.host().is_none() {
                return Err("Turnstone's WebGL device is not ready".into());
            }
            let device = device.clone();
            Ok(genet_scripted::ScriptedDocumentOptions {
                webgl: Some(Box::new(move |w, h| device.host()?.create_handler(w, h))),
                ..Default::default()
            })
        };
        let fetcher = LocalFetcher.with_fallback(remote);
        match self.engine {
            ScriptedEngine::Bole => engines.register(Box::new(
                ScriptedSessionEngine::<script_engine_boa::BoaEngine, _>::new_with_wake(
                    self.engine_id(),
                    fetcher,
                    wake,
                )
                .with_options_factory(options),
            )),
            #[cfg(target_pointer_width = "64")]
            ScriptedEngine::Vano => engines.register(Box::new(
                ScriptedSessionEngine::<script_engine_nova::NovaEngine, _>::new_with_wake(
                    self.engine_id(),
                    fetcher,
                    wake,
                )
                .with_options_factory(options),
            )),
        }
    }
}
