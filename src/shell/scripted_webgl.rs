// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::{RemoteFetcher, Scene, SessionRegistry, ShellEvent};
use document_session_api::engine_ids::ENGINE_GENET_SCRIPTED;
#[cfg(target_pointer_width = "64")]
use document_session_api::engine_ids::ENGINE_GENET_SCRIPTED_NOVA;
use genet_documents::{LocalFetcher, ScriptWake, ScriptedSessionEngine};
use genet_render_host::webgl::WebGlHost;
use std::sync::{Arc, Mutex};
use winit::event_loop::EventLoopProxy;

pub(super) fn default_engine_from_environment() -> Option<&'static str> {
    match std::env::var("TURNSTONE_GENET_ENGINE").as_deref() {
        Ok("bole") => Some(ENGINE_GENET_SCRIPTED),
        #[cfg(target_pointer_width = "64")]
        Ok("vano") => Some(ENGINE_GENET_SCRIPTED_NOVA),
        #[cfg(not(target_pointer_width = "64"))]
        Ok("vano") => panic!("Turnstone's Vano engine requires a 64-bit target"),
        _ => None,
    }
}

pub(super) fn is_scripted_engine(id: &str) -> bool {
    match id {
        ENGINE_GENET_SCRIPTED => true,
        #[cfg(target_pointer_width = "64")]
        ENGINE_GENET_SCRIPTED_NOVA => true,
        _ => false,
    }
}

#[derive(Clone, Default)]
pub(super) struct ScriptedWebGl(Arc<Mutex<Option<WebGlHost>>>);

impl ScriptedWebGl {
    pub(super) fn set_device(&self, handles: &netrender::WgpuHandles) -> Result<(), &'static str> {
        let mut current = self.0.lock().expect("WebGL host lock");
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
        self.0.lock().expect("WebGL host lock").clone()
    }

    pub(super) fn detach_if_idle(&self) -> bool {
        let mut current = self.0.lock().expect("WebGL host lock");
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
        // Each engine numbers its own document generations, so their wake
        // tracking needs a separate owner even though both wake this shell.
        let wake = || {
            let wake = ScriptWake::new();
            let proxy = proxy.clone();
            wake.set_callback(move |_| {
                let _ = proxy.send_event(ShellEvent::ActorWake);
            });
            wake
        };
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
        engines.register(Box::new(
            ScriptedSessionEngine::<script_engine_boa::BoaEngine, _>::new_with_wake(
                ENGINE_GENET_SCRIPTED,
                fetcher.clone(),
                wake(),
            )
            .with_options_factory(options.clone()),
        ));
        #[cfg(target_pointer_width = "64")]
        engines.register(Box::new(
            ScriptedSessionEngine::<script_engine_nova::NovaEngine, _>::new_with_wake(
                ENGINE_GENET_SCRIPTED_NOVA,
                fetcher,
                wake(),
            )
            .with_options_factory(options),
        ));
    }
}
