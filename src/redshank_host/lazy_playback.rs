// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl RedshankHost {
    pub fn set_wake(&mut self, wake: armillary::Wake) {
        if let Some(runtime) = self.runtime() {
            runtime.set_wake(wake.clone());
        }
        self.playback_wake = Some(wake);
    }

    pub(super) fn ensure_runtime(&self) -> Option<&PlaybackRuntime> {
        if let Some(runtime) = self.runtime() {
            return Some(runtime);
        }
        if self.output != Output::Device {
            return None;
        }
        let fetch = self.fetch.as_ref()?;
        #[cfg(feature = "audio")]
        {
            Some(self.runtime.get_or_init(|| {
                let held = self.retained_voice.clone();
                let blobs: redshank_playback::HostBlobReader = Arc::new(move |id| {
                    let mut held = held
                        .lock()
                        .map_err(|_| "Retained voice buffer unavailable")?;
                    if held.as_ref().is_none_or(|(expected, _)| expected != id) {
                        return Err("Retained voice body is not admitted".into());
                    }
                    Ok(held.take().expect("checked").1)
                });
                let runtime = PlaybackRuntime::start_with_blobs(fetch.clone(), Some(blobs));
                // Passive controls update the retained model before a runtime
                // exists. Install them in order before the first media command.
                let _ = runtime.command(PlaybackCommand::SetRate(
                    self.model.settings.playback_rate_percent,
                ));
                let _ = runtime.command(PlaybackCommand::SetVolume(
                    self.model.settings.volume_percent,
                ));
                if let Some(wake) = &self.playback_wake {
                    runtime.set_wake(wake.clone());
                }
                runtime
            }))
        }
        #[cfg(not(feature = "audio"))]
        {
            let _ = fetch;
            None
        }
    }
}
