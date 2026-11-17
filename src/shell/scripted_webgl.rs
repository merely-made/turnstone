// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::{RemoteFetcher, Scene, SessionRegistry, ShellEvent};
use genet_documents::{LocalFetcher, ScriptWake, ScriptedSessionEngine};
use genet_render_host::webgl::WebGlHost;
use std::sync::{Arc, Mutex};
use winit::event_loop::EventLoopProxy;

pub(super) const ENGINE_ID: &str = "genet.scripted";

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
        let wake = ScriptWake::new();
        wake.set_callback(move |_| {
            let _ = proxy.send_event(ShellEvent::ActorWake);
        });
        let device = self.clone();
        let engine = ScriptedSessionEngine::<script_engine_boa::BoaEngine, _>::new_with_wake(
            ENGINE_ID,
            LocalFetcher.with_fallback(remote),
            wake,
        )
        .with_options_factory(move |_| {
            if device.host().is_none() {
                return Err("Turnstone's WebGL device is not ready".into());
            }
            let device = device.clone();
            Ok(genet_scripted::ScriptedDocumentOptions {
                webgl: Some(Box::new(move |w, h| device.host()?.create_handler(w, h))),
                ..Default::default()
            })
        });
        engines.register(Box::new(engine));
    }
}
